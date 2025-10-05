//! https://platform.openai.com/docs/guides/conversation-state?api-mode=responses

use reqwest::Method;
use tokio_stream::{Stream, StreamExt};
use tracing::Level;

pub use crate::protocol::responses::Request;
use crate::{protocol::responses, Client, Error};

impl Client {
    #[tracing::instrument(level = Level::DEBUG, skip_all, err)]
    pub async fn responses(&self, request: Request) -> Result<(), Error> {
        tracing::debug!(?request);
        // let response: chat::ChatResponse = self
        //     .shared
        //     .request(Method::POST, "/v1/chat/completions")
        //     .body(&request)
        //     .send()
        //     .await?
        //     .json()
        //     .await?;
        // tracing::debug!(?response);
        Ok(())
    }
}
