//! Wire types for the GA Realtime API over WebSocket.
//!
//! https://developers.openai.com/api/reference/resources/realtime
//!
//! niinii only uses the text modality: configure a text-only session, push a
//! user message, ask for a response, and stream `response.output_text.delta`
//! events back. Audio, transcription, and turn-detection are not modelled.
//!
//! Only the subset of client/server events the translator needs is typed.
//! Unrecognised server events deserialize to [`ServerEvent::Other`] so the
//! stream is robust to events we don't handle (mirrors `responses::StreamEvent`).

use serde::{Deserialize, Serialize};

use crate::protocol::{ReasoningEffort, Role};

/// `session.type` discriminator. Always `"realtime"` in the GA API.
#[derive(Debug, Clone, Copy, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SessionType {
    #[default]
    Realtime,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Modality {
    Text,
    Audio,
}

/// Reasoning controls for reasoning-capable Realtime models (e.g.
/// `gpt-realtime-2`). Ignored / rejected by non-reasoning realtime models, so it
/// is only sent when explicitly configured.
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct RealtimeReasoning {
    pub effort: Option<ReasoningEffort>,
}

/// `session.truncation`: what the server does when the conversation exceeds the
/// model's input-token limit.
///
/// - [`Auto`](Self::Auto): server default -- drop the oldest messages once the
///   limit is hit.
/// - [`Disabled`](Self::Disabled): never truncate; the server errors instead if
///   the conversation exceeds the limit.
/// - [`RetentionRatio`](Self::RetentionRatio): truncate early, keeping messages
///   up to `ratio` (0.0..=1.0) of the model's max context. Fewer future
///   truncations means a better prompt-cache rate. `post_instructions_token_limit`
///   optionally caps the tokens allowed after the instructions (the
///   `token_limits.post_instructions` field); `None` uses the model default.
///
/// Serializes as the bare string `"auto"` / `"disabled"`, or the object
/// `{"type":"retention_ratio","retention_ratio":<ratio>,"token_limits":{...}}`
/// (the `anyOf` shape of the GA `RealtimeTruncation` schema). Not [`Eq`] because
/// of the `f32` ratio.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RealtimeTruncation {
    Auto,
    Disabled,
    RetentionRatio {
        ratio: f32,
        /// `token_limits.post_instructions`: max tokens kept after the
        /// instructions (incl. tool definitions). `None` omits it (model default).
        post_instructions_token_limit: Option<u32>,
    },
}

impl Serialize for RealtimeTruncation {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            RealtimeTruncation::Auto => serializer.serialize_str("auto"),
            RealtimeTruncation::Disabled => serializer.serialize_str("disabled"),
            RealtimeTruncation::RetentionRatio {
                ratio,
                post_instructions_token_limit,
            } => {
                /// Wire shape of `token_limits` on a retention-ratio truncation.
                #[derive(Serialize)]
                struct TokenLimits {
                    post_instructions: u32,
                }
                use serde::ser::SerializeStruct;
                let len = 2 + post_instructions_token_limit.is_some() as usize;
                let mut s = serializer.serialize_struct("RealtimeTruncationRetentionRatio", len)?;
                s.serialize_field("type", "retention_ratio")?;
                s.serialize_field("retention_ratio", ratio)?;
                if let Some(post_instructions) = post_instructions_token_limit {
                    s.serialize_field(
                        "token_limits",
                        &TokenLimits {
                            post_instructions: *post_instructions,
                        },
                    )?;
                } else {
                    s.skip_field("token_limits")?;
                }
                s.end()
            }
        }
    }
}

/// Text-only session configuration sent in `session.update`. Fields left `None`
/// keep the server defaults.
///
/// Note: the GA `RealtimeSessionCreateRequest` has no `temperature` field (unlike
/// Chat/Responses), so it is intentionally not modelled here.
///
/// Not [`Eq`] because `truncation` can carry an `f32` ratio; equality is still
/// available (`PartialEq`) for the "skip a redundant `session.update`" check.
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct SessionConfig {
    #[serde(rename = "type")]
    pub session_type: SessionType,
    pub output_modalities: Option<Vec<Modality>>,
    pub instructions: Option<String>,
    /// Between 1 and 4096, or omit for the model maximum (`inf`).
    pub max_output_tokens: Option<u32>,
    pub reasoning: Option<RealtimeReasoning>,
    pub truncation: Option<RealtimeTruncation>,
}

impl SessionConfig {
    /// A text-in/text-out session with the given instructions.
    pub fn text(instructions: impl Into<String>) -> Self {
        Self {
            session_type: SessionType::Realtime,
            output_modalities: Some(vec![Modality::Text]),
            instructions: Some(instructions.into()),
            max_output_tokens: None,
            reasoning: None,
            truncation: None,
        }
    }
}

