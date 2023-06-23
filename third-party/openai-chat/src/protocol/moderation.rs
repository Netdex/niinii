use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use strum_macros::AsRefStr;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub enum Model {
    #[serde(rename = "text-moderation-latest")]
    TextModerationLatest,
    #[serde(rename = "text-moderation-stable")]
    TextModerationStable,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Default, Serialize)]
pub struct Request {
    /// Two content moderations models are available: text-moderation-stable and
    /// text-moderation-latest.
    /// The default is text-moderation-latest which will be automatically
    /// upgraded over time. This ensures you are always using our most accurate
    /// model. If you use text-moderation-stable, we will provide advanced
    /// notice before updating the model. Accuracy of text-moderation-stable may
    /// be slightly lower than for text-moderation-latest.
    pub model: Option<Model>,
    /// The input text to classify
    pub input: String, // Vec<String> + String
}

#[derive(Debug, Clone, Deserialize, PartialOrd, Ord, PartialEq, Eq, AsRefStr)]
pub enum Category {
    #[serde(rename = "hate")]
    Hate,
    #[serde(rename = "hate/threatening")]
    HateThreatening,
    #[serde(rename = "self-harm")]
    SelfHarm,
    #[serde(rename = "sexual")]
    Sexual,
    #[serde(rename = "sexual/minors")]
    SexualMinors,
    #[serde(rename = "violence")]
    Violence,
    #[serde(rename = "violence/graphic")]
    ViolenceGraphic,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Result {
    pub categories: BTreeMap<Category, bool>,
    pub category_scores: BTreeMap<Category, f64>,
    pub flagged: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Response {
    pub id: String,
    pub model: String,
    pub results: Vec<Result>,
}
