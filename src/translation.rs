use deepl_api::{DeepL, TranslatableTextList};

#[derive(Debug)]
pub struct Translation {
    pub deepl_text: String,
    pub deepl_usage: deepl_api::UsageInformation,
}

pub fn translate(deepl_api_key: &str, text: &str) -> Result<Translation, deepl_api::Error> {
    let deepl = DeepL::new(deepl_api_key.to_string());
    let deepl_text = deepl
        .translate(
            None,
            TranslatableTextList {
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
    Ok(Translation {
        deepl_text,
        deepl_usage,
    })
}
