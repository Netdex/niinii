//! https://platform.openai.com/docs/guides/conversation-state?api-mode=responses

use serde::Deserialize;
use tracing::Level;

pub use crate::protocol::responses::{
    ContextManagementEntry, ContextManagementType, ConversationRef, InputTokensDetails, Message,
    MessageContent, OutputItem, OutputMessage, OutputTextContent, ReasoningOptions, Request,
    Response, StreamEvent, Usage,
};
use crate::{Client, Error};
use eventsource_stream::Eventsource;
use reqwest::Method;
use tokio_stream::{Stream, StreamExt};

impl Client {
    #[tracing::instrument(level = Level::DEBUG, skip_all, err)]
    pub async fn responses(&self, mut request: Request) -> Result<Response, Error> {
        request.stream = None;
        request.stream_options = None;
        tracing::debug!(?request);
        let response = self
            .shared
            .request(Method::POST, "v1/responses")
            .body(&request)
            .send()
            .await?;
        let status = response.status();
        if status.is_success() {
            let response: Response = response.json().await?;
            tracing::debug!(?response);
            Ok(response)
        } else {
            let response_err = response.error_for_status_ref().unwrap_err();
            let protocol_err = response.json::<ErrorResponse>().await;
            match protocol_err {
                Ok(err) => Err(Error::Protocol(err.error)),
                Err(err) => {
                    tracing::error!(?response_err, ?err, "unexpected responses error");
                    Err(response_err.into())
                }
            }
        }
    }

    #[tracing::instrument(level = Level::DEBUG, skip_all, err)]
    pub async fn stream_responses(
        &self,
        mut request: Request,
    ) -> Result<impl Stream<Item = Result<StreamEvent, Error>>, Error> {
        request.stream = Some(true);
        request.stream_options = None;
        tracing::debug!(?request);

        let response = self
            .shared
            .request(Method::POST, "v1/responses")
            .body(&request)
            .send()
            .await?;

        let status = response.status();
        if status.is_success() {
            let stream = response.bytes_stream().eventsource();
            Ok(stream.map_while(|event| match event {
                Ok(event) => {
                    tracing::trace!(?event);
                    if event.data == "[DONE]" {
                        None
                    } else {
                        let parsed =
                            serde_json::from_str::<StreamEvent>(&event.data).map_err(Error::from);
                        Some(parsed)
                    }
                }
                Err(err) => {
                    tracing::trace!(?err, "responses SSE event error");
                    Some(Err(err.into()))
                }
            }))
        } else {
            let response_err = response.error_for_status_ref().unwrap_err();
            let protocol_err = response.json::<ErrorResponse>().await;
            match protocol_err {
                Ok(err) => Err(Error::Protocol(err.error)),
                Err(err) => {
                    tracing::error!(?response_err, ?err, "unexpected responses stream error");
                    Err(response_err.into())
                }
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    error: crate::protocol::Error,
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
    async fn test_responses_basic() {
        let client = fixture::client();
        let request = Request {
            model: crate::ModelId("gpt-4.1-mini".into()),
            input: vec![crate::protocol::responses::Message {
                role: crate::protocol::Role::User,
                content: Some("Say hello in one short sentence.".into()),
            }],
            ..Default::default()
        };

        let response = client.responses(request).await.unwrap();
        println!("{:#?}", response);
        assert!(!response.id.is_empty());
    }

    #[tokio::test]
    #[traced_test]
    #[ignore]
    async fn test_responses_stream() {
        let client = fixture::client();
        let request = Request {
            model: crate::ModelId("gpt-4.1-mini".into()),
            input: vec![crate::protocol::responses::Message {
                role: crate::protocol::Role::User,
                content: Some("Count to three.".into()),
            }],
            ..Default::default()
        };

        let mut stream = client.stream_responses(request).await.unwrap();
        while let Some(event) = stream.next().await {
            println!("{:?}", event);
        }
    }
}