/// One part of a conversation item's content. Only user text input is sent.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum ContentPart {
    #[serde(rename = "input_text")]
    InputText { text: String },
}

#[derive(Debug, Clone, Copy, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ItemType {
    #[default]
    Message,
}

/// A conversation item added via `conversation.item.create`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Item {
    #[serde(rename = "type")]
    pub item_type: ItemType,
    pub role: Role,
    pub content: Vec<ContentPart>,
}

impl Item {
    pub fn user_text(text: impl Into<String>) -> Self {
        Self {
            item_type: ItemType::Message,
            role: Role::User,
            content: vec![ContentPart::InputText { text: text.into() }],
        }
    }
}

/// Per-response overrides for `response.create`. When `None` everywhere, the
/// session defaults apply.
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct ResponseConfig {
    pub instructions: Option<String>,
    pub max_output_tokens: Option<u32>,
}

/// Client -> server events. `#[serde(tag = "type")]` emits the dotted event
/// name as the `type` field.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "type")]
pub enum ClientEvent {
    #[serde(rename = "session.update")]
    SessionUpdate { session: SessionConfig },
    #[serde(rename = "conversation.item.create")]
    ConversationItemCreate { item: Item },
    #[serde(rename = "response.create")]
    ResponseCreate {
        #[serde(skip_serializing_if = "Option::is_none")]
        response: Option<ResponseConfig>,
    },
    #[serde(rename = "response.cancel")]
    ResponseCancel,
}

/// Input-token breakdown on a completed response's usage. `cached_tokens`
/// surfaces prompt-cache hits.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct InputTokenDetails {
    pub cached_tokens: u32,
    pub text_tokens: u32,
    pub audio_tokens: u32,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct OutputTokenDetails {
    pub text_tokens: u32,
    pub audio_tokens: u32,
}

/// Realtime usage object. Note the singular `input_token_details` /
/// `output_token_details` keys (the Responses API uses the plural
/// `input_tokens_details`). No reasoning-token field.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct RealtimeUsage {
    pub total_tokens: u32,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub input_token_details: Option<InputTokenDetails>,
    pub output_token_details: Option<OutputTokenDetails>,
}

/// Error payload carried by an `error` event or a failed `response.done`.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct RealtimeError {
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub message: String,
    #[serde(rename = "type", default)]
    pub error_type: Option<String>,
}

/// `status_details` on a `response.done` whose `status` is not `completed`
/// (e.g. `failed`, `incomplete`, `cancelled`).
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct StatusDetails {
    #[serde(rename = "type")]
    pub detail_type: Option<String>,
    pub reason: Option<String>,
    pub error: Option<RealtimeError>,
}

/// Minimal `response` object on `response.created`.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ResponseMeta {
    pub id: String,
}

/// `response` object on `response.done`.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ResponseObject {
    pub id: String,
    pub status: Option<String>,
    pub status_details: Option<StatusDetails>,
    pub usage: Option<RealtimeUsage>,
}

