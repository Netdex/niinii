//! https://platform.openai.com/docs/api-reference/responses

use eventsource_stream::Eventsource;
use reqwest::Method;
use tokio_stream::{Stream, StreamExt};
use tracing::Level;

pub use crate::protocol::responses::{
    Input, InputItem, OutputContent, OutputItem, Reasoning, ReasoningSummary, Request, Response,
    ResponseError, ResponseStatus, StreamEvent, SummaryText, TextConfig, Usage,
};

use crate::{
    protocol::{self, responses::ResponsesResponse},
    Client, Error,
};

/// Build a [`protocol::Error`] from a failed response's error object so callers
/// see the same `Error::Protocol` variant as Chat Completions failures.
fn protocol_error_from(err: Option<ResponseError>) -> protocol::Error {
    let (message, code) = match err {
        Some(e) => (e.message, e.code),
        None => ("response failed".to_string(), None),
    };
    protocol::Error {
        message,
        error_type: "response_failed".to_string(),
        param: None,
        code,
        event_id: None,
    }
}

impl Client {
    #[tracing::instrument(level = Level::DEBUG, skip_all, err)]
    pub async fn responses(&self, mut request: Request) -> Result<Response, Error> {
        request.stream = None;
        tracing::debug!(?request);
        let response: ResponsesResponse = self
            .shared
            .request(Method::POST, "v1/responses")
            .body(&request)
            .send()
            .await?
            .json()
            .await?;
        tracing::debug!(?response);
        let response = response.0?;
        // A 200 OK can still carry a terminal failure in the body.
        if response.status == ResponseStatus::Failed {
            return Err(Error::Protocol(protocol_error_from(response.error)));
        }
        Ok(response)
    }

