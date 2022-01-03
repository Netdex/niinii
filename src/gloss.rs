use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use deepl_api::{DeepL, TranslatableTextList, UsageInformation};
use ichiran::{
    kanji::Kanji, pgdaemon::PostgresDaemon, romanize::Root, Ichiran, IchiranError, JmDictData,
};
use thiserror::Error;

use crate::view::settings::SettingsView;

#[derive(Debug)]
pub struct Gloss {
    pub elapsed: Duration,

    pub root: Root,
    pub kanji_info: HashMap<char, Kanji>,
    pub jmdict_data: JmDictData,

    pub deepl_text: Option<String>,
    pub deepl_usage: Option<UsageInformation>,
}

#[derive(Error, Debug)]
pub enum GlossError {
    #[error(transparent)]
    Ichiran(#[from] IchiranError),
    #[error(transparent)]
    DeepL(#[from] deepl_api::Error),
}

#[derive(Clone)]
pub struct Glossator {
    shared: Arc<Shared>,
}
struct Shared {
    ichiran: Ichiran,
    _pg_daemon: Option<PostgresDaemon>,
    deepl: DeepL,
}
impl Glossator {
    pub fn new(settings: &SettingsView) -> Self {
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
                deepl: DeepL::new(settings.deepl_api_key.clone()),
            }),
        }
    }
    pub fn gloss(&self, text: &str, use_deepl: bool) -> Result<Gloss, GlossError> {
        let ichiran = &self.shared.ichiran;

        let mut root = None;
        let mut kanji_info = None;
        let mut jmdict_data = None;
        let mut deepl_text = None;
        let mut deepl_usage = None;

        let start = Instant::now();
        rayon::scope(|s| {
            s.spawn(|_| root = Some(ichiran.romanize(&text, 5)));
            s.spawn(|_| kanji_info = Some(ichiran.kanji_from_str(&text)));
            s.spawn(|_| jmdict_data = Some(ichiran.jmdict_data()));

            if use_deepl {
                s.spawn(|_| {
                    deepl_text = Some(self.shared.deepl.translate(
                        None,
                        TranslatableTextList {
                            source_language: Some("JA".into()),
                            target_language: "EN-US".into(),
                            texts: vec![text.to_string()],
                        },
                    ))
                });
                s.spawn(|_| deepl_usage = Some(self.shared.deepl.usage_information()));
            }
        });
        let elapsed = start.elapsed();

        Ok(Gloss {
            elapsed,
            root: root.unwrap()?,
            kanji_info: kanji_info.unwrap()?,
            jmdict_data: jmdict_data.unwrap()?,
            deepl_text: deepl_text
                .transpose()?
                .map(|x| x.first().unwrap().text.clone()),
            deepl_usage: deepl_usage.transpose()?,
        })
    }
}
