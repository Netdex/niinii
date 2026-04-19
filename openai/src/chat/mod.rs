//! https://platform.openai.com/docs/api-reference/chat

use eventsource_stream::Eventsource;
use reqwest::Method;
use tokio_stream::{Stream, StreamExt};
use tracing::Level;

pub use crate::protocol::chat::{
    FunctionCall, FunctionDef, Message, PartialFunctionCall, PartialMessage, PartialToolCall,
    Request, Role, Tool, ToolCall, ToolCallKind, ToolChoice, ToolChoiceMode, Usage,
};

/// Accumulates streaming `PartialToolCall` fragments (keyed by `index`) into
/// complete [`ToolCall`]s once the model stops emitting chunks.
///
/// OpenAI sends tool-call id/name/type once on the first chunk under an index,
/// and then streams `function.arguments` as string fragments on subsequent
/// chunks. The accumulator concatenates them.
#[derive(Debug, Default, Clone)]
pub struct ToolCallAccumulator {
    slots: Vec<Option<ToolCall>>,
}

impl ToolCallAccumulator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, partial: PartialToolCall) {
        let idx = partial.index as usize;
        if self.slots.len() <= idx {
            self.slots.resize(idx + 1, None);
        }
        let slot = self.slots[idx].get_or_insert_with(|| ToolCall {
            id: String::new(),
            kind: ToolCallKind::Function,
            function: FunctionCall::default(),
        });
        if let Some(id) = partial.id {
            slot.id = id;
        }
        if let Some(kind) = partial.kind {
            slot.kind = kind;
        }
        if let Some(function) = partial.function {
            if let Some(name) = function.name {
                slot.function.name = name;
            }
            if let Some(args) = function.arguments {
                slot.function.arguments.push_str(&args);
            }
        }
    }

    pub fn extend<I: IntoIterator<Item = PartialToolCall>>(&mut self, iter: I) {
        for partial in iter {
            self.push(partial);
        }
    }

    /// Drain completed tool calls in index order. Partial slots (never filled)
    /// are skipped.
    pub fn finish(self) -> Vec<ToolCall> {
        self.slots.into_iter().flatten().collect()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.iter().all(Option::is_none)
    }
}

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
    use super::*;

    fn weather_tool() -> Tool {
        Tool::function(FunctionDef {
            name: "get_weather".into(),
            description: Some("Get the current weather for a city".into()),
            parameters: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "location": { "type": "string" },
                    "unit": { "type": "string", "enum": ["celsius", "fahrenheit"] }
                },
                "required": ["location"],
                "additionalProperties": false,
            })),
            strict: Some(true),
        })
    }

    #[test]
    fn tool_serializes_as_function_type() {
        let json = serde_json::to_value(weather_tool()).unwrap();
        assert_eq!(json["type"], "function");
        assert_eq!(json["function"]["name"], "get_weather");
        assert_eq!(json["function"]["strict"], true);
        assert_eq!(json["function"]["parameters"]["required"][0], "location");
    }

    #[test]
    fn tool_choice_serialization_modes() {
        assert_eq!(
            serde_json::to_value(ToolChoice::Mode(ToolChoiceMode::Auto)).unwrap(),
            serde_json::json!("auto")
        );
        assert_eq!(
            serde_json::to_value(ToolChoice::Mode(ToolChoiceMode::None)).unwrap(),
            serde_json::json!("none")
        );
        assert_eq!(
            serde_json::to_value(ToolChoice::Mode(ToolChoiceMode::Required)).unwrap(),
            serde_json::json!("required")
        );
        let forced = serde_json::to_value(ToolChoice::function("get_weather")).unwrap();
        assert_eq!(forced["type"], "function");
        assert_eq!(forced["function"]["name"], "get_weather");
    }

    #[test]
    fn request_includes_tools_and_tool_choice() {
        let request = Request {
            model: crate::ModelId("gpt-4o-mini".into()),
            messages: vec![Message {
                role: Role::User,
                content: Some("weather in Tokyo?".into()),
                ..Default::default()
            }],
            tools: Some(vec![weather_tool()]),
            tool_choice: Some(ToolChoice::Mode(ToolChoiceMode::Auto)),
            parallel_tool_calls: Some(false),
            ..Default::default()
        };
        let json = serde_json::to_value(&request).unwrap();
        assert!(json["tools"].is_array());
        assert_eq!(json["tools"][0]["function"]["name"], "get_weather");
        assert_eq!(json["tool_choice"], "auto");
        assert_eq!(json["parallel_tool_calls"], false);
        // Omitted fields must not serialize.
        assert!(json.get("temperature").is_none());
    }

    #[test]
    fn request_omits_tool_fields_when_none() {
        let request = Request {
            model: crate::ModelId("gpt-4o-mini".into()),
            messages: vec![],
            ..Default::default()
        };
        let json = serde_json::to_value(&request).unwrap();
        assert!(json.get("tools").is_none());
        assert!(json.get("tool_choice").is_none());
        assert!(json.get("parallel_tool_calls").is_none());
    }

    #[test]
    fn deserialize_assistant_tool_call_completion() {
        let raw = r#"{
            "id": "chatcmpl-abc",
            "object": "chat.completion",
            "created": 1700000000,
            "model": "gpt-4o-mini",
            "choices": [{
                "index": 0,
                "finish_reason": "tool_calls",
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "get_weather",
                            "arguments": "{\"location\":\"Tokyo\"}"
                        }
                    }]
                }
            }],
            "usage": { "prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15 }
        }"#;
        let completion: chat::Completion = serde_json::from_str(raw).unwrap();
        let choice = &completion.choices[0];
        assert_eq!(choice.finish_reason.as_deref(), Some("tool_calls"));
        let calls = choice.message.tool_calls.as_ref().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_1");
        assert_eq!(calls[0].kind, ToolCallKind::Function);
        assert_eq!(calls[0].function.name, "get_weather");
        #[derive(serde::Deserialize)]
        struct Args {
            location: String,
        }
        let args: Args = calls[0].parse_arguments().unwrap();
        assert_eq!(args.location, "Tokyo");
    }

    #[test]
    fn deserialize_llama_cpp_style_tool_call_without_id_or_type() {
        // llama.cpp's OpenAI-compatible server may omit `id` and `type` on
        // tool_call objects; clients must tolerate both.
        let raw = r#"{
            "id": "chatcmpl-llama",
            "object": "chat.completion",
            "created": 1700000000,
            "model": "llama-3.1-8b-instruct",
            "choices": [{
                "index": 0,
                "finish_reason": "tool_calls",
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "function": {
                            "name": "get_weather",
                            "arguments": "{\"location\":\"Tokyo\"}"
                        }
                    }]
                }
            }],
            "usage": { "prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15 }
        }"#;
        let completion: chat::Completion = serde_json::from_str(raw).unwrap();
        let call = &completion.choices[0].message.tool_calls.as_ref().unwrap()[0];
        assert_eq!(call.id, "");
        assert_eq!(call.kind, ToolCallKind::Function);
        assert_eq!(call.function.name, "get_weather");
    }

    #[test]
    fn tool_result_message_roundtrips() {
        let msg = Message::tool_result("call_1", "22C, clear");
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["role"], "tool");
        assert_eq!(json["tool_call_id"], "call_1");
        assert_eq!(json["content"], "22C, clear");
        // Unused fields must not serialize.
        assert!(json.get("name").is_none());
        assert!(json.get("tool_calls").is_none());
        let round: Message = serde_json::from_value(json).unwrap();
        assert_eq!(round.role, Role::Tool);
        assert_eq!(round.tool_call_id.as_deref(), Some("call_1"));
    }

    #[test]
    fn accumulator_merges_streamed_chunks() {
        // Canonical streaming pattern: first chunk carries id+name, subsequent
        // chunks stream arguments as string fragments. OpenAI can also interleave
        // multiple tool calls under different `index` values.
        let chunks: Vec<PartialToolCall> = vec![
            PartialToolCall {
                index: 0,
                id: Some("call_a".into()),
                kind: Some(ToolCallKind::Function),
                function: Some(PartialFunctionCall {
                    name: Some("get_weather".into()),
                    arguments: Some(String::new()),
                }),
            },
            PartialToolCall {
                index: 1,
                id: Some("call_b".into()),
                kind: Some(ToolCallKind::Function),
                function: Some(PartialFunctionCall {
                    name: Some("get_time".into()),
                    arguments: Some(String::new()),
                }),
            },
            PartialToolCall {
                index: 0,
                function: Some(PartialFunctionCall {
                    arguments: Some("{\"loc".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            PartialToolCall {
                index: 1,
                function: Some(PartialFunctionCall {
                    arguments: Some("{\"tz\":\"JST\"}".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            PartialToolCall {
                index: 0,
                function: Some(PartialFunctionCall {
                    arguments: Some("ation\":\"Tokyo\"}".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
        ];
        let mut acc = ToolCallAccumulator::new();
        acc.extend(chunks);
        let calls = acc.finish();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].id, "call_a");
        assert_eq!(calls[0].function.name, "get_weather");
        assert_eq!(calls[0].function.arguments, "{\"location\":\"Tokyo\"}");
        assert_eq!(calls[1].id, "call_b");
        assert_eq!(calls[1].function.arguments, "{\"tz\":\"JST\"}");
    }

    #[test]
    fn deserialize_stream_chunks_with_tool_call_deltas() {
        // First delta: role + tool call skeleton.
        let first = r#"{
            "id":"chatcmpl-1","object":"chat.completion.chunk","created":1,"model":"gpt-4o-mini",
            "choices":[{"index":0,"delta":{"role":"assistant","tool_calls":[
                {"index":0,"id":"call_1","type":"function","function":{"name":"get_weather","arguments":""}}
            ]},"finish_reason":null}]
        }"#;
        // Second delta: argument fragment, no id/name.
        let second = r#"{
            "id":"chatcmpl-1","object":"chat.completion.chunk","created":1,"model":"gpt-4o-mini",
            "choices":[{"index":0,"delta":{"tool_calls":[
                {"index":0,"function":{"arguments":"{\"location\":\"Tokyo\"}"}}
            ]},"finish_reason":null}]
        }"#;
        // Final delta: finish_reason.
        let third = r#"{
            "id":"chatcmpl-1","object":"chat.completion.chunk","created":1,"model":"gpt-4o-mini",
            "choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]
        }"#;
        let mut acc = ToolCallAccumulator::new();
        let mut last_finish = None;
        for raw in [first, second, third] {
            let partial: chat::PartialCompletion = serde_json::from_str(raw).unwrap();
            let choice = &partial.choices[0];
            if let Some(calls) = &choice.delta.tool_calls {
                acc.extend(calls.clone());
            }
            if choice.finish_reason.is_some() {
                last_finish.clone_from(&choice.finish_reason);
            }
        }
        assert_eq!(last_finish.as_deref(), Some("tool_calls"));
        let calls = acc.finish();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_1");
        assert_eq!(calls[0].function.name, "get_weather");
        assert_eq!(calls[0].function.arguments, "{\"location\":\"Tokyo\"}");
    }

}