    #[tracing::instrument(level = Level::DEBUG, skip_all, err)]
    pub async fn stream_responses(
        &self,
        mut request: Request,
    ) -> Result<impl Stream<Item = Result<StreamEvent, Error>>, Error> {
        request.stream = Some(true);
        tracing::debug!(?request);
        let response = self
            .shared
            .request(Method::POST, "v1/responses")
            .body(&request)
            .send()
            .await?;
        let status = response.status();
        if status.is_success() {
            // HTTP success: expect an SSE stream of typed events.
            let stream = response.bytes_stream().eventsource();
            Ok(stream.map_while(|event| {
                tracing::trace!(?event);
                match event {
                    Ok(event) => {
                        if event.data == "[DONE]" {
                            None
                        } else {
                            let parsed = match serde_json::from_str::<StreamEvent>(&event.data) {
                                Ok(parsed) => {
                                    tracing::debug!(?parsed);
                                    Ok::<_, Error>(parsed)
                                }
                                Err(err) => {
                                    tracing::error!(?err, ?event.data);
                                    Err(err.into())
                                }
                            };
                            Some(parsed)
                        }
                    }
                    Err(err) => {
                        tracing::error!(?err);
                        Some(Err(err.into()))
                    }
                }
            }))
        } else {
            // HTTP error: expect a JSON error envelope.
            let response_err = response.error_for_status_ref().unwrap_err();
            let body = response.json::<ResponsesResponse>().await;
            match body {
                Ok(envelope) => match envelope.0 {
                    Ok(resp) => Err(Error::Protocol(protocol_error_from(resp.error))),
                    Err(err) => Err(Error::Protocol(err)),
                },
                Err(err) => {
                    tracing::error!(?response_err, ?err, "unexpected stream response");
                    Err(response_err.into())
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{protocol::ReasoningEffort, ModelId, Role};

    fn sample_request() -> Request {
        Request::builder()
            .model(ModelId("gpt-5".into()))
            .input(Input::Items(vec![InputItem {
                role: Role::User,
                content: "Translate this line:\ntest".into(),
            }]))
            .instructions("You are a translator.".to_string())
            .store(true)
            .previous_response_id("resp_prev".to_string())
            .prompt_cache_key("session-1".to_string())
            .reasoning(Reasoning {
                effort: Some(ReasoningEffort::Minimal),
                summary: Some(ReasoningSummary::Auto),
            })
            .build()
    }

    #[test]
    fn request_serializes_chaining_and_latency_fields() {
        let json = serde_json::to_value(sample_request()).unwrap();
        assert_eq!(json["model"], "gpt-5");
        assert_eq!(json["store"], true);
        assert_eq!(json["previous_response_id"], "resp_prev");
        assert_eq!(json["prompt_cache_key"], "session-1");
        assert_eq!(json["instructions"], "You are a translator.");
        assert_eq!(json["reasoning"]["effort"], "minimal");
        assert_eq!(json["reasoning"]["summary"], "auto");
        assert_eq!(json["input"][0]["role"], "user");
        // Omitted optionals must not serialize.
        assert!(json.get("temperature").is_none());
        assert!(json.get("stream").is_none());
    }

    #[test]
    fn request_omits_reasoning_summary_when_off() {
        let req = Request::builder()
            .model(ModelId("gpt-5".into()))
            .input(Input::Text("hi".into()))
            .reasoning(Reasoning {
                effort: Some(ReasoningEffort::Low),
                summary: None,
            })
            .build();
        let json = serde_json::to_value(req).unwrap();
        assert_eq!(json["reasoning"]["effort"], "low");
        assert!(json["reasoning"].get("summary").is_none());
        assert_eq!(json["input"], "hi");
    }

    #[test]
    fn deserialize_completed_response_with_message_reasoning_usage() {
        let raw = r#"{
            "id": "resp_abc",
            "object": "response",
            "created_at": 1700000000,
            "model": "gpt-5",
            "status": "completed",
            "output": [
                {
                    "type": "reasoning",
                    "id": "rs_1",
                    "summary": [{ "type": "summary_text", "text": "Thinking about it." }]
                },
                {
                    "type": "message",
                    "id": "msg_1",
                    "role": "assistant",
                    "content": [{ "type": "output_text", "text": "Hello world.", "annotations": [] }]
                }
            ],
            "usage": {
                "input_tokens": 12,
                "input_tokens_details": { "cached_tokens": 8 },
                "output_tokens": 5,
                "output_tokens_details": { "reasoning_tokens": 3 },
                "total_tokens": 17
            }
        }"#;
        let resp: Response = serde_json::from_str(raw).unwrap();
        assert_eq!(resp.status, ResponseStatus::Completed);
        assert_eq!(resp.output_text(), "Hello world.");
        assert_eq!(resp.reasoning_text(), "Thinking about it.");
        let usage = resp.usage.unwrap();
        assert_eq!(usage.input_tokens, 12);
        assert_eq!(usage.input_tokens_details.unwrap().cached_tokens, 8);
        assert_eq!(usage.output_tokens_details.unwrap().reasoning_tokens, 3);
    }

    #[test]
    fn deserialize_unknown_output_item_is_tolerated() {
        let raw = r#"{
            "id": "resp_x",
            "status": "completed",
            "output": [
                { "type": "web_search_call", "id": "ws_1", "status": "completed" },
                { "type": "message", "role": "assistant",
                  "content": [{ "type": "output_text", "text": "ok" }] }
            ]
        }"#;
        let resp: Response = serde_json::from_str(raw).unwrap();
        assert_eq!(resp.output.len(), 2);
        assert_eq!(resp.output_text(), "ok");
    }

    #[test]
    fn deserialize_stream_events() {
        let created = r#"{"type":"response.created","sequence_number":0,"response":{"id":"resp_1","status":"in_progress"}}"#;
        let delta = r#"{"type":"response.output_text.delta","sequence_number":1,"item_id":"msg_1","output_index":0,"content_index":0,"delta":"Hel"}"#;
        let rdelta = r#"{"type":"response.reasoning_summary_text.delta","sequence_number":2,"item_id":"rs_1","output_index":0,"summary_index":0,"delta":"because"}"#;
        let completed = r#"{"type":"response.completed","sequence_number":3,"response":{"id":"resp_1","status":"completed","usage":{"input_tokens":1,"output_tokens":2,"total_tokens":3}}}"#;
        let unknown = r#"{"type":"response.output_item.added","sequence_number":4,"output_index":0,"item":{"type":"message","role":"assistant","content":[]}}"#;

        assert!(matches!(
            serde_json::from_str::<StreamEvent>(created).unwrap(),
            StreamEvent::Created { .. }
        ));
        assert_eq!(
            serde_json::from_str::<StreamEvent>(delta).unwrap(),
            StreamEvent::OutputTextDelta { delta: "Hel".into() }
        );
        assert_eq!(
            serde_json::from_str::<StreamEvent>(rdelta).unwrap(),
            StreamEvent::ReasoningSummaryTextDelta {
                delta: "because".into()
            }
        );
        match serde_json::from_str::<StreamEvent>(completed).unwrap() {
            StreamEvent::Completed { response } => {
                assert_eq!(response.id, "resp_1");
                assert_eq!(response.usage.unwrap().total_tokens, 3);
            }
            other => panic!("unexpected: {other:?}"),
        }
        assert_eq!(
            serde_json::from_str::<StreamEvent>(unknown).unwrap(),
            StreamEvent::Other
        );
    }

    #[test]
    fn deserialize_error_envelope() {
        let raw = r#"{"error":{"message":"bad request","type":"invalid_request_error","param":null,"code":"x"}}"#;
        let envelope: ResponsesResponse = serde_json::from_str(raw).unwrap();
        assert!(envelope.0.is_err());
    }
}
