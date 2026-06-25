//! https://platform.openai.com/docs/api-reference/responses

use bon::Builder;
use serde::{Deserialize, Serialize};
use strum_macros::{EnumIter, IntoStaticStr};

use super::{untagged_ok_result, Result};
use crate::{
    protocol::{ReasoningEffort, Role, ServiceTier, Verbosity},
    ModelId,
};

/// The `input` field accepts either a bare string or a list of structured
/// items. niinii only ever sends a single user line, but the array form is
/// kept available for completeness.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum Input {
    Text(String),
    Items(Vec<InputItem>),
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InputItem {
    pub role: Role,
    pub content: String,
}

/// Reasoning controls for thinking-capable models. `summary` is opt-in because
/// generating a summary costs extra latency.
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub struct Reasoning {
    pub effort: Option<ReasoningEffort>,
    pub summary: Option<ReasoningSummary>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, IntoStaticStr, EnumIter)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningSummary {
    Auto,
    Concise,
    Detailed,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub struct TextConfig {
    pub verbosity: Option<Verbosity>,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Builder)]
pub struct Request {
    /// ID of the model to use.
    pub model: ModelId,
    /// Text or structured input to the model.
    pub input: Input,
    /// System/developer instructions. Unlike the Chat Completions system
    /// message, instructions are NOT inherited across `previous_response_id`
    /// and must be resent on every request.
    pub instructions: Option<String>,
    /// Whether to persist the response so it can be referenced by a subsequent
    /// request's `previous_response_id`. niinii sets this `true` to chain turns.
    pub store: Option<bool>,
    /// The id of a previous response to continue from. Lets the server hold
    /// conversation state so each turn only uploads the new input.
    pub previous_response_id: Option<String>,
    /// Upper bound on generated tokens (including reasoning tokens).
    pub max_output_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    /// Reasoning controls (effort + optional summary).
    pub reasoning: Option<Reasoning>,
    /// Output controls (currently just verbosity).
    pub text: Option<TextConfig>,
    pub service_tier: Option<ServiceTier>,
    /// Stable key that routes requests to the same prompt cache, raising the
    /// prefix-cache hit rate and lowering time-to-first-token.
    pub prompt_cache_key: Option<String>,
    pub(crate) stream: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, IntoStaticStr, EnumIter)]
#[serde(rename_all = "snake_case")]
pub enum ResponseStatus {
    Completed,
    Failed,
    Incomplete,
    InProgress,
    Queued,
    Cancelled,
}

/// Error object carried on a response with `status == Failed`. The Responses
/// API shape (`{code, message}`) differs from the top-level API error envelope,
/// so it is modelled separately.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ResponseError {
    #[serde(default)]
    pub code: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct InputTokensDetails {
    pub cached_tokens: u32,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct OutputTokensDetails {
    pub reasoning_tokens: u32,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub total_tokens: u32,
    pub input_tokens_details: Option<InputTokensDetails>,
    pub output_tokens_details: Option<OutputTokensDetails>,
}

/// A single piece of message content. Only `output_text` is consumed; other
/// content types (refusals, etc.) are tolerated and ignored.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutputContent {
    OutputText {
        #[serde(default)]
        text: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct SummaryText {
    #[serde(default)]
    pub text: String,
}

/// One item in a response's `output` array. Unknown item types deserialize to
/// `Other` rather than failing.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutputItem {
    Message {
        #[serde(default)]
        role: Role,
        #[serde(default)]
        content: Vec<OutputContent>,
    },
    Reasoning {
        #[serde(default)]
        summary: Vec<SummaryText>,
    },
    FunctionCall {
        #[serde(default)]
        name: String,
        #[serde(default)]
        arguments: String,
        #[serde(default)]
        call_id: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Response {
    pub id: String,
    #[serde(default)]
    pub object: String,
    #[serde(default)]
    pub created_at: u64,
    #[serde(default)]
    pub model: ModelId,
    pub status: ResponseStatus,
    #[serde(default)]
    pub error: Option<ResponseError>,
    #[serde(default)]
    pub output: Vec<OutputItem>,
    #[serde(default)]
    pub usage: Option<Usage>,
}

impl Response {
    /// Concatenate all assistant message text in `output`.
    pub fn output_text(&self) -> String {
        let mut out = String::new();
        for item in &self.output {
            if let OutputItem::Message { content, .. } = item {
                for c in content {
                    if let OutputContent::OutputText { text } = c {
                        out.push_str(text);
                    }
                }
            }
        }
        out
    }

    /// Concatenate all reasoning summary text in `output`, if any.
    pub fn reasoning_text(&self) -> String {
        let mut out = String::new();
        for item in &self.output {
            if let OutputItem::Reasoning { summary } = item {
                for s in summary {
                    out.push_str(&s.text);
                }
            }
        }
        out
    }
}

/// Typed streaming events. Each SSE `data` payload carries a `type`
/// discriminator. Unhandled event types deserialize to `Other` so the stream
/// is robust to new server-side events.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum StreamEvent {
    #[serde(rename = "response.created")]
    Created { response: Response },
    #[serde(rename = "response.output_text.delta")]
    OutputTextDelta { delta: String },
    #[serde(rename = "response.reasoning_summary_text.delta")]
    ReasoningSummaryTextDelta { delta: String },
    #[serde(rename = "response.completed")]
    Completed { response: Response },
    #[serde(rename = "response.incomplete")]
    Incomplete { response: Response },
    #[serde(rename = "response.failed")]
    Failed { response: Response },
    #[serde(rename = "error")]
    Error {
        #[serde(default)]
        code: Option<String>,
        #[serde(default)]
        message: String,
    },
    #[serde(other)]
    Other,
}

/// Non-streaming response envelope: either a `Response` object or an `{error}`
/// payload, disambiguated the same way as the Chat Completions response.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ResponsesResponse(
    #[serde(deserialize_with = "untagged_ok_result::deserialize")] pub Result<Response>,
);
