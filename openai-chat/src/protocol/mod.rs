use serde::Deserialize;
use thiserror::Error;

pub mod chat;
pub mod moderation;

#[derive(Error, Debug, Clone, Deserialize, PartialEq, Eq)]
#[error("{kind}: {message}")]
pub struct Error {
    message: String,
    #[serde(rename = "type")]
    kind: String,
    param: Option<String>,
    code: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub enum Response<T> {
    #[serde(rename = "error")]
    Error(Error),
    #[serde(untagged)]
    Ok(T),
}
impl<T> From<Response<T>> for Result<T, Error> {
    fn from(response: Response<T>) -> Self {
        match response {
            Response::Error(error) => Err(error),
            Response::Ok(value) => Ok(value),
        }
    }
}
