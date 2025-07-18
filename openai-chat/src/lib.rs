//! https://platform.openai.com/docs/api-reference/introduction
//!
//! Q: Did you really write an entire throwaway crate for this?
//!    Like, you're not even publishing this crate anywhere.
//!    You literally wrote it just for this program.
//! A: Yes, and?

use std::{sync::Arc, time::Duration};

use backon::Retryable;
use serde::Serialize;
use thiserror::Error;

pub mod chat;
pub mod realtime;

mod protocol;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Request(#[from] reqwest::Error),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error("protocol error: {0}")]
    Protocol(#[from] protocol::Error),
    #[error(transparent)]
    EventStream(#[from] eventsource_stream::EventStreamError<reqwest::Error>),
    #[error(transparent)]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("unexpected server response: {0:?}")]
    UnexpectedResponse(Box<protocol::realtime::ServerEvent>),
}

#[derive(Clone)]
pub struct Client {
    shared: Arc<Shared>,
}
struct Shared {
    client: reqwest::Client,
    api_base: reqwest::Url,
    token: String,
}

#[derive(Clone)]
pub struct ConnectionPolicy {
    pub timeout: Duration,
    pub connect_timeout: Duration,
}

impl Default for ConnectionPolicy {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(10),
            connect_timeout: Duration::from_secs(3),
        }
    }
}

pub struct RequestBuilder {
    reqwest_builder: reqwest::RequestBuilder,
}
impl RequestBuilder {
    pub fn new(reqwest_builder: reqwest::RequestBuilder) -> Self {
        Self { reqwest_builder }
    }
    pub fn body<T: Serialize + ?Sized>(mut self, j: &T) -> Self {
        self.reqwest_builder = self.reqwest_builder.json(j);
        self
    }
    pub fn beta<T: Into<String>>(mut self, beta: T) -> Self {
        self.reqwest_builder = self.reqwest_builder.header("OpenAI-Beta", beta.into());
        self
    }
    pub async fn send(self) -> reqwest::Result<reqwest::Response> {
        let request_fn_mut = || async { self.reqwest_builder.try_clone().unwrap().send().await };
        request_fn_mut
            .retry(backon::ConstantBuilder::default())
            .notify(|err: &reqwest::Error, dur: Duration| {
                tracing::error!(%err, retry=?dur, "request");
            })
            .await
    }
}

impl Client {
    pub fn new(
        token: impl Into<String>,
        api_endpoint: impl reqwest::IntoUrl,
        connection_policy: ConnectionPolicy,
    ) -> Self {
        Self {
            shared: Arc::new(Shared {
                client: reqwest::Client::builder()
                    .timeout(connection_policy.timeout)
                    .connect_timeout(connection_policy.connect_timeout)
                    .build()
                    .unwrap(),
                api_base: api_endpoint.into_url().unwrap(),
                token: token.into(),
            }),
        }
    }
}

impl Shared {
    fn request(&self, method: reqwest::Method, path: impl AsRef<str>) -> RequestBuilder {
        let Shared { token, client, .. } = self;
        let uri = self.api_base.join(path.as_ref()).unwrap();
        let r = client.request(method, uri).bearer_auth(token);
        RequestBuilder::new(r)
    }
}

#[cfg(test)]
mod tests {
    pub(crate) mod fixture;
}
