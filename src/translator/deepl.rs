use std::sync::Arc;

use async_trait::async_trait;

use crate::settings::Settings;

use super::{Error, Translate, Translation};

#[derive(Debug)]
pub struct DeepLTranslation {
    pub source_text: String,
    pub deepl_text: String,
    pub deepl_usage: deepl_api::UsageInformation,
}

#[derive(Clone)]
pub struct DeepLTranslator {
    shared: Arc<Shared>,
}
struct Shared {
    deepl: deepl_api::DeepL,
}
impl DeepLTranslator {
    pub fn new(settings: &Settings) -> Self {
        Self {
            shared: Arc::new(Shared {
                deepl: deepl_api::DeepL::new(settings.deepl_api_key.to_string()),
            }),
        }
    }
}
#[async_trait]
impl Translate for DeepLTranslator {
    async fn translate(
        &mut self,
        _settings: &Settings,
        text: impl 'async_trait + Into<String> + Send,
    ) -> Result<Translation, Error> {
        let deepl = &self.shared.deepl;
        let text = text.into();
        let deepl_text = deepl
            .translate(
                None,
                deepl_api::TranslatableTextList {
                    source_language: Some("JA".into()),
                    target_language: "EN-US".into(),
                    texts: vec![text.clone()],
                },
            )?
            .first()
            .unwrap()
            .text
            .clone();
        let deepl_usage = deepl.usage_information()?;
        Ok(Translation::DeepL(DeepLTranslation {
            source_text: text,
            deepl_text,
            deepl_usage,
        }))
    }
}
