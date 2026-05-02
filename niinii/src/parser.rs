use std::{collections::HashMap, sync::Arc};

use ichiran::prelude::*;
use thiserror::Error;

use crate::settings::Settings;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Ichiran(#[from] IchiranError),
    #[error(transparent)]
    RegexError(#[from] fancy_regex::Error),
}

#[derive(Debug)]
pub struct SyntaxTree {
    pub original_text: String,
    pub root: Root,
    /// Filled in asynchronously after the AST is displayable. May be empty
    /// while a kanji-info fetch is still in flight; callers must tolerate
    /// missing entries.
    pub kanji_info: HashMap<char, Kanji>,
    pub jmdict_data: JmDictData,
}
impl SyntaxTree {
    pub fn empty(&self) -> bool {
        self.root.is_flat()
    }
}

#[derive(Clone)]
pub struct Parser {
    shared: Arc<Shared>,
}
struct Shared {
    ichiran: Ichiran,
    _pg_daemon: Option<PostgresDaemon>,
}
impl Parser {
    pub async fn new(settings: &Settings) -> Self {
        let ichiran = Ichiran::new(settings.ichiran_path.clone(), settings.ichiran_pool_size);
        let pg_daemon = match ichiran.conn_params().await {
            Ok(conn_params) => {
                let pg_daemon = PostgresDaemon::new(
                    &settings.postgres_path,
                    &settings.db_path,
                    conn_params,
                    false,
                );
                Some(pg_daemon)
            }
            Err(_) => {
                tracing::warn!("could not get db conn params from ichiran");
                None
            }
        };
        Self {
            shared: Arc::new(Shared {
                ichiran,
                _pg_daemon: pg_daemon,
            }),
        }
    }
    /// Parse the AST for display. Skips kanji info, which is fetched
    /// separately via `parse_kanji` and merged into the tree once it
    /// arrives. This is the latency-critical path: the AST renders as
    /// soon as romanize returns.
    pub async fn parse_ast(
        &self,
        text: &str,
        splits: &[(Split, String)],
        variants: u32,
    ) -> Result<SyntaxTree, Error> {
        let ichiran = &self.shared.ichiran;

        let (root, jmdict_data) = tokio::try_join!(
            ichiran.romanize(splits, variants),
            ichiran.jmdict_data(),
        )?;

        Ok(SyntaxTree {
            root,
            kanji_info: HashMap::new(),
            jmdict_data,
            original_text: text.to_string(),
        })
    }

    /// Fetch per-character kanji info for `text`. Runs concurrently with
    /// `parse_ast` and contends for the same ichiran-cli pool.
    pub async fn parse_kanji(&self, text: &str) -> Result<HashMap<char, Kanji>, Error> {
        Ok(self.shared.ichiran.kanji_from_str(text).await?)
    }
}
