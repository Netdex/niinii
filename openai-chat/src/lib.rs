//! Q: Did you really write an entire throwaway crate for this?
//!    Like, you're not even publishing this crate anywhere.
//!    You literally wrote it just for this program.
//! A: Yes, and?

use std::{sync::Arc, time::Duration};

use backon::{BackoffBuilder, Retryable};
use thiserror::Error;

pub mod assistant;
pub mod chat;
pub mod moderation;

mod protocol;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Network(#[from] reqwest::Error),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error("Protocol Error: {0}")]
    Protocol(#[from] protocol::Error),
    #[error(transparent)]
    EventStream(#[from] eventsource_stream::EventStreamError<reqwest::Error>),
}

#[derive(Clone)]
pub struct Client<B> {
    shared: Arc<Shared<B>>,
}
struct Shared<B> {
    client: reqwest::Client,
    api_base: reqwest::Url,
    token: String,
    connection_policy: ConnectionPolicy<B>,
}

#[derive(Clone)]
pub struct ConnectionPolicy<B> {
    pub backoff: B,
    pub timeout: Duration,
    pub connect_timeout: Duration,
}

impl<B: Default> Default for ConnectionPolicy<B> {
    fn default() -> Self {
        Self {
            backoff: Default::default(),
            timeout: Duration::from_secs(10),
            connect_timeout: Duration::from_secs(3),
        }
    }
}

impl<B: BackoffBuilder> Client<B> {
    pub fn new(
        token: impl Into<String>,
        api_endpoint: impl reqwest::IntoUrl,
        connection_policy: ConnectionPolicy<B>,
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
                connection_policy,
            }),
        }
    }
}

impl<B: BackoffBuilder> Shared<B> {
    async fn request_with_body(
        &self,
        method: reqwest::Method,
        path: impl AsRef<str>,
        request: &impl serde::Serialize,
    ) -> reqwest::Result<reqwest::Response> {
        self.request(method, path, Some(request)).await
    }
    async fn request_without_body(
        &self,
        method: reqwest::Method,
        path: impl AsRef<str>,
    ) -> reqwest::Result<reqwest::Response> {
        self.request(method, path, None::<&()>).await
    }

    async fn request(
        &self,
        method: reqwest::Method,
        path: impl AsRef<str>,
        request: Option<&impl serde::Serialize>,
    ) -> reqwest::Result<reqwest::Response> {
        let Shared {
            token,
            client,
            connection_policy,
            ..
        } = self;
        let uri = self.api_base.join(path.as_ref()).unwrap();
        let request_builder = || async {
            let method = method.clone();
            let uri = uri.clone();
            let r = client
                .request(method, uri)
                .bearer_auth(token)
                .header("OpenAI-Beta", "assistants=v1");
            match request {
                Some(request) => r.json(&request),
                None => r,
            }
            .send()
            .await
        };
        request_builder
            .retry(&connection_policy.backoff)
            .notify(|err: &reqwest::Error, dur: Duration| {
                tracing::error!(%err, retry=?dur, "request");
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    pub(crate) mod fixture;
}
