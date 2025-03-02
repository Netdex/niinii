use async_trait::async_trait;
use enclose::enclose;

use super::{Error, Translation, Translator};
use crate::{
    settings::Settings,
    view::{
        translator::{ViewDeepLTranslation, ViewDeepLTranslationUsage, ViewDeepLTranslator},
        View,
    },
};

pub struct DeepLTranslator;

#[async_trait]
impl Translator for DeepLTranslator {
    async fn translate(
        &self,
        settings: &Settings,
        text: String,
    ) -> Result<Box<dyn Translation>, Error> {
        let Settings { deepl_api_key, .. } = settings;

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

        Ok(Box::new(DeepLTranslation {
            source_text: text,
            deepl_text,
            deepl_usage,
        }))
    }
    fn view<'a>(&'a self, _settings: &'a mut Settings) -> Box<dyn View + 'a> {
        Box::new(ViewDeepLTranslator)
    }
}

#[derive(Debug)]
pub struct DeepLTranslation {
    pub source_text: String,
    pub deepl_text: String,
    pub deepl_usage: deepl_api::UsageInformation,
}
impl Translation for DeepLTranslation {
    fn view(&self) -> Box<dyn View + '_> {
        Box::new(ViewDeepLTranslation(self))
    }
    fn view_usage(&self) -> Box<dyn View + '_> {
        Box::new(ViewDeepLTranslationUsage(self))
    }
}
