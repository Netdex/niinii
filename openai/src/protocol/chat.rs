use bon::Builder;
use serde::{Deserialize, Serialize};
use strum_macros::{EnumIter, IntoStaticStr};
use tiktoken_rs::cl100k_base_singleton;

use super::{untagged_ok_result, Result};
use crate::{
    protocol::{ReasoningEffort, ServiceTier, StreamOptions, Verbosity},
    ModelId,
};

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Default, Serialize, Builder)]
pub struct Request {
    /// ID of the model to use. Currently, only gpt-3.5-turbo and gpt-3.5-turbo-0301 are supported.
    pub model: ModelId,
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[builder(default)]
    pub stop: Vec<String>,
    /// The maximum number of tokens allowed for the generated answer. By
    /// default, the number of tokens the model can return will be (4096 - prompt
    /// tokens).
    pub max_completion_tokens: Option<u32>,
    /// Number between -2.0 and 2.0. Positive values penalize new tokens based
    /// on whether they appear in the text so far, increasing the model's
    /// likelihood to talk about new topics.
    pub presence_penalty: Option<f32>,
    /// Specifies the processing type used for serving the request.
    /// - If set to 'auto', then the request will be processed with the service
    ///   tier configured in the Project settings. Unless otherwise configured,
    ///   the Project will use 'default'.
    /// - If set to 'default', then the request will be processed with the
    ///   standard pricing and performance for the selected model.
    /// - If set to 'flex' or 'priority', then the request will be processed
    ///   with the corresponding service tier. Contact sales to learn more about
    ///   Priority processing.
    ///
    /// When not set, the default behavior is 'auto'.
    /// When the service_tier parameter is set, the response body will include
    /// the service_tier value based on the processing mode actually used to
    /// serve the request. This response value may be different from the value
    /// set in the parameter.
    pub service_tier: Option<ServiceTier>,
    /// Constrains effort on reasoning for reasoning models. Currently supported
    /// values are minimal, low, medium, and high. Reducing reasoning effort can
    /// result in faster responses and fewer tokens used on reasoning in a
    /// response.
    pub reasoning_effort: Option<ReasoningEffort>,
    /// Constrains the verbosity of the model's response. Lower values will
    /// result in more concise responses, while higher values will result in
    /// more verbose responses. Currently supported values are low, medium, and
    /// high.
    pub verbosity: Option<Verbosity>,
    // logit_bias
    pub(crate) stream: Option<bool>,
    pub(crate) stream_options: Option<StreamOptions>,
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

#[derive(Debug, Clone, Deserialize)]
pub struct PartialMessage {
    pub role: Option<Role>,
    // llama-cpp begins responses with content: null for some reason
    #[serde(default)]
    pub content: Option<String>,
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
    pub completion_tokens_details: Option<CompletionTokensDetails>,
    pub prompt_tokens_details: Option<PromptTokensDetails>,
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
