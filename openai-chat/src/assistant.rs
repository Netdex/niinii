use std::sync::Arc;

use backon::BackoffBuilder;
use reqwest::Method;

pub use crate::protocol::assistant::{AssistantId, CreateAssistantRequest};

use crate::{
    protocol::{
        assistant::{
            CreateAssistantResponse, CreateMessageResponse, DeleteAssistantResponse, ThreadId,
        },
        chat::Message,
    },
    Client, Error, Shared,
};

pub struct Assistant<B: BackoffBuilder> {
    shared: Arc<Shared<B>>,
    id: AssistantId,
    deleted: bool,
}
impl<B: BackoffBuilder> Assistant<B> {
    fn new(shared: Arc<Shared<B>>, id: AssistantId) -> Self {
        Assistant {
            shared,
            id,
            deleted: false,
        }
    }
    pub async fn delete(&mut self) -> Result<(), Error> {
        let response: DeleteAssistantResponse = self
            .shared
            .request_without_body(
                Method::DELETE,
                format!("https://api.openai.com/v1/assistants/{}", self.id),
            )
            .await?
            .json()
            .await?;
        Result::from(response)?;
        self.deleted = true;
        Ok(())
    }
}
impl<B: BackoffBuilder> Drop for Assistant<B> {
    fn drop(&mut self) {
        if !self.deleted {
            let handle = tokio::runtime::Handle::current();
            handle.block_on(self.delete()).unwrap();
        }
    }
}

pub struct Thread<B: BackoffBuilder> {
    shared: Arc<Shared<B>>,
    id: ThreadId,
    deleted: bool,
}
impl<B: BackoffBuilder> Thread<B> {
    fn new(shared: Arc<Shared<B>>, id: ThreadId) -> Self {
        Thread {
            shared,
            id,
            deleted: false,
        }
    }
    pub async fn add_message(&self, message: Message) -> Result<Message, Error> {
        let response: CreateMessageResponse = self
            .shared
            .request_with_body(
                Method::POST,
                format!("https://api.openai.com/v1/threads/{}/messages", self.id),
                &message,
            )
            .await?
            .json()
            .await?;
        Ok(Result::from(response)?)
    }
    pub async fn delete(&mut self) -> Result<(), Error> {
        let response: DeleteAssistantResponse = self
            .shared
            .request_without_body(
                Method::DELETE,
                format!("https://api.openai.com/v1/threads/{}", self.id),
            )
            .await?
            .json()
            .await?;
        Result::from(response)?;
        self.deleted = true;
        Ok(())
    }
}
impl<B: BackoffBuilder> Drop for Thread<B> {
    fn drop(&mut self) {
        if !self.deleted {
            let handle = tokio::runtime::Handle::current();
            handle.block_on(self.delete()).unwrap();
        }
    }
}

impl<B: BackoffBuilder> Client<B> {
    pub async fn create_assistant(
        &self,
        request: CreateAssistantRequest,
    ) -> Result<Assistant<B>, Error> {
        tracing::trace!(?request);
        let response: CreateAssistantResponse = self
            .shared
            .request_with_body(
                Method::POST,
                self.shared.api_endpoint.join("/v1/assistants").unwrap(),
                &request,
            )
            .await?
            .json()
            .await?;
        tracing::trace!(?response);
        let assistant = Result::from(response)?;
        Ok(Assistant::new(self.shared.clone(), assistant.id.clone()))
    }
}

#[cfg(test)]
mod tests {
    use tracing_test::traced_test;

    use super::*;
    use crate::tests::fixture;

    #[tokio::test]
    #[traced_test]
    async fn test_assistant() {
        let client = fixture::client();
        let assistant = client.create_assistant(CreateAssistantRequest {
            name: Some("My Assistant".into()),
            ..Default::default()
        });
    }
}
