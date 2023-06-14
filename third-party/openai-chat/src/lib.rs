//! Q: Did you really write an entire throwaway crate for this?
//!    Like, you're not even publishing this crate anywhere.
//!    You literally wrote it just for this program.
//! A: Yes, and?

use std::{sync::Arc, time::Duration};

use eventsource_stream::Eventsource;
use thiserror::Error;
use tokio_stream::{Stream, StreamExt};

pub use message_buffer::*;
pub use protocol::*;

mod message_buffer;
mod protocol;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Network(#[from] reqwest::Error),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error("Chat Error: {0}")]
    Chat(#[from] chat::Error),
    #[error(transparent)]
    EventStream(#[from] eventsource_stream::EventStreamError<reqwest::Error>),
}

#[derive(Clone)]
pub struct Client {
    client: reqwest::Client,
    shared: Arc<Shared>,
}
struct Shared {
    token: String,
}

impl Client {
    pub fn new(token: impl Into<String>) -> Self {
        let token = token.into();
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap(),
            shared: Arc::new(Shared { token }),
        }
    }

    pub async fn chat(&self, mut request: chat::Request) -> Result<chat::Completion, Error> {
        assert!(!request.stream.unwrap_or(false), "streaming not supported");
        request.stream = Some(false);

        let Self { shared, client } = self;
        let Shared { token } = &**shared;
        tracing::trace!(?request);
        let response: chat::Response = client
            .post("https://api.openai.com/v1/chat/completions")
            .bearer_auth(token)
            .json(&request)
            .send()
            .await?
            .json()
            .await?;
        tracing::trace!(?response);
        match response {
            chat::Response::Completion(completion) => Ok(completion),
            chat::Response::Error { error } => Err(Error::Chat(error)),
        }
    }

    pub async fn stream(
        &self,
        mut request: chat::Request,
    ) -> Result<impl Stream<Item = Result<chat::PartialCompletion, Error>>, reqwest::Error> {
        assert!(request.stream.unwrap_or(true), "streaming required");
        request.stream = Some(true);

        let Self { shared, client } = self;
        let Shared { token } = &**shared;
        tracing::trace!(?request);
        let stream = client
            .post("https://api.openai.com/v1/chat/completions")
            .bearer_auth(token)
            .json(&request)
            .send()
            .await?
            .bytes_stream()
            .eventsource();
        Ok(stream.map(|event| match event {
            Ok(event) => {
                let response: chat::PartialResponse = serde_json::from_str(&event.data)?;
                tracing::trace!(?response);
                match response {
                    chat::PartialResponse::Delta(delta) => Ok::<_, Error>(delta),
                    chat::PartialResponse::Error { error } => Err(Error::Chat(error)),
                }
            }
            Err(err) => Err(err.into()),
        }))
    }

    pub async fn moderation(
        &self,
        request: &moderation::Request,
    ) -> Result<moderation::Result, Error> {
        let Self { shared, client } = self;
        let Shared { token } = &**shared;
        tracing::trace!(?request);
        let mut response: moderation::Response = client
            .post("https://api.openai.com/v1/moderations")
            .bearer_auth(token)
            .json(request)
            .send()
            .await?
            .json()
            .await?;
        tracing::trace!(?response);
        Ok(response.results.remove(0))
    }
}

#[cfg(test)]
mod tests {
    use tracing_test::traced_test;

    mod fixture;
    use super::*;

    #[tokio::test]
    async fn test_chat() {
        let client = fixture::client();
        let request = chat::Request {
            messages: vec![chat::Message {
                role: chat::Role::User,
                content: "What is the capital city of Canada?".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let response = client.chat(request).await.unwrap();
        let content = &response.choices.first().unwrap().message.content;
        assert!(content.contains("Ottawa"));
    }

    #[tokio::test]
    #[traced_test]
    async fn test_stream() {
        let client = fixture::client();
        let request = chat::Request {
            messages: vec![chat::Message {
                role: chat::Role::User,
                content: "What is the capital city of Canada?".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut response = client.stream(request).await.unwrap();
        while let Some(_msg) = response.next().await {}
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
