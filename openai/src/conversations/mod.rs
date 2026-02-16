//! https://platform.openai.com/docs/api-reference/conversations/create

use reqwest::Method;
use serde_json::json;
use tracing::Level;

pub use crate::protocol::conversations::Conversation;
use crate::{Client, Error};

impl Client {
    #[tracing::instrument(level = Level::DEBUG, skip_all, err)]
    pub async fn create_conversation(&self) -> Result<Conversation, Error> {
        let response = self
            .shared
            .request(Method::POST, "v1/conversations")
            .body(&json!({}))
            .send()
            .await?
            .json()
            .await?;
        Ok(response)
    }
}
