use bon::Builder;
use serde::{Deserialize, Serialize};
use strum_macros::{EnumIter, IntoStaticStr};

use super::{untagged_ok_result, Result};
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
    effort: Option<ReasoningEffort>,
}

// https://platform.openai.com/docs/api-reference/responses/object
// #[derive(Debug, Clone, Deserialize)]
// #[serde(tag = "type")]
// pub enum StreamResponse {
//     ResponseCreated { response: Response },
//     #[serde(rename = "response.done")]
//     ResponseDone { response: Response },
//     #[serde(rename = "response.output_item.added")]
//     ResponseOutputItemAdded(ResponseOutputItem),
//     #[serde(rename = "response.output_item.done")]
//     ResponseOutputItemDone(ResponseOutputItem),
//     #[serde(rename = "response.content_part.added")]
//     ResponseContentPartAdded(ResponseContentPart),
//     #[serde(rename = "response.content_part.done")]
//     ResponseContentPartDone(ResponseContentPart),
//     #[serde(rename = "response.text.delta")]
//     ResponseTextDelta(ResponseTextDelta),
//     #[serde(rename = "response.text.done")]
//     ResponseTextDone(ResponseTextDone),
//     #[serde(rename = "response.function_call_arguments.delta")]
//     ResponseFunctionCallArgumentsDelta(ResponseFunctionCallArgumentsDelta),
//     #[serde(rename = "response.function_call_arguments.done")]
//     ResponseFunctionCallArgumentsDone(ResponseFunctionCallArgumentsDone),
//     #[serde(rename = "rate_limits.updated")]
//     RateLimitsUpdated { rate_limits: Vec<RateLimits> },
// }
