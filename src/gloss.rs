use std::{collections::HashMap, sync::Arc};

use ichiran::{
    kanji::Kanji, pgdaemon::PostgresDaemon, romanize::Root, Ichiran, IchiranError, JmDictData,
};
use thiserror::Error;

use crate::view::settings::Settings;

const MAX_TEXT_LENGTH: usize = 512;

#[derive(Error, Debug)]
pub enum GlossError {
    #[error(transparent)]
    Ichiran(#[from] IchiranError),
    #[error("Text too long ({length}/{MAX_TEXT_LENGTH} chars)")]
    TextTooLong { length: usize },
    #[error(transparent)]
    RegexError(#[from] fancy_regex::Error),
}

#[derive(Debug)]
pub struct Gloss {
    pub original_text: String,
    pub root: Root,
    pub kanji_info: HashMap<char, Kanji>,
    pub jmdict_data: JmDictData,
    pub translatable: bool,
}

#[derive(Clone)]
pub struct Glossator {
    shared: Arc<Shared>,
}
struct Shared {
    ichiran: Ichiran,
    _pg_daemon: Option<PostgresDaemon>,
}
impl Glossator {
    pub fn new(settings: &Settings) -> Self {
        let ichiran = Ichiran::new(settings.ichiran_path.clone());
        let pg_daemon = match ichiran.conn_params() {
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
    pub fn gloss(&self, text: &str, variants: u32) -> Result<Gloss, GlossError> {
        if text.len() > MAX_TEXT_LENGTH {
            return Err(GlossError::TextTooLong { length: text.len() });
        }
        let ichiran = &self.shared.ichiran;

        let mut root = None;
        let mut kanji_info = None;
        let mut jmdict_data = None;

        std::thread::scope(|s| {
            s.spawn(|| root = Some(ichiran.romanize(&text, variants)));
            s.spawn(|| kanji_info = Some(ichiran.kanji_from_str(&text)));
            s.spawn(|| jmdict_data = Some(ichiran.jmdict_data()));
        });

        let root = root.unwrap()?;
        let kanji_info = kanji_info.unwrap()?;
        let jmdict_data = jmdict_data.unwrap()?;
        let translatable = !root.is_flat();

        Ok(Gloss {
            root,
            kanji_info,
            jmdict_data,
            translatable,
            original_text: text.to_string(),
        })
    }
}
