use serde::{Deserialize, Serialize};
use thiserror::Error;
use tiktoken_rs::tiktoken::cl100k_base_singleton;

#[derive(Error, Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[error("{kind}: {message}")]
pub struct Error {
    message: String,
    #[serde(rename = "type")]
    kind: String,
    param: Option<String>,
    code: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub enum Model {
    #[serde(rename = "gpt-3.5-turbo")]
    Gpt35Turbo,
    #[serde(rename = "gpt-3.5-turbo-0301")]
    Gpt35Turbo0301,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Request {
    /// ID of the model to use. Currently, only gpt-3.5-turbo and gpt-3.5-turbo-0301 are supported.
    pub model: Model,
    /// The messages to generate chat completions for, in the chat format.
    pub messages: Vec<Message>,
    /// What sampling temperature to use, between 0 and 2. Higher values like
    /// 0.8 will make the output more random, while lower values like 0.2 will
    /// make it more focused and deterministic.
    /// We generally recommend altering this or top_p but not both.
    pub temperature: Option<f32>,
    /// An alternative to sampling with temperature, called nucleus sampling,
    /// where the model considers the results of the tokens with top_p
    /// probability mass. So 0.1 means only the tokens comprising the top 10%
    /// probability mass are considered.
    /// We generally recommend altering this or temperature but not both.
    pub top_p: Option<f32>,
    /// How many chat completion choices to generate for each input message.
    pub n: Option<u32>,
    /// If set, partial message deltas will be sent, like in ChatGPT. Tokens
    /// will be sent as data-only server-sent events as they become available,
    /// with the stream terminated by a data: [DONE] message.
    pub stream: Option<bool>,
    /// Up to 4 sequences where the API will stop generating further tokens.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub stop: Vec<String>,
    /// The maximum number of tokens allowed for the generated answer. By
    /// default, the number of tokens the model can return will be (4096 - prompt
    /// tokens).
    pub max_tokens: Option<u32>,
    /// Number between -2.0 and 2.0. Positive values penalize new tokens based
    /// on whether they appear in the text so far, increasing the model's
    /// likelihood to talk about new topics.
    pub presence_penalty: Option<f32>,
    // logit-bias
    // user
}
impl Default for Request {
    fn default() -> Self {
        Self {
            model: Model::Gpt35Turbo,
            messages: Default::default(),
            temperature: Default::default(),
            top_p: Default::default(),
            n: Default::default(),
            stream: Default::default(),
            stop: Default::default(),
            max_tokens: Default::default(),
            presence_penalty: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}
impl Message {
    pub fn estimate_tokens(&self) -> u32 {
        let bpe = cl100k_base_singleton();
        let bpe = bpe.lock();
        4 + bpe.encode_with_special_tokens(&self.content).len() as u32
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PartialMessage {
    Role(Role),
    Content(String),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Choice {
    pub message: Message,
    // finish_reason
    // index
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PartialChoice {
    pub delta: PartialMessage,
    // finish_reason
    // index
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Completion {
    pub id: String,
    pub object: String,
    pub created: u32,
    pub model: Model,
    pub usage: Usage,
    pub choices: Vec<Choice>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub(crate) enum Response {
    Completion(Completion),
    Error { error: Error },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PartialCompletion {
    pub id: String,
    pub object: String,
    pub created: u32,
    pub model: Model,
    pub choices: Vec<PartialChoice>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub(crate) enum PartialResponse {
    Delta(PartialCompletion),
    Error { error: Error }, // TODO: ???
}
