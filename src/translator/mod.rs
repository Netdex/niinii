use std::sync::{Arc, Mutex};

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
pub struct Translator {
    pub(crate) shared: Arc<Shared>,
}
pub(crate) struct Shared {
    pub(crate) state: Mutex<State>,
}
pub(crate) enum State {
    DeepL(DeepLTranslator),
    ChatGpt(ChatGptTranslator),
}
impl Translator {
    pub fn new(settings: &Settings) -> Self {
        Self {
            shared: Arc::new(Shared {
                state: Mutex::new(match settings.translator_type() {
                    TranslatorType::DeepL => State::DeepL(DeepLTranslator::new(settings)),
                    TranslatorType::ChatGpt => State::ChatGpt(ChatGptTranslator::new(settings)),
                }),
            }),
        }
    }
    pub fn translate(&self, text: &str) -> Result<Translation, Error> {
        let mut state = self.shared.state.lock().unwrap();
        match &mut *state {
            State::DeepL(deepl) => Ok(Translation::DeepL(deepl.translate(text)?)),
            State::ChatGpt(chatgpt) => Ok(Translation::ChatGpt(chatgpt.translate(text)?)),
        }
    }
}

#[derive(Debug)]
pub enum Translation {
    DeepL(DeepLTranslation),
    ChatGpt(ChatGptTranslation),
}
