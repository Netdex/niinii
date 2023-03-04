use std::sync::{Arc, Mutex};

use thiserror::Error;

use crate::settings::{Settings, TranslatorType};

pub use self::{
    chatgpt::{ChatGpt, ChatGptTranslation},
    deepl::{DeepL, DeepLTranslation},
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
    shared: Arc<Shared>,
}
struct Shared {
    state: Mutex<State>,
}
enum State {
    DeepL(DeepL),
    ChatGpt(ChatGpt),
}
impl Translator {
    pub fn new(settings: &Settings) -> Self {
        Self {
            shared: Arc::new(Shared {
                state: Mutex::new(match settings.translator_type() {
                    TranslatorType::DeepL => State::DeepL(DeepL::new(settings)),
                    TranslatorType::ChatGpt => State::ChatGpt(ChatGpt::new(settings)),
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
