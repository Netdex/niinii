use crate::settings::Settings;

#[derive(Debug)]
pub struct DeepLTranslation {
    pub source_text: String,
    pub deepl_text: String,
    pub deepl_usage: deepl_api::UsageInformation,
}

pub struct DeepLTranslator {
    deepl: deepl_api::DeepL,
}
impl DeepLTranslator {
    pub fn new(settings: &Settings) -> Self {
        Self {
            deepl: deepl_api::DeepL::new(settings.deepl_api_key.to_string()),
        }
    }
    pub fn translate(&self, text: &str) -> Result<DeepLTranslation, deepl_api::Error> {
        let Self { deepl } = self;
        let deepl_text = deepl
            .translate(
                None,
                deepl_api::TranslatableTextList {
                    source_language: Some("JA".into()),
                    target_language: "EN-US".into(),
                    texts: vec![text.to_string()],
                },
            )?
            .first()
            .unwrap()
            .text
            .clone();
        let deepl_usage = deepl.usage_information()?;
        Ok(DeepLTranslation {
            source_text: text.to_string(),
            deepl_text,
            deepl_usage,
        })
    }
}
