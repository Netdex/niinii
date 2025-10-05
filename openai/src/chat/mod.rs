//! https://platform.openai.com/docs/api-reference/chat

mod chat_buffer;

use eventsource_stream::Eventsource;
use reqwest::Method;
use tokio_stream::{Stream, StreamExt};
use tracing::Level;

pub use crate::protocol::chat::{Message, PartialMessage, Request, Role, Usage};
pub use chat_buffer::{ChatBuffer, Exchange};

use crate::{
    protocol::{
        chat::{self, ChatResponse, StreamResponse},
        StreamOptions,
    },
    Client, Error,
};

impl Client {
    #[tracing::instrument(level = Level::DEBUG, skip_all, err)]
    pub async fn chat(&self, mut request: Request) -> Result<chat::Completion, Error> {
        request.stream = None;
        request.stream_options = None;
        tracing::debug!(?request);
        let response: chat::ChatResponse = self
            .shared
            .request(Method::POST, "v1/chat/completions")
            .body(&request)
            .send()
            .await?
            .json()
            .await?;
        tracing::debug!(?response);
        Ok(response.0?)
    }

    #[tracing::instrument(level = Level::DEBUG, skip_all, err)]
    pub async fn stream(
        &self,
        mut request: Request,
    ) -> Result<impl Stream<Item = Result<chat::PartialCompletion, Error>>, Error> {
        request.stream = Some(true);
        request.stream_options = Some(StreamOptions {
            include_obfuscation: false,
            include_usage: true,
        });
        tracing::debug!(?request);
        let response = self
            .shared
            .request(Method::POST, "v1/chat/completions")
            .body(&request)
            .send()
            .await?;
        let status = response.status();
        if status.is_success() {
            // HTTP success: Expect SSE response
            let stream = response.bytes_stream().eventsource();
            Ok(stream.map_while(|event| {
                tracing::trace!(?event);
                match event {
                    Ok(event) => {
                        if event.data == "[DONE]" {
                            None
                        } else {
                            let response = match serde_json::from_str::<StreamResponse>(&event.data)
                            {
                                Ok(response) => {
                                    tracing::debug!(?response);
                                    Ok::<_, Error>(response.0)
                                }
                                Err(err) => {
                                    // Serde error
                                    tracing::error!(?err, ?event.data);
                                    Err(err.into())
                                }
                            };
                            Some(response)
                        }
                    }
                    Err(err) => {
                        // SSE error
                        tracing::error!(?err);
                        Some(Err(err.into()))
                    }
                }
            }))
        } else {
            // HTTP error: Expect JSON response
            let response_err = response.error_for_status_ref().unwrap_err();
            let chat_response = response.json::<ChatResponse>().await;
            match chat_response {
                Ok(err) => {
                    // OpenAI application error
                    Err(Error::Protocol(err.0.unwrap_err()))
                }
                Err(err) => {
                    // Not application error, return HTTP error
                    tracing::error!(?response_err, ?err, "unexpected stream response");
                    Err(response_err.into())
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio_stream::StreamExt;
    use tracing_test::traced_test;

    use super::*;
    use crate::tests::fixture;

    #[tokio::test]
    #[traced_test]
    #[ignore]
    async fn test_chat() {
        let client = fixture::client();
        let request = Request {
            messages: vec![chat::Message {
                role: chat::Role::User,
                content: Some("What is the capital city of Canada?".into()),
                ..Default::default()
            }],
            ..Default::default()
        };
        let response = client.chat(request).await.unwrap();
        println!("{:#?}", response);
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
    #[ignore]
    async fn test_stream() {
        let client = fixture::client();
        let request = Request {
            messages: vec![chat::Message {
                role: chat::Role::User,
                content: Some("What is the capital city of Canada?".into()),
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut stream = client.stream(request).await.unwrap();
        while let Some(msg) = stream.next().await {
            println!("{:?}", msg);
        }
    }
}
