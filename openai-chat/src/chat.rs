use backon::BackoffBuilder;
use eventsource_stream::Eventsource;
use reqwest::Method;
use tokio_stream::{Stream, StreamExt};

pub use crate::protocol::chat::{Message, Model, PartialMessage, Role, Usage};

use crate::{
    protocol::chat::{self, StreamResponse},
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
    pub max_tokens: Option<u32>,
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
            max_tokens,
            presence_penalty,
        } = value;
        chat::Request {
            model,
            messages,
            temperature,
            top_p,
            n,
            max_tokens,
            presence_penalty,
            ..Default::default()
        }
    }
}

impl<B: BackoffBuilder + Clone> Client<B> {
    pub async fn chat(&self, request: Request) -> Result<chat::Completion, Error> {
        let request: chat::Request = request.into();

        tracing::trace!(?request);
        let response: chat::ChatResponse = self
            .shared
            .request_with_body(Method::POST, "/v1/chat/completions", &request)
            .await?
            .json()
            .await?;
        tracing::trace!(?response);
        Ok(Result::from(response)?)
    }

    pub async fn stream(
        &self,
        request: Request,
    ) -> Result<impl Stream<Item = Result<chat::PartialCompletion, Error>>, Error> {
        let mut request: chat::Request = request.into();
        request.stream = Some(true);

        tracing::trace!(?request);
        let stream = self
            .shared
            .request_with_body(Method::POST, "/v1/chat/completions", &request)
            .await?
            .bytes_stream()
            .eventsource();
        Ok(stream.map_while(|event| match event {
            Ok(event) => {
                if event.data == "[DONE]" {
                    None
                } else {
                    let response = match serde_json::from_str::<StreamResponse>(&event.data) {
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
            Err(err) => {
                tracing::error!(?err);
                Some(Err(err.into()))
            }
        }))
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
