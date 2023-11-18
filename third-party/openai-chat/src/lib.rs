//! Q: Did you really write an entire throwaway crate for this?
//!    Like, you're not even publishing this crate anywhere.
//!    You literally wrote it just for this program.
//! A: Yes, and?

use std::time::Duration;

use backon::{BackoffBuilder, Retryable};
use eventsource_stream::Eventsource;
use thiserror::Error;
use tokio_stream::{Stream, StreamExt};

pub use message_buffer::*;
pub use protocol::*;

use crate::chat::PartialResponse;

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
pub struct Client<B> {
    client: reqwest::Client,
    token: String,
    backoff: B,
}

impl<B: BackoffBuilder> Client<B> {
    pub fn new(token: impl Into<String>, backoff: B) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap(),
            token: token.into(),
            backoff,
        }
    }

    pub async fn chat(&self, mut request: chat::Request) -> Result<chat::Completion, Error> {
        assert!(!request.stream.unwrap_or(false), "streaming not supported");
        request.stream = Some(false);

        tracing::trace!(?request);
        let response: chat::Response = self
            .send_request("https://api.openai.com/v1/chat/completions", &request)
            .await?
            .json()
            .await?;
        tracing::trace!(?response);
        match response {
            chat::Response::Completion(completion) => Ok(completion),
            chat::Response::Error(error) => Err(Error::Chat(error)),
        }
    }

    pub async fn stream(
        &self,
        mut request: chat::Request,
    ) -> Result<impl Stream<Item = Result<chat::PartialCompletion, Error>>, reqwest::Error> {
        assert!(request.stream.unwrap_or(true), "streaming required");
        request.stream = Some(true);

        tracing::trace!(?request);
        let stream = self
            .send_request("https://api.openai.com/v1/chat/completions", &request)
            .await?
            .bytes_stream()
            .eventsource();
        Ok(stream.map_while(|event| match event {
            Ok(event) => {
                if event.data == "[DONE]" {
                    None
                } else {
                    let response = match serde_json::from_str::<PartialResponse>(&event.data) {
                        Ok(response) => {
                            tracing::trace!(?response);
                            Ok::<_, Error>(response.0)
                        }
                        Err(err) => {
                            tracing::error!(?err, ?event.data);
                            Err(err.into())
                        }
                    };
                    Some(response)
                }
            }
            Err(err) => Some(Err(err.into())),
        }))
    }

    pub async fn moderation(
        &self,
        request: &moderation::Request,
    ) -> Result<moderation::Result, Error> {
        tracing::trace!(?request);
        let mut response: moderation::Response = self
            .send_request("https://api.openai.com/v1/moderations", request)
            .await?
            .json()
            .await?;
        tracing::trace!(?response);
        Ok(response.results.remove(0))
    }

    async fn send_request(
        &self,
        uri: impl reqwest::IntoUrl,
        request: &impl serde::Serialize,
    ) -> reqwest::Result<reqwest::Response> {
        let Self {
            token,
            client,
            backoff,
        } = self;
        let uri = uri.into_url()?;
        let request_builder = || async {
            let uri = uri.clone();
            client
                .post(uri)
                .bearer_auth(token)
                .json(&request)
                .send()
                .await
        };
        Ok(request_builder.retry(backoff).await?)
    }
}

#[cfg(test)]
mod tests {
    use tracing_test::traced_test;

    mod fixture;
    use super::*;

    #[tokio::test]
    #[traced_test]
    async fn test_chat() {
        let client = fixture::client();
        let request = chat::Request {
            messages: vec![chat::Message {
                role: chat::Role::User,
                content: Some("What is the capital city of Canada?".into()),
                ..Default::default()
            }],
            ..Default::default()
        };
        let response = client.chat(request).await.unwrap();
        let content = &response
            .choices
            .first()
            .unwrap()
            .message
            .content
            .as_ref()
            .unwrap();
        assert!(content.contains("Ottawa"));
    }

    #[tokio::test]
    #[traced_test]
    async fn test_stream() {
        let client = fixture::client();
        let request = chat::Request {
            messages: vec![chat::Message {
                role: chat::Role::User,
                content: Some("What is the capital city of Canada?".into()),
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut response = client.stream(request).await.unwrap();
        while let Some(msg) = response.next().await {
            msg.unwrap();
        }
    }

    #[tokio::test]
    #[traced_test]
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