/// Server -> client events. Only the text-translation subset is typed; anything
/// else deserializes to [`ServerEvent::Other`].
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum ServerEvent {
    #[serde(rename = "response.created")]
    ResponseCreated { response: ResponseMeta },
    #[serde(rename = "response.output_text.delta")]
    OutputTextDelta {
        #[serde(default)]
        response_id: String,
        delta: String,
    },
    #[serde(rename = "response.output_text.done")]
    OutputTextDone { text: String },
    #[serde(rename = "response.done")]
    ResponseDone { response: ResponseObject },
    #[serde(rename = "error")]
    Error { error: RealtimeError },
    #[serde(other)]
    Other,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_update_serializes_text_only() {
        let ev = ClientEvent::SessionUpdate {
            session: SessionConfig::text("You are a translator."),
        };
        let json = serde_json::to_value(&ev).unwrap();
        assert_eq!(json["type"], "session.update");
        assert_eq!(json["session"]["type"], "realtime");
        assert_eq!(json["session"]["output_modalities"][0], "text");
        assert_eq!(json["session"]["instructions"], "You are a translator.");
        // Omitted optionals must not serialize.
        assert!(json["session"].get("reasoning").is_none());
        assert!(json["session"].get("max_output_tokens").is_none());
    }

    #[test]
    fn session_update_serializes_reasoning_effort() {
        let mut session = SessionConfig::text("hi");
        session.reasoning = Some(RealtimeReasoning {
            effort: Some(ReasoningEffort::Low),
        });
        let ev = ClientEvent::SessionUpdate { session };
        let json = serde_json::to_value(&ev).unwrap();
        assert_eq!(json["session"]["reasoning"]["effort"], "low");
    }

    #[test]
    fn truncation_serializes_each_form() {
        let mut session = SessionConfig::text("hi");
        // Omitted by default.
        let json = serde_json::to_value(&session).unwrap();
        assert!(json.get("truncation").is_none());

        // String forms.
        session.truncation = Some(RealtimeTruncation::Auto);
        assert_eq!(serde_json::to_value(&session).unwrap()["truncation"], "auto");
        session.truncation = Some(RealtimeTruncation::Disabled);
        assert_eq!(
            serde_json::to_value(&session).unwrap()["truncation"],
            "disabled"
        );

        // Object form, no token limit -> token_limits omitted.
        session.truncation = Some(RealtimeTruncation::RetentionRatio {
            ratio: 0.8,
            post_instructions_token_limit: None,
        });
        let json = serde_json::to_value(&session).unwrap();
        assert_eq!(json["truncation"]["type"], "retention_ratio");
        let ratio = json["truncation"]["retention_ratio"].as_f64().unwrap();
        assert!((ratio - 0.8).abs() < 1e-6, "got {ratio}");
        assert!(json["truncation"].get("token_limits").is_none());

        // Object form with a post-instructions token limit.
        session.truncation = Some(RealtimeTruncation::RetentionRatio {
            ratio: 0.5,
            post_instructions_token_limit: Some(4096),
        });
        let json = serde_json::to_value(&session).unwrap();
        assert_eq!(json["truncation"]["token_limits"]["post_instructions"], 4096);
    }

    #[test]
    fn conversation_item_create_serializes_user_text() {
        let ev = ClientEvent::ConversationItemCreate {
            item: Item::user_text("hello"),
        };
        let json = serde_json::to_value(&ev).unwrap();
        assert_eq!(json["type"], "conversation.item.create");
        assert_eq!(json["item"]["type"], "message");
        assert_eq!(json["item"]["role"], "user");
        assert_eq!(json["item"]["content"][0]["type"], "input_text");
        assert_eq!(json["item"]["content"][0]["text"], "hello");
    }

    #[test]
    fn response_create_omits_empty_config() {
        let ev = ClientEvent::ResponseCreate { response: None };
        let json = serde_json::to_value(&ev).unwrap();
        assert_eq!(json["type"], "response.create");
        assert!(json.get("response").is_none());
    }

    #[test]
    fn deserialize_text_delta_and_done() {
        let delta = r#"{"type":"response.output_text.delta","response_id":"resp_1","item_id":"item_1","output_index":0,"content_index":0,"delta":"Hel"}"#;
        assert_eq!(
            serde_json::from_str::<ServerEvent>(delta).unwrap(),
            ServerEvent::OutputTextDelta {
                response_id: "resp_1".into(),
                delta: "Hel".into()
            }
        );
        let done = r#"{"type":"response.output_text.done","response_id":"resp_1","item_id":"item_1","output_index":0,"content_index":0,"text":"Hello"}"#;
        assert_eq!(
            serde_json::from_str::<ServerEvent>(done).unwrap(),
            ServerEvent::OutputTextDone {
                text: "Hello".into()
            }
        );
    }

    #[test]
    fn deserialize_response_done_with_usage() {
        let raw = r#"{
            "type":"response.done",
            "response":{
                "id":"resp_1",
                "status":"completed",
                "usage":{
                    "total_tokens":30,
                    "input_tokens":20,
                    "output_tokens":10,
                    "input_token_details":{"cached_tokens":8,"text_tokens":12,"audio_tokens":0},
                    "output_token_details":{"text_tokens":10,"audio_tokens":0}
                }
            }
        }"#;
        match serde_json::from_str::<ServerEvent>(raw).unwrap() {
            ServerEvent::ResponseDone { response } => {
                assert_eq!(response.id, "resp_1");
                assert_eq!(response.status.as_deref(), Some("completed"));
                let usage = response.usage.unwrap();
                assert_eq!(usage.total_tokens, 30);
                assert_eq!(usage.input_token_details.unwrap().cached_tokens, 8);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn deserialize_error_event() {
        let raw = r#"{"type":"error","error":{"type":"invalid_request_error","code":"bad","message":"nope"}}"#;
        match serde_json::from_str::<ServerEvent>(raw).unwrap() {
            ServerEvent::Error { error } => {
                assert_eq!(error.message, "nope");
                assert_eq!(error.code.as_deref(), Some("bad"));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn unknown_event_is_tolerated() {
        let raw = r#"{"type":"response.output_audio.delta","delta":"=="}"#;
        assert_eq!(
            serde_json::from_str::<ServerEvent>(raw).unwrap(),
            ServerEvent::Other
        );
    }
}
