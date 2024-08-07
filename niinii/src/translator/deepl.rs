use async_trait::async_trait;
use enclose::enclose;

use super::{Error, Translate, Translation};
use crate::settings::Settings;

#[derive(Debug)]
pub struct DeepLTranslation {
    pub source_text: String,
    pub deepl_text: String,
    pub deepl_usage: deepl_api::UsageInformation,
}

#[derive(Clone)]
pub struct DeepLTranslator;

#[async_trait]
impl Translate for DeepLTranslator {
    async fn translate(
        &mut self,
        settings: &Settings,
        text: impl 'async_trait + Into<String> + Send,
    ) -> Result<Translation, Error> {
        let Settings { deepl_api_key, .. } = settings;
        let text = text.into();

        // TODO: it would be great if there was an async version of this
        let (deepl_text, deepl_usage) = tokio::task::spawn_blocking(enclose! { (text, deepl_api_key) move || {
            let deepl = deepl_api::DeepL::new(deepl_api_key);
            let deepl_text = deepl
                .translate(
                    None,
                    deepl_api::TranslatableTextList {
                        source_language: Some("JA".into()),
                        target_language: "EN-US".into(),
                        texts: vec![text],
                    },
                )?
                .first()
                .unwrap()
                .text
                .trim()
                .to_owned();
            let deepl_usage = deepl.usage_information()?;
            Ok::<(String, deepl_api::UsageInformation), deepl_api::Error>((deepl_text, deepl_usage))
        }})
        .await
        .unwrap()?;

        Ok(Translation::DeepL(DeepLTranslation {
            source_text: text,
            deepl_text,
            deepl_usage,
        }))
    }
}
