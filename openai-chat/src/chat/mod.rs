//! https://platform.openai.com/docs/api-reference/chat

mod chat_buffer;

use eventsource_stream::Eventsource;
use reqwest::Method;
use tokio_stream::{Stream, StreamExt};
use tracing::Level;

pub use crate::protocol::chat::{Message, Model, PartialMessage, Role, Usage};
pub use chat_buffer::{ChatBuffer, Exchange};

use crate::{
    protocol::chat::{self, ChatResponse, StreamOptions, StreamResponse},
    Client, Error,
};

#[derive(Debug, Clone, Default)]
pub struct Request {
    /// ID of the model to use. Currently, only gpt-3.5-turbo and gpt-3.5-turbo-0301 are supported.
    pub model: Model,
    /// The messages to generate chat completions for, in the chat format.
    pub messages: Vec<Message>,
    /// What sampling temperature to use, between 0 and 2. Higher values like
    /// 0.8 will make the output more random, while lower values like 0.2 will
    /// make it more focused and deterministic.
    /// We generally recommend altering this or top_p but not both.
    pub temperature: Option<f32>,
    /// An alternative to sampling with temperature, called nucleus sampling,
    /// where the model considers the results of the tokens with top_p
    /// probability mass. So 0.1 means only the tokens comprising the top 10%
    /// probability mass are considered.
    /// We generally recommend altering this or temperature but not both.
    pub top_p: Option<f32>,
    /// How many chat completion choices to generate for each input message.
    pub n: Option<u32>,
    /// The maximum number of tokens allowed for the generated answer. By
    /// default, the number of tokens the model can return will be (4096 - prompt
    /// tokens).
    pub max_completion_tokens: Option<u32>,
    /// Number between -2.0 and 2.0. Positive values penalize new tokens based
    /// on whether they appear in the text so far, increasing the model's
    /// likelihood to talk about new topics.
    pub presence_penalty: Option<f32>,
}
impl From<Request> for chat::Request {
    fn from(value: Request) -> Self {
        let Request {
            model,
            messages,
            temperature,
            top_p,
            n,
            max_completion_tokens,
            presence_penalty,
        } = value;
        chat::Request {
            model,
            messages,
            temperature,
            top_p,
            n,
            max_completion_tokens,
            presence_penalty,
            ..Default::default()
        }
    }
}

impl Client {
    #[tracing::instrument(level = Level::DEBUG, skip_all, err)]
    pub async fn chat(&self, request: Request) -> Result<chat::Completion, Error> {
        let request: chat::Request = request.into();
        tracing::debug!(?request);
        let response: chat::ChatResponse = self
            .shared
            .request(Method::POST, "/v1/chat/completions")
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
        request: Request,
    ) -> Result<impl Stream<Item = Result<chat::PartialCompletion, Error>>, Error> {
        let mut request: chat::Request = request.into();
        request.stream = Some(true);
        request.stream_options = Some(StreamOptions {
            include_obfuscation: false,
            include_usage: false,
        });

        tracing::debug!(?request);
        let response = self
            .shared
            .request(Method::POST, "/v1/chat/completions")
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
