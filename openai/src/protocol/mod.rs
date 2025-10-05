use serde::{Deserialize, Serialize};
use strum_macros::{EnumIter, IntoStaticStr};
use thiserror::Error;

pub mod chat;
pub mod realtime;
pub mod responses;

#[derive(Error, Debug, Clone, Deserialize, PartialEq, Eq)]
#[error("{error_type}: {message} (param={param:?}, code={code:?}, event_id={event_id:?})")]
pub struct Error {
    pub message: String,
    #[serde(rename = "type")]
    pub error_type: String,
    pub param: Option<String>,
    pub code: Option<String>,
    pub event_id: Option<String>,
}

type Result<T> = std::result::Result<T, Error>;

mod untagged_ok_result {
    use crate::protocol::Error;
    use serde::{Deserialize, Deserializer};

    #[allow(unused)]
    pub(crate) fn deserialize<'de, D, T>(de: D) -> Result<Result<T, Error>, D::Error>
    where
        D: Deserializer<'de>,
        T: Deserialize<'de>,
    {
        serde_untagged::UntaggedEnumVisitor::new()
            .map(|map| {
                let value: serde_json::Value = map.deserialize()?;
                if let Some(error) = value["error"].as_object() {
                    Ok(Err(
                        Error::deserialize(error).map_err(serde::de::Error::custom)?
                    ))
                } else {
                    Ok(Ok(T::deserialize(value).map_err(serde::de::Error::custom)?))
                }
            })
            .deserialize(de)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(transparent)]
pub struct ModelId(pub String);
impl AsRef<str> for ModelId {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Model {
    pub id: ModelId,
    // pub object: String,
    // pub created: u32,
    // pub owned_by: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListModelsResponse {
    // pub object: String,
    pub data: Vec<Model>,
}

/// https://model-spec.openai.com/2025-02-12.html#chain_of_command
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq, IntoStaticStr, EnumIter)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    Developer,
    #[default]
    User,
    Assistant,
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
