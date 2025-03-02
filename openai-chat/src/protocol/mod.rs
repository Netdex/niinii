use serde::Deserialize;
use thiserror::Error;

pub mod chat;
pub mod moderation;
pub mod realtime;

#[derive(Error, Debug, Clone, Deserialize, PartialEq, Eq)]
#[error("{error_type}: {message} (param={param:?}, code={code:?}, event_id={event_id:?})")]
pub struct Error {
    message: String,
    #[serde(rename = "type")]
    error_type: String,
    param: Option<String>,
    code: Option<String>,
    event_id: Option<String>,
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
