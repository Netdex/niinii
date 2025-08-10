use serde::{Deserialize, Serialize};
use strum_macros::{EnumIter, IntoStaticStr};
use tiktoken_rs::cl100k_base_singleton;

use crate::ModelId;

use super::{untagged_ok_result, Result};

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq, IntoStaticStr, EnumIter)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    #[default]
    User,
    Assistant,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Message {
    pub role: Role,
    pub content: Option<String>,
    pub name: Option<String>,
}
impl Message {
    pub fn estimate_tokens(&self) -> u32 {
        // https://platform.openai.com/docs/guides/text-generation/managing-tokens
        if let Some(content) = &self.content {
            let bpe = cl100k_base_singleton();
            let bpe = bpe.lock();
            // every message follows <im_start>{role/name}\n{content}<im_end>\n
            4 + bpe.encode_with_special_tokens(content).len() as u32
        } else {
            0
        }
    }
}
impl Default for Message {
    fn default() -> Self {
        Self {
            role: Role::User,
            content: Default::default(),
            name: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct StreamOptions {
    pub include_obfuscation: bool,
    pub include_usage: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, IntoStaticStr, EnumIter)]
#[serde(rename_all = "lowercase")]
pub enum ServiceTier {
    Auto,
    Default,
    Flex,
    Priority,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, IntoStaticStr, EnumIter)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningEffort {
    Minimal,
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, IntoStaticStr, EnumIter)]
#[serde(rename_all = "lowercase")]
pub enum Verbosity {
    Low,
    Medium,
    High,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Default, Serialize)]
pub struct Request {
    pub model: ModelId,
    pub messages: Vec<Message>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub n: Option<u32>,
    pub stream: Option<bool>,
    pub stream_options: Option<StreamOptions>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub stop: Vec<String>,
    pub max_completion_tokens: Option<u32>,
    pub presence_penalty: Option<f32>,
    pub service_tier: Option<ServiceTier>,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub verbosity: Option<Verbosity>,
    // logit_bias
}

#[derive(Debug, Clone, Deserialize)]
pub struct PartialMessage {
    pub role: Option<Role>,
    #[serde(default)]
    pub content: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Choice {
    pub message: Message,
    // finish_reason
    // index
}

#[derive(Debug, Clone, Deserialize)]
pub struct CompletionTokensDetails {
    pub accepted_prediction_tokens: u32,
    pub audio_tokens: u32,
    pub reasoning_tokens: u32,
    pub rejected_prediction_tokens: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PromptTokensDetails {
    pub audio_tokens: u32,
    pub cached_tokens: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    pub completion_tokens_details: CompletionTokensDetails,
    pub prompt_tokens_details: PromptTokensDetails,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PartialChoice {
    pub delta: PartialMessage,
    // finish_reason
    // index
}

#[derive(Debug, Clone, Deserialize)]
pub struct Completion {
    pub id: String,
    pub object: String,
    pub created: u32,
    pub model: ModelId,
    pub usage: Usage,
    pub choices: Vec<Choice>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ChatResponse(
    #[serde(deserialize_with = "untagged_ok_result::deserialize")] pub Result<Completion>,
);

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Deserialize)]
pub struct PartialCompletion {
    pub id: String,
    pub object: String,
    pub created: u32,
    pub model: ModelId,
    pub choices: Vec<PartialChoice>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(transparent)]
pub(crate) struct StreamResponse(pub PartialCompletion);
