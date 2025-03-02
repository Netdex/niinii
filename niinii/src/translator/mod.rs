use async_trait::async_trait;
use thiserror::Error;

use crate::{settings::Settings, view::View};

pub mod chat;
pub mod deepl;
pub mod realtime;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    DeepL(#[from] deepl_api::Error),
    #[error(transparent)]
    OpenAI(#[from] openai_chat::Error),
}

#[async_trait]
pub trait Translator: Send + Sync {
    async fn translate(
        &self,
        settings: &Settings,
        text: String,
    ) -> Result<Box<dyn Translation>, Error>;
    fn view<'a>(&'a self, settings: &'a mut Settings) -> Box<dyn View + 'a>;
}

pub trait Translation: Send {
    fn view(&self) -> Box<dyn View + '_>;
    fn view_usage(&self) -> Box<dyn View + '_>;
}
