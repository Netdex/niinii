use enum_dispatch::enum_dispatch;
use thiserror::Error;

use crate::settings::{Settings, TranslatorType};

pub use self::{
    chatgpt::{ChatGptTranslation, ChatGptTranslator},
    deepl::{DeepLTranslation, DeepLTranslator},
};

mod chatgpt;
mod deepl;

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
            TranslatorType::DeepL => Translator::DeepL(DeepLTranslator::new(&settings)),
            TranslatorType::ChatGpt => Translator::ChatGpt(ChatGptTranslator::new(&settings)),
        }
    }
}

#[enum_dispatch(Translator)]
pub trait Translate {
    fn translate(
        &mut self,
        settings: &Settings,
        text: impl Into<String>,
    ) -> Result<Translation, Error>;
}

#[derive(Debug)]
pub enum Translation {
    DeepL(DeepLTranslation),
    ChatGpt(ChatGptTranslation),
}
