mod charset;
mod coerce;
mod error;
mod lisp;
mod pgdaemon;
mod protocol;
mod split;

use std::{
    collections::HashMap,
    ffi::OsStr,
    io::{self, ErrorKind},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use enclose::enclose;
use futures::{stream, TryStreamExt};
use itertools::Itertools;
use lru::LruCache;
use nonzero_ext::nonzero;
use par_stream::ParStreamExt;
use tokio::process::Command;
use tracing::{Instrument, Level};

pub mod prelude {
    pub use crate::charset::*;
    pub use crate::error::*;
    pub use crate::pgdaemon::*;
    pub use crate::protocol::*;
    pub use crate::*;
}
use crate::{
    lisp::*,
    split::{basic_split, Split},
};
use prelude::*;

#[derive(Debug)]
pub struct ConnParams {
    pub database: String,
    pub user: String,
    pub password: String,
    pub hostname: String,
    pub port: u16,
}

#[derive(Clone)]
pub struct Ichiran {
    shared: Arc<Shared>,
}

struct Shared {
    path: PathBuf,
    state: Mutex<State>,
}
impl Shared {
    /// Evaluate the expression with ichiran and return the raw output.
    async fn evaluate(&self, expr: impl AsRef<OsStr>) -> Result<String, IchiranError> {
        let working_dir = self.working_dir()?;
        let expr = expr.as_ref();
        tracing::trace!(expr = ?expr);

        let output = Command::new(&self.path)
            .current_dir(working_dir)
            .arg("-e")
            .arg(expr)
            .output()
            .await?;

        if output.status.success() {
            Ok(String::from_utf8(output.stdout).unwrap())
        } else {
            Err(IchiranError::Failure {
                status: output.status,
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            })
        }
    }
    fn working_dir(&self) -> Result<&Path, io::Error> {
        Path::new(&self.path).parent().ok_or_else(|| {
            io::Error::new(
                ErrorKind::NotFound,
                "Could not find working directory of ichiran-cli",
            )
        })
    }
    async fn jmdict_path(&self) -> Result<PathBuf, IchiranError> {
        let working_dir = self.working_dir()?;
        let jmdict_path = self
            .evaluate(r#"(format t "~d" ichiran/dict::*jmdict-data*)"#)
            .await?;
        let jmdict_path = jmdict_path
            .lines()
            .next()
            .ok_or_else(|| IchiranError::Parsing(jmdict_path.clone()))?;
        Ok(working_dir.join(jmdict_path))
    }
}

struct State {
    kanji_cache: LruCache<char, Kanji>,
    segment_cache: LruCache<String, Segment>,
    jmdict: Option<JmDictData>,
}

impl Ichiran {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            shared: Arc::new(Shared {
                path: path.into(),
                state: Mutex::new(State {
                    kanji_cache: LruCache::new(nonzero!(512usize)),
                    segment_cache: LruCache::new(nonzero!(512usize)),
                    jmdict: None,
                }),
            }),
        }
    }

    #[tracing::instrument(level = Level::DEBUG, skip_all, err)]
    pub async fn romanize(&self, text: &str, limit: u32) -> Result<Root, IchiranError> {
        assert!(limit > 0);

        let shared = self.shared.clone();
        let splits: Vec<(Split, String)> = basic_split(text)
            .into_iter()
            .map(|(ty, text)| (ty, text.to_owned()))
            .collect();

        // determine minimal candidate queries from splits
        let split_queries: Vec<_> = splits
            .iter()
            .filter_map(|split| match split {
                (Split::Text, text) => Some(text),
                _ => None,
            })
            .sorted_unstable()
            .unique()
            .cloned()
            .collect();

        let mut segment_table: HashMap<String, Segment> = {
            let segment_cache = &mut shared.state.lock().unwrap().segment_cache;
            // for entries which are in segment cache, use cached value
            split_queries
                .iter()
                .filter_map(|text| {
                    segment_cache
                        .get(text)
                        .map(|segment| (text.clone(), segment.clone()))
                })
                .collect::<HashMap<String, Segment>>()
        };

        let split_queries: Vec<_> = split_queries
            .into_iter()
            .filter(|query| !segment_table.contains_key(query))
            .collect();

        // for entries which are not in the cache, query ichiran
        let span = tracing::Span::current();
        let query_table: HashMap<String, Segment> = stream::iter(split_queries)
            .par_then_unordered(
                None,
                enclose! { (span, shared) move |split: String| {
                    enclose! { (span, shared) async move {
                        let output = shared
                            .evaluate(format!(
                                r#"(jsown:to-json (ichiran:romanize* "{}" :limit {}))"#,
                                lisp_escape_string(split.clone()),
                                limit
                            ))
                            .await?;
                        let output: String = serde_lexpr::from_str(&output)?;
                        let root: Root = serde_json::from_str(&output)?;
                        assert_eq!(
                            root.segments().len(),
                            1,
                            "unexpected number of segments",
                        );
                        let segment = root.segments()[0].clone();
                        Ok::<_, IchiranError>((split.clone(), segment))
                    }.instrument(span)
                }}},
            )
            .try_collect()
            .await?;

        // put queried entries into segment cache
        let segment_cache = &mut shared.state.lock().unwrap().segment_cache;
        for (k, v) in &query_table {
            segment_cache.push(k.clone(), v.clone());
        }
        segment_table.extend(query_table);

        let segments: Vec<_> = splits
            .iter()
            .map(|split| match split {
                (Split::Text, text) => segment_table.get(text).cloned().unwrap(),
                (Split::Skip, skip) => Segment::Skipped(skip.clone()),
            })
            .collect();

        Ok(Root(segments))
    }

    #[tracing::instrument(level = Level::DEBUG, skip_all, err)]
    pub async fn kanji(&self, chars: &[char]) -> Result<HashMap<char, Kanji>, IchiranError> {
        let (mut kanji_info, query_chars): (HashMap<char, Kanji>, Vec<_>) = {
            let kanji_cache = &mut self.shared.state.lock().unwrap().kanji_cache;
            let kanji_info = chars
                .iter()
                .filter_map(|c| kanji_cache.get(c).map(|kanji| (*c, kanji.clone())))
                .collect();
            let query_chars = chars.iter().filter(|c| !kanji_cache.contains(c)).collect();
            (kanji_info, query_chars)
        };

        if query_chars.is_empty() {
            return Ok(kanji_info);
        }

        let commands: Vec<String> = query_chars
            .iter()
            .map(|c| format!(r#"(jsown:to-json (ichiran/kanji:kanji-info-json #\{}))"#, c))
            .collect();

        let expr = format!("(list {})", commands.join(" "));

        let output = self.shared.evaluate(expr).await?;
        let output: Vec<String> = serde_lexpr::from_str(&output)?;

        let kanji_cache = &mut self.shared.state.lock().unwrap().kanji_cache;
        for json in output {
            if json != "[]" {
                let kanji: Kanji = serde_json::from_str(&json)?;
                let chr = kanji.text().chars().next().unwrap();
                kanji_info.insert(chr, kanji.clone());
                kanji_cache.put(chr, kanji);
            }
        }
        Ok(kanji_info)
    }

    pub async fn kanji_from_str(
        &self,
        text: impl AsRef<str>,
    ) -> Result<HashMap<char, Kanji>, IchiranError> {
        let text = text.as_ref();
        let mut uniq: Vec<char> = text.chars().filter(is_kanji).collect();
        uniq.sort_unstable();
        uniq.dedup();
        self.kanji(&uniq).await
    }

    pub async fn jmdict_data(&self) -> Result<JmDictData, IchiranError> {
        {
            let state = self.shared.state.lock().unwrap();
            if let Some(jmdict) = &state.jmdict {
                return Ok(jmdict.clone());
            }
        }

        let jmdict_path = &self.shared.jmdict_path().await?;
        let jmdict = JmDictData::new(jmdict_path).await;

        if let Ok(jmdict) = &jmdict {
            let mut state = self.shared.state.lock().unwrap();
            state.jmdict.replace(jmdict.clone());
        }
        jmdict
    }

    pub async fn conn_params(&self) -> Result<ConnParams, IchiranError> {
        let conn_params = self
            .shared
            .evaluate(r#"(format t "~{~a~^,~}" ichiran/conn::*connection*)"#)
            .await?;
        let parse_error = || IchiranError::Parsing(conn_params.clone());

        let conn_params = conn_params
            .lines()
            .next()
            .map(|x| x.split(','))
            .and_then(|x| x.collect_tuple())
            .ok_or_else(parse_error)?;

        let (database, user, password, hostname, _, port) = conn_params;
        let port = port.parse::<u16>().map_err(|_| parse_error())?;

        Ok(ConnParams {
            database: database.to_string(),
            user: user.to_string(),
            password: password.to_string(),
            hostname: hostname.to_string(),
            port,
        })
    }
}
#[cfg(test)]
mod tests {
    pub(crate) mod fixture;
}
