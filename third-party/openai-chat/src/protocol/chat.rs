use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum_macros::{EnumIter, IntoStaticStr};
use thiserror::Error;
use tiktoken_rs::cl100k_base_singleton;

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
    #[serde(rename = "gpt-4")]
    Gpt4,
    #[serde(rename = "gpt-4-0613")]
    Gpt4_0613,
    #[serde(rename = "gpt-4-32k")]
    Gpt4_32k,
    #[serde(rename = "gpt-4-32k-0613")]
    Gpt4_32k0613,
    #[serde(rename = "gpt-3.5-turbo")]
    #[default]
    Gpt35Turbo,
    #[serde(rename = "gpt-3.5-turbo-0301")]
    Gpt35Turbo0301,
    #[serde(rename = "gpt-3.5-turbo-0613")]
    Gpt35Turbo0613,
    #[serde(rename = "gpt-3.5-turbo-16k")]
    Gpt35Turbo16k,
    #[serde(rename = "gpt-3.5-turbo-16k-0613")]
    Gpt35Turbo16k0613,
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
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
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
        if let Some(content) = &self.content {
            let bpe = cl100k_base_singleton();
            let bpe = bpe.lock();
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
    /// ID of the model to use. Currently, only gpt-3.5-turbo and gpt-3.5-turbo-0301 are supported.
    pub model: Model,
    /// The messages to generate chat completions for, in the chat format.
    pub messages: Vec<Message>,
    /// A list of functions the model may generate JSON inputs for.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub functions: Vec<Function>,
    /// Controls how the model responds to function calls. "none" means the
    /// model does not call a function, and responds to the end-user. "auto"
    /// means the model can pick between an end-user or calling a function.
    /// Specifying a particular function via {"name":\ "my_function"} forces the
    /// model to call that function. "none" is the default when no functions are
    /// present. "auto" is the default if functions are present.
    pub function_call: Option<FunctionUsage>,
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
    // Modify the likelihood of specified tokens appearing in the completion.
    // Accepts a json object that maps tokens (specified by their token ID in
    // the tokenizer) to an associated bias value from -100 to 100.
    // Mathematically, the bias is added to the logits generated by the model
    // prior to sampling. The exact effect will vary per model, but values
    // between -1 and 1 should decrease or increase likelihood of selection;
    // values like -100 or 100 should result in a ban or exclusive selection of
    // the relevant token.
    // logit_bias
    // A unique identifier representing your end-user, which can help OpenAI to
    // monitor and detect abuse.
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
pub(crate) enum Response {
    #[serde(rename = "error")]
    Error(Error),
    #[serde(untagged)]
    Completion(Completion),
}

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
pub(crate) struct PartialResponse(pub PartialCompletion);
