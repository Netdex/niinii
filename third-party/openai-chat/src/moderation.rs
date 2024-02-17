use backon::BackoffBuilder;
use reqwest::Method;

pub use crate::protocol::moderation::{Category, Moderation, Request, Response};

use crate::{Client, Error};

impl<B: BackoffBuilder> Client<B> {
    pub async fn moderation(&self, request: &Request) -> Result<Moderation, Error> {
        tracing::trace!(?request);
        let mut response: Response = self
            .shared
            .request_with_body(
                Method::POST,
                "https://api.openai.com/v1/moderations",
                request,
            )
            .await?
            .json()
            .await?;
        tracing::trace!(?response);
        Ok(response.results.remove(0))
    }
}

#[cfg(test)]
mod tests {
    use tracing_test::traced_test;

    use super::*;
    use crate::tests::fixture;

    #[tokio::test]
    #[traced_test]
    async fn test_moderation() {
        let client = fixture::client();
        let request = Request {
            input: "I'm going to fucking kill you".into(),
            ..Default::default()
        };
        let response = client.moderation(&request).await.unwrap();
        assert!(response.flagged);
        assert!(response.categories[&Category::Violence]);
    }
}
