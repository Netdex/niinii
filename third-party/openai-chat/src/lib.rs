//! Q: Did you really write an entire throwaway crate for this?
//!    Like, you're not even publishing this crate anywhere.
//!    You literally wrote it just for this program.
//! A: Yes, and?

use std::{sync::Arc, time::Duration};

use thiserror::Error;

pub use protocol::*;
use tokio::sync::Mutex;

mod protocol;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Network(#[from] reqwest::Error),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error("Chat Error: {0}")]
    Chat(#[from] chat::Error),
}

#[derive(Clone)]
pub struct Client {
    shared: Arc<Shared>,
}
struct Shared {
    token: String,
    state: Mutex<State>,
}
struct State {
    client: reqwest::Client,
}

impl Client {
    pub fn new(token: impl Into<String>) -> Self {
        let token = token.into();
        Self {
            shared: Arc::new(Shared {
                token,
                state: Mutex::new(State {
                    client: reqwest::Client::builder()
                        .timeout(Duration::from_secs(5))
                        .build()
                        .unwrap(),
                }),
            }),
        }
    }
    pub async fn chat(&self, request: &chat::Request) -> Result<chat::Completion, Error> {
        assert!(!request.stream.unwrap_or(false), "streaming not supported");
        let Shared { token, state } = &*self.shared;
        let State { client } = &mut *state.lock().await;
        let response: chat::Response = client
            .post("https://api.openai.com/v1/chat/completions")
            .bearer_auth(token)
            .json(request)
            .send()
            .await?
            .json()
            .await?;
        match response {
            chat::Response::Completion(completion) => Ok(completion),
            chat::Response::Error { error } => Err(Error::Chat(error)),
        }
    }
    pub async fn moderation(
        &self,
        request: &moderation::Request,
    ) -> Result<moderation::Result, Error> {
        let Shared { token, state } = &*self.shared;
        let State { client } = &mut *state.lock().await;
        let mut response: moderation::Response = client
            .post("https://api.openai.com/v1/moderations")
            .bearer_auth(token)
            .json(request)
            .send()
            .await?
            .json()
            .await?;
        Ok(response.results.remove(0))
    }
}

#[cfg(test)]
mod tests {
    mod fixture;
    use super::*;

    #[tokio::test]
    async fn test_chat() {
        let client = fixture::client();
        let request = chat::Request {
            messages: vec![chat::Message {
                role: chat::Role::User,
                content: "What is the capital city of Canada?".into(),
            }],
            ..Default::default()
        };
        let response = client.chat(&request).await.unwrap();
        let content = &response.choices.first().unwrap().message.content;
        assert!(content.contains("Ottawa"));
    }

    #[tokio::test]
    async fn test_moderation() {
        let client = fixture::client();
        let request = moderation::Request {
            input: "I'm going to fucking kill you".into(),
            ..Default::default()
        };
        let response = client.moderation(&request).await.unwrap();
        assert!(response.flagged);
        assert!(response.categories[&moderation::Category::Violence]);
    }
}
