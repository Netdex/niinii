use bon::Builder;
use serde::{Deserialize, Serialize};

use crate::{
    protocol::{ReasoningEffort, Role, StreamOptions, Verbosity},
    ModelId,
};

/// https://platform.openai.com/docs/api-reference/responses/create
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Default, Serialize, Builder)]
pub struct Request {
    pub input: Vec<Message>,
    pub instructions: Option<String>,
    pub max_output_tokens: Option<u32>,
    pub model: ModelId,
    pub reasoning: Option<ReasoningOptions>,
    pub context_management: Option<Vec<ContextManagementEntry>>,
    pub conversation: Option<String>,
    pub previous_response_id: Option<String>,
    pub store: Option<bool>,
    pub(crate) stream: Option<bool>,
    pub(crate) stream_options: Option<StreamOptions>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub verbosity: Option<Verbosity>,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Message {
    pub role: Role,
    pub content: Option<String>,
}
impl Default for Message {
    fn default() -> Self {
        Self {
            role: Role::User,
            content: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ReasoningOptions {
    pub effort: Option<ReasoningEffort>,
}
impl ReasoningOptions {
    pub fn with_effort(effort: ReasoningEffort) -> Self {
        Self {
            effort: Some(effort),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ContextManagementEntry {
    #[serde(rename = "type")]
    pub entry_type: ContextManagementType,
    pub compact_threshold: Option<u32>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ContextManagementType {
    Compaction,
}

// https://platform.openai.com/docs/api-reference/responses/object
#[derive(Debug, Clone, Deserialize)]
pub struct Response {
    pub id: String,
    pub object: String,
    pub model: ModelId,
    #[serde(default)]
    pub output: Vec<OutputItem>,
    pub conversation: Option<ConversationRef>,
    pub previous_response_id: Option<String>,
    #[serde(default)]
    pub store: bool,
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConversationRef {
    pub id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub total_tokens: u32,
    pub input_tokens_details: Option<InputTokensDetails>,
    pub output_tokens_details: Option<OutputTokensDetails>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InputTokensDetails {
    #[serde(default)]
    pub cached_tokens: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OutputTokensDetails {
    #[serde(default)]
    pub reasoning_tokens: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum OutputItem {
    #[serde(rename = "message")]
    Message(OutputMessage),
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OutputMessage {
    pub id: String,
    pub role: Role,
    #[serde(default)]
    pub content: Vec<MessageContent>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum MessageContent {
    #[serde(rename = "output_text")]
    OutputText(OutputTextContent),
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OutputTextContent {
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum StreamEvent {
    #[serde(rename = "response.created")]
    ResponseCreated { response: Response },
    #[serde(rename = "response.completed")]
    ResponseCompleted { response: Response },
    #[serde(rename = "response.output_text.delta")]
    OutputTextDelta { delta: String },
    #[serde(rename = "response.output_text.done")]
    OutputTextDone,
    #[serde(other)]
    Unknown,
}
