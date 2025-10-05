use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use enclose::enclose;
use openai::{
    chat::{self, ChatBuffer, Exchange, Message},
    ConnectionPolicy, ModelId,
};
use tokio::sync::Mutex;
use tokio_stream::StreamExt;
use tokio_util::sync::{CancellationToken, DropGuard};
use tracing::Instrument;

use crate::{
    settings::Settings,
    view::{
        translator::{ViewChatTranslation, ViewChatTranslationUsage, ViewChatTranslator},
        View,
    },
};

use super::{Error, Translation, Translator};

pub struct ChatTranslator {
    client: openai::Client,
    pub models: Vec<openai::ModelId>,
    pub buffer: Arc<Mutex<ChatBuffer>>,
}
impl ChatTranslator {
    pub async fn new(settings: &Settings) -> Self {
        let client = openai::Client::new(
            &settings.openai_api_key,
            &settings.chat.api_endpoint,
            ConnectionPolicy {
                timeout: Duration::from_millis(settings.chat.timeout),
                connect_timeout: Duration::from_millis(settings.chat.connection_timeout),
            },
        );
        let models = client
            .models()
            .await
            .inspect_err(|err| {
                tracing::error!(
                    ?err,
                    "failed to query OpenAI models, no models will be available"
                )
            })
            .unwrap_or_default();
        Self {
            client,
            models,
            buffer: Arc::new(Mutex::new(ChatBuffer::new())),
        }
    }
}

#[async_trait]
impl Translator for ChatTranslator {
    async fn translate(
        &self,
        settings: &Settings,
        text: String,
    ) -> Result<Box<dyn Translation>, Error> {
        let chat = &settings.chat;

        let exchange = {
            let mut buffer = self.buffer.lock().await;
            buffer.start_exchange(
                Message {
                    role: chat::Role::System,
                    content: Some(chat.system_prompt.clone()),
                    ..Default::default()
                },
                Message {
                    role: chat::Role::User,
                    content: Some(text.clone()),
                    ..Default::default()
                },
            )
        };

        let chat_request = chat::Request::builder()
            .model(chat.model.clone())
            .messages(exchange.prompt())
            .maybe_temperature(chat.temperature)
            .maybe_top_p(chat.top_p)
            .maybe_max_completion_tokens(chat.max_tokens)
            .maybe_presence_penalty(chat.presence_penalty)
            .maybe_service_tier(chat.service_tier)
            .maybe_reasoning_effort(chat.reasoning_effort)
            .build();

        let exchange = Arc::new(Mutex::new(exchange));
        let token = CancellationToken::new();
        if chat.stream {
            let mut stream = self.client.stream(chat_request).await?;
            tokio::spawn(
                enclose! { (self.buffer => buffer, token, exchange, chat.max_context_tokens => max_context_tokens) async move {
                    loop {
                        tokio::select! {
                            msg = stream.next() => match msg {
                                Some(Ok(cmpl)) => {
                                    let mut exchange = exchange.lock().await;
                                    exchange.partial(cmpl)
                                },
                                Some(Err(err)) => {
                                    tracing::error!(%err, "stream");
                                    break
                                },
                                None => {
                                    let mut buffer = buffer.lock().await;
                                    let exchange = exchange.lock().await;
                                    buffer.commit(&exchange);
                                    buffer.enforce_context_limit(&max_context_tokens);
                                    break
                                }
                            },
                            _ = token.cancelled() => {
                                break
                            }
                        }
                    }
                    let mut exchange = exchange.lock().await;
                    exchange.set_completed();
                }.instrument(tracing::Span::current())},
            );
        } else {
            let cmpl = self.client.chat(chat_request).await?;
            let mut exchange = exchange.lock().await;
            exchange.complete(cmpl);
            self.buffer.lock().await.commit(&exchange);
        }

        Ok(Box::new(ChatTranslation {
            model: chat.model.clone(),
            exchange,
            _guard: token.drop_guard(),
        }))
    }

    fn view<'a>(&'a self, settings: &'a mut Settings) -> Box<dyn View + 'a> {
        Box::new(ViewChatTranslator(self, settings))
    }
}

pub struct ChatTranslation {
    pub model: ModelId,
    pub exchange: Arc<Mutex<Exchange>>,
    _guard: DropGuard,
}
impl Translation for ChatTranslation {
    fn view(&self) -> Box<dyn View + '_> {
        Box::new(ViewChatTranslation(self))
    }
    fn view_usage(&self) -> Box<dyn View + '_> {
        Box::new(ViewChatTranslationUsage(self))
    }
}
