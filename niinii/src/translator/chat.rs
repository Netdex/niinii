use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use enclose::enclose;
use openai_chat::{
    chat::{self, ChatBuffer, Exchange, Message, Model},
    ConnectionPolicy,
};
use tokio::sync::{Mutex, Semaphore};
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
    client: openai_chat::Client,
    pub chat: Arc<Mutex<ChatBuffer>>,
    semaphore: Arc<Semaphore>,
}
impl ChatTranslator {
    pub fn new(settings: &Settings) -> Self {
        Self {
            client: openai_chat::Client::new(
                &settings.openai_api_key,
                &settings.chat.api_endpoint,
                ConnectionPolicy {
                    timeout: Duration::from_millis(settings.chat.timeout),
                    connect_timeout: Duration::from_millis(settings.chat.connection_timeout),
                },
            ),
            chat: Arc::new(Mutex::new(ChatBuffer::new())),
            semaphore: Arc::new(Semaphore::const_new(1)),
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
        let chatgpt = &settings.chat;

        let permit = self.semaphore.clone().acquire_owned().await.unwrap();
        let mut exchange = {
            let mut chat = self.chat.lock().await;
            chat.start_exchange(
                Message {
                    role: chat::Role::System,
                    content: Some(chatgpt.system_prompt.clone()),
                    ..Default::default()
                },
                Message {
                    role: chat::Role::User,
                    content: Some(text.clone()),
                    ..Default::default()
                },
            )
        };

        let chat_request = chat::Request {
            model: chatgpt.model,
            messages: exchange.prompt(),
            temperature: chatgpt.temperature,
            top_p: chatgpt.top_p,
            max_tokens: chatgpt.max_tokens,
            presence_penalty: chatgpt.presence_penalty,
            ..Default::default()
        };

        let exchange = Arc::new(Mutex::new(exchange));
        let mut stream = self.client.stream(chat_request).await?;
        let token = CancellationToken::new();
        let chat = &self.chat;
        tokio::spawn(
            enclose! { (chat, token, exchange, chatgpt.max_context_tokens => max_context_tokens) async move {
                // Hold permit: We are not allowed to begin another translation
                // request until this one is complete.
                let _permit = permit;
                loop {
                    tokio::select! {
                        msg = stream.next() => match msg {
                            Some(Ok(completion)) => {
                                let mut exchange = exchange.lock().await;
                                let message = &completion.choices.first().unwrap().delta;
                                exchange.append(message)
                            },
                            Some(Err(err)) => {
                                tracing::error!(%err, "stream");
                                break
                            },
                            None => {
                                let mut chat = chat.lock().await;
                                let mut exchange = exchange.lock().await;
                                chat.commit(&mut exchange);
                                chat.enforce_context_limit(max_context_tokens);
                                break
                            }
                        },
                        _ = token.cancelled() => {
                            break
                        }
                    }
                }
            }.instrument(tracing::Span::current())},
        );

        Ok(Box::new(ChatTranslation {
            model: chatgpt.model,
            exchange,
            _guard: token.drop_guard(),
        }))
    }

    fn view<'a>(&'a self, settings: &'a mut Settings) -> Box<dyn View + 'a> {
        Box::new(ViewChatTranslator(self, settings))
    }
}

pub struct ChatTranslation {
    pub model: Model,
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
