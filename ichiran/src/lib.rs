mod charset;
mod coerce;
mod error;
mod pgdaemon;
mod protocol;
mod server;
pub mod split;

use std::{
    collections::HashMap,
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
use tokio::sync::OnceCell;
use tracing::{Instrument, Level};

use crate::server::IchiranPool;

/// Suggested default pool size: cap at 8 to avoid spinning up more
/// resident `ichiran-cli` workers than there's parallel benefit for
/// (a single parse fans out at most ~20 calls but the longest call
/// caps the critical path).
pub fn default_pool_size() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(8)
}

pub mod prelude {
    pub use crate::charset::*;
    pub use crate::error::*;
    pub use crate::pgdaemon::*;
    pub use crate::protocol::*;
    pub use crate::split::{basic_split, Split};
    pub use crate::*;
}
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
    pool_size: usize,
    state: Mutex<State>,
    pool: OnceCell<IchiranPool>,
}
impl Shared {
    /// Evaluate the expression with ichiran and return the raw output.
    async fn evaluate(&self, expr: impl Into<String>) -> Result<String, IchiranError> {
        let pool = self
            .pool
            .get_or_try_init(|| IchiranPool::spawn(&self.path, self.pool_size))
            .await?;
        pool.evaluate(expr.into()).await
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
        Ok(working_dir.join(jmdict_path.trim()))
    }
}

/// Escape a Rust string into a Common Lisp `"..."` literal. Lisp string
/// syntax only requires escaping `"` and `\`; everything else (including
/// newlines and non-ASCII) is literal.
fn lisp_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

struct State {
    kanji_cache: LruCache<char, Kanji>,
    segment_cache: LruCache<String, Segment>,
    jmdict: Option<JmDictData>,
}

impl Ichiran {
    pub fn new(path: impl Into<PathBuf>, pool_size: usize) -> Self {
        assert!(pool_size >= 1, "pool size must be >= 1");
        Self {
            shared: Arc::new(Shared {
                path: path.into(),
                pool_size,
                state: Mutex::new(State {
                    kanji_cache: LruCache::new(nonzero!(512usize)),
                    segment_cache: LruCache::new(nonzero!(512usize)),
                    jmdict: None,
                }),
                pool: OnceCell::new(),
            }),
        }
    }

    #[tracing::instrument(level = Level::DEBUG, skip_all, err)]
    pub async fn romanize(
        &self,
        splits: &[(Split, String)],
        limit: u32,
    ) -> Result<Root, IchiranError> {
        assert!(limit > 0);

        let shared = self.shared.clone();

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
                                r#"(princ (jsown:to-json (ichiran:romanize* {} :limit {})))"#,
                                lisp_string(&split),
                                limit
                            ))
                            .await?;
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
        let (mut kanji_info, query_chars): (HashMap<char, Kanji>, Vec<char>) = {
            let kanji_cache = &mut self.shared.state.lock().unwrap().kanji_cache;
            let kanji_info = chars
                .iter()
                .filter_map(|c| kanji_cache.get(c).map(|kanji| (*c, kanji.clone())))
                .collect();
            let query_chars = chars
                .iter()
                .filter(|c| !kanji_cache.contains(c))
                .copied()
                .collect();
            (kanji_info, query_chars)
        };

        if query_chars.is_empty() {
            return Ok(kanji_info);
        }

        // Fan out per char so the pool can dispatch them across workers in
        // parallel. Previously these were batched into one (list ...) form
        // which serialized through a single worker.
        let shared = self.shared.clone();
        let span = tracing::Span::current();
        let results: Vec<(char, Option<Kanji>)> = stream::iter(query_chars)
            .par_then_unordered(
                None,
                enclose! { (span, shared) move |c: char| {
                    enclose! { (span, shared) async move {
                        let expr = format!(
                            r#"(princ (jsown:to-json (ichiran/kanji:kanji-info-json #\{})))"#,
                            c
                        );
                        let output = shared.evaluate(expr).await?;
                        let kanji = if output.trim() == "[]" {
                            None
                        } else {
                            Some(serde_json::from_str::<Kanji>(&output)?)
                        };
                        Ok::<_, IchiranError>((c, kanji))
                    }.instrument(span)}
                }},
            )
            .try_collect()
            .await?;

        let kanji_cache = &mut self.shared.state.lock().unwrap().kanji_cache;
        for (chr, maybe_kanji) in results {
            if let Some(kanji) = maybe_kanji {
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
        let parse_error = || IchiranError::Server(format!("parse error:\n{conn_params}"));

        let conn_params = conn_params
            .trim()
            .split(',')
            .collect_tuple()
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
