use std::{collections::HashMap, sync::Arc};

use ichiran::{
    kanji::Kanji, pgdaemon::PostgresDaemon, romanize::Root, Ichiran, IchiranError, JmDictData,
};
use thiserror::Error;

use crate::settings::Settings;

const MAX_TEXT_LENGTH: usize = 512;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Ichiran(#[from] IchiranError),
    #[error("Text too long ({length}/{MAX_TEXT_LENGTH} chars)")]
    TextTooLong { length: usize },
    #[error(transparent)]
    RegexError(#[from] fancy_regex::Error),
}

#[derive(Debug)]
pub struct SyntaxTree {
    pub original_text: String,
    pub root: Root,
    pub kanji_info: HashMap<char, Kanji>,
    pub jmdict_data: JmDictData,
    pub translatable: bool,
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
        let ichiran = Ichiran::new(settings.ichiran_path.clone());
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
                log::warn!("could not get db conn params from ichiran");
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
    pub async fn parse(&self, text: &str, variants: u32) -> Result<SyntaxTree, Error> {
        if text.len() > MAX_TEXT_LENGTH {
            return Err(Error::TextTooLong { length: text.len() });
        }
        let ichiran = &self.shared.ichiran;

        let (root, kanji_info, jmdict_data) = tokio::try_join!(
            ichiran.romanize(text, variants),
            ichiran.kanji_from_str(text),
            ichiran.jmdict_data()
        )?;

        let translatable = !root.is_flat();

        Ok(SyntaxTree {
            root,
            kanji_info,
            jmdict_data,
            translatable,
            original_text: text.to_string(),
        })
    }
}
