use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum_macros::{EnumIter, IntoStaticStr};
use thiserror::Error;
use tiktoken_rs::cl100k_base_singleton;

use super::Response;

#[derive(Error, Debug, Clone, Deserialize, PartialEq, Eq)]
#[error("{kind}: {message}")]
pub struct Error {
    message: String,
    #[serde(rename = "type")]
    kind: String,
    param: Option<String>,
    code: Option<String>,
}

#[derive(
    Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq, IntoStaticStr, EnumIter,
)]
pub enum Model {
    #[default]
    #[serde(rename = "gpt-3.5-turbo")]
    Gpt35Turbo,
    #[serde(rename = "gpt-3.5-turbo-0613")]
    Gpt35Turbo0613,
    #[serde(rename = "gpt-3.5-turbo-1106")]
    Gpt35Turbo1106,
    #[serde(rename = "gpt-3.5-turbo-0125")]
    Gpt35Turbo0125,

    #[serde(rename = "gpt-4")]
    Gpt4,
    #[serde(rename = "gpt-4-0613")]
    Gpt4_0613,
    #[serde(rename = "gpt-4-32k")]
    Gpt4_32k,

    #[serde(rename = "gpt-4-1106-preview")]
    Gpt4_1106Preview,
    #[serde(rename = "gpt-4-0125-preview")]
    Gpt4_0125Preview,
}
impl Model {
    /// https://openai.com/pricing
    pub fn cost(&self, input_tokens: u32, output_tokens: u32) -> f64 {
        let input = input_tokens as f64 / 1000.0;
        let output = output_tokens as f64 / 1000.0;
        match self {
            Model::Gpt35Turbo
            | Model::Gpt35Turbo0613
            | Model::Gpt35Turbo1106
            | Model::Gpt35Turbo0125 => input * 0.0005 + output * 0.0015,
            Model::Gpt4 | Model::Gpt4_0613 => input * 0.03 + output * 0.06,
            Model::Gpt4_32k => input * 0.06 + output * 0.12,
            Model::Gpt4_1106Preview | Model::Gpt4_0125Preview => input * 0.01 + output * 0.03,
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

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FunctionUsage {
    None,
    Auto,
    Name(String),
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Function {
    name: String,
    description: Option<String>,
    #[serde(skip_serializing_if = "Value::is_null")]
    parameters: Value,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct FunctionCall {
    name: String,
    arguments: Value,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Message {
    pub role: Role,
    pub content: Option<String>,
    pub name: Option<String>,
    pub function_call: Option<FunctionCall>,
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
            function_call: None,
        }
    }
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Default, Serialize)]
pub struct Request {
    pub model: Model,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub functions: Vec<Function>,
    pub function_call: Option<FunctionUsage>,
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
pub(crate) type ChatResponse = Response<Completion>;

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
