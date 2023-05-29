use async_trait::async_trait;
use enum_dispatch::enum_dispatch;
use thiserror::Error;

use crate::settings::{Settings, TranslatorType};

use self::{
    chatgpt::{ChatGptTranslation, ChatGptTranslator},
    deepl::{DeepLTranslation, DeepLTranslator},
};

pub mod chatgpt;
pub mod deepl;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    DeepL(#[from] deepl_api::Error),
    #[error(transparent)]
    ChatGpt(#[from] openai_chat::Error),
}

#[derive(Clone)]
#[enum_dispatch]
pub enum Translator {
    DeepL(DeepLTranslator),
    ChatGpt(ChatGptTranslator),
}
impl Translator {
    pub fn new(settings: &Settings) -> Self {
        match settings.translator_type() {
            TranslatorType::DeepL => Translator::DeepL(DeepLTranslator),
            TranslatorType::ChatGpt => Translator::ChatGpt(ChatGptTranslator::new(settings)),
        }
    }
}

#[async_trait]
#[enum_dispatch(Translator)]
pub trait Translate {
    async fn translate(
        &mut self,
        settings: &Settings,
        text: impl 'async_trait + Into<String> + Send,
    ) -> Result<Translation, Error>;
}

#[derive(Debug)]
#[enum_dispatch]
pub enum Translation {
    DeepL(DeepLTranslation),
    ChatGpt(ChatGptTranslation),
}
