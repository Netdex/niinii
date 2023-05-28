pub mod charset;
mod coerce;
pub mod error;
pub mod jmdict_data;
pub mod kanji;
mod lisp;
pub mod pgdaemon;
pub mod romanize;

use std::{
    collections::HashMap,
    ffi::OsStr,
    io::{self, ErrorKind},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use lru::LruCache;

use charset::is_kanji;
pub use error::IchiranError;
pub use jmdict_data::JmDictData;
use kanji::Kanji;
use lisp::*;
use romanize::Root;
use tokio::process::Command;

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
struct State {
    kanji_cache: LruCache<char, Kanji>,
    jmdict: Option<JmDictData>,
}

impl Ichiran {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            shared: Arc::new(Shared {
                path: path.into(),
                state: Mutex::new(State {
                    kanji_cache: LruCache::new(512),
                    jmdict: None,
                }),
            }),
        }
    }

    #[tracing::instrument(skip(self), fields(text = text.as_ref()))]
    pub async fn romanize(&self, text: impl AsRef<str>, limit: u32) -> Result<Root, IchiranError> {
        assert!(limit > 0);
        let text = text.as_ref();

        let output = self
            .evaluate(format!(
                r#"(jsown:to-json (ichiran:romanize* "{}" :limit {}))"#,
                lisp_escape_string(text),
                limit
            ))
            .await?;

        let output = lisp_interpret::<String>(&output)?;
        let root: Root = serde_json::from_str(&output)?;
        Ok(root)
    }

    #[tracing::instrument(skip(self))]
    pub async fn kanji(&self, chars: &[char]) -> Result<HashMap<char, Kanji>, IchiranError> {
        let mut kanji_info: HashMap<char, Kanji> = HashMap::new();
        let mut commands: Vec<String> = vec![];

        {
            let mut state = self.shared.state.lock().unwrap();
            chars.iter().for_each(|chr| {
                if let Some(kanji) = state.kanji_cache.get(chr) {
                    kanji_info.insert(*chr, kanji.clone());
                } else {
                    commands.push(format!(
                        r#"(jsown:to-json (ichiran/kanji:kanji-info-json #\{}))"#,
                        chr
                    ));
                }
            });
        }

        if commands.is_empty() {
            return Ok(kanji_info);
        }

        // this is what happens when you don't know lisp
        let expr = format!("(list {})", commands.join(" "));

        let output = self.evaluate(expr).await?;
        let output = format!("'{}", output); // add an apostrophe to turn it into a list
        let output: Vec<String> = lisp_interpret(&output)?;

        let mut state = self.shared.state.lock().unwrap();
        output.iter().try_for_each(|x| {
            if *x != "[]" {
                let kanji: kanji::Kanji = serde_json::from_str(x)?;
                let chr = kanji.text().chars().next().unwrap();
                kanji_info.insert(chr, kanji.clone());
                state.kanji_cache.put(chr, kanji);
            }
            Ok::<(), IchiranError>(())
        })?;
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

        let jmdict_path = &self.jmdict_path().await?;
        let jmdict = JmDictData::new(jmdict_path).await;

        if let Ok(jmdict) = &jmdict {
            let mut state = self.shared.state.lock().unwrap();
            state.jmdict.replace(jmdict.clone());
        }
        jmdict
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

    pub async fn conn_params(&self) -> Result<ConnParams, IchiranError> {
        let conn_params = self
            .evaluate(r#"(format t "~{~a~^,~}" ichiran/conn::*connection*)"#)
            .await?;
        let parse_error = || IchiranError::Parsing(conn_params.clone());

        let mut conn_params = conn_params
            .lines()
            .next()
            .ok_or_else(parse_error)?
            .split(',');

        let database = conn_params.next().ok_or_else(parse_error)?.to_owned();
        let user = conn_params.next().ok_or_else(parse_error)?.to_owned();
        let password = conn_params.next().ok_or_else(parse_error)?.to_owned();
        let hostname = conn_params.next().ok_or_else(parse_error)?.to_owned();
        let _ = conn_params.next().ok_or_else(parse_error)?.to_owned();
        let port = conn_params
            .next()
            .ok_or_else(parse_error)?
            .parse::<u16>()
            .map_err(|_| parse_error())?;

        Ok(ConnParams {
            database,
            user,
            password,
            hostname,
            port,
        })
    }

    /// Evaluate the expression with ichiran and return the raw output.
    async fn evaluate(&self, expr: impl AsRef<OsStr>) -> Result<String, IchiranError> {
        let working_dir = self.working_dir()?;
        let expr = expr.as_ref();
        tracing::trace!(expr = ?expr);

        let output = Command::new(&self.shared.path)
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
        Path::new(&self.shared.path).parent().ok_or_else(|| {
            io::Error::new(
                ErrorKind::NotFound,
                "Could not find working directory of ichiran-cli",
            )
        })
    }
}
#[cfg(test)]
mod tests {
    pub(crate) mod fixture;
}
