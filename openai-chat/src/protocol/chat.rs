use serde::{Deserialize, Serialize};
use strum_macros::{EnumIter, IntoStaticStr};
use tiktoken_rs::cl100k_base_singleton;

use super::{untagged_ok_result, Result};

#[derive(
    Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq, IntoStaticStr, EnumIter,
)]
pub enum Model {
    #[serde(rename = "gpt-3.5-turbo")]
    Gpt35Turbo,
    #[serde(rename = "gpt-3.5-turbo-0125")]
    Gpt35Turbo0125,

    #[serde(rename = "gpt-4o")]
    Gpt4o,
    #[serde(rename = "gpt-4o-2024-05-13")]
    Gpt4o20240513,

    #[serde(rename = "gpt-4o-mini")]
    Gpt4oMini,
    #[serde(rename = "gpt-4o-mini-2024-07-18")]
    Gpt4oMini20240718,

    #[serde(rename = "gpt-4.1")]
    Gpt41,
    #[serde(rename = "gpt-4.1-2025-04-14")]
    Gpt41_20250414,

    #[serde(rename = "gpt-4.1-mini")]
    Gpt41Mini,
    #[serde(rename = "gpt-4.1-mini-2025-04-14")]
    Gpt41Mini20250414,

    #[serde(rename = "gpt-4.1-nano")]
    Gpt41Nano,
    #[serde(rename = "gpt-4.1-nano-2025-04-14")]
    Gpt41Nano20250414,

    #[serde(rename = "gpt-5")]
    Gpt5,
    #[serde(rename = "gpt-5-2025-08-07")]
    Gpt5_20250807,

    #[default]
    #[serde(rename = "gpt-5-mini")]
    Gpt5Mini,
    #[serde(rename = "gpt-5-mini-2025-08-07")]
    Gpt5Mini20250807,

    #[serde(rename = "gpt-5-nano")]
    Gpt5Nano,
    #[serde(rename = "gpt-5-nano-2025-08-07")]
    Gpt5Nano20250807,
}

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

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Default, Serialize)]
pub struct Request {
    pub model: Model,
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
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
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
    pub model: Model,
    pub usage: Usage,
    pub choices: Vec<Choice>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ChatResponse(
    #[serde(deserialize_with = "untagged_ok_result::deserialize")] pub Result<Completion>,
);

#[derive(Debug, Clone, Deserialize)]
pub struct PartialCompletion {
    pub id: String,
    pub object: String,
    pub created: u32,
    pub model: Model,
    pub choices: Vec<PartialChoice>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(transparent)]
pub(crate) struct StreamResponse(pub PartialCompletion);
