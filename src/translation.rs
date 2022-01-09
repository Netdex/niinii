use deepl_api::{DeepL, TranslatableTextList};

#[derive(Debug)]
pub enum Translation {
    DeepL {
        deepl_text: String,
        deepl_usage: deepl_api::UsageInformation,
    },
}

fn filter_text(text: &str) -> &str {
    let pattern: &[_] = &['"', '[', ']', '«', '»', ' '];
    text.trim_matches(pattern)
}

pub fn translate(deepl_api_key: &str, text: &str) -> Result<Translation, deepl_api::Error> {
    let text = filter_text(text);
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
    Ok(Translation::DeepL {
        deepl_text,
        deepl_usage,
    })
}
