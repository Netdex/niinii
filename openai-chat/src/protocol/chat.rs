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

    #[default]
    #[serde(rename = "gpt-4o-mini")]
    Gpt4oMini,
    #[serde(rename = "gpt-4o-mini-2024-07-18")]
    Gpt4oMini20240718,
}
impl Model {
    /// https://openai.com/pricing
    pub fn cost(&self, input_tokens: u32, output_tokens: u32) -> f64 {
        let input = input_tokens as f64;
        let output = output_tokens as f64;
        match self {
            Model::Gpt35Turbo | Model::Gpt35Turbo0125 => input * 0.50e-6 + output * 1.50e-6,
            Model::Gpt4o | Model::Gpt4o20240513 => input * 5.00e-6 + output * 15.00e-6,
            Model::Gpt4oMini | Model::Gpt4oMini20240718 => input * 0.15e-6 + output * 0.60e-6,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq, IntoStaticStr, EnumIter)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    #[default]
    User,
    Assistant,
    Function,
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

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Default, Serialize)]
pub struct Request {
    pub model: Model,
    pub messages: Vec<Message>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub n: Option<u32>,
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub stop: Vec<String>,
    pub max_tokens: Option<u32>,
    pub presence_penalty: Option<f32>,
    // logit_bias
    // user
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
