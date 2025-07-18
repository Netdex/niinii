use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use enclose::enclose;
use openai_chat::{
    chat::{self, ChatBuffer, Message, Model},
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
        let chat_request = {
            let mut chat = self.chat.lock().await;
            chat.begin_exchange(
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
            );
            chat.enforce_context_limit(chatgpt.max_context_tokens);

            chat::Request {
                model: chatgpt.model,
                messages: chat.prompt(),
                temperature: chatgpt.temperature,
                top_p: chatgpt.top_p,
                max_tokens: chatgpt.max_tokens,
                presence_penalty: chatgpt.presence_penalty,
                ..Default::default()
            }
        };

        let stream = self.client.stream(chat_request).await;
        let mut stream = match stream {
            Ok(stream) => stream,
            Err(err) => {
                let mut chat = self.chat.lock().await;
                chat.cancel_exchange();
                return Err(err.into());
            }
        };

        let token = CancellationToken::new();
        let chat = &self.chat;
        tokio::spawn(enclose! { (chat, token) async move {
            // Hold permit: We are not allowed to begin another translation
            // request until this one is complete.
            let _permit = permit;
            loop {
                tokio::select! {
                    msg = stream.next() => match msg {
                        Some(Ok(completion)) => {
                            let mut chat = chat.lock().await;
                            let message = &completion.choices.first().unwrap().delta;
                            chat.append_partial_response(message)
                        },
                        Some(Err(err)) => {
                            tracing::error!(%err, "stream");
                            let mut chat = chat.lock().await;
                            chat.cancel_exchange();
                            break
                        },
                        None => {
                            let mut chat = chat.lock().await;
                            chat.end_exchange();
                            break
                        }
                    },
                    _ = token.cancelled() => {
                        let mut chat = chat.lock().await;
                        chat.cancel_exchange();
                        break
                    }
                }
            }
        }.instrument(tracing::Span::current())});

        Ok(Box::new(ChatTranslation::Translated {
            model: chatgpt.model,
            chat: chat.clone(),
            _guard: token.drop_guard(),
        }))
    }

    fn view<'a>(&'a self, settings: &'a mut Settings) -> Box<dyn View + 'a> {
        Box::new(ViewChatTranslator(self, settings))
    }
}

pub enum ChatTranslation {
    Translated {
        model: Model,
        chat: Arc<Mutex<ChatBuffer>>,
        _guard: DropGuard,
    },
}
impl Translation for ChatTranslation {
    fn view(&self) -> Box<dyn View + '_> {
        Box::new(ViewChatTranslation(self))
    }
    fn view_usage(&self) -> Box<dyn View + '_> {
        Box::new(ViewChatTranslationUsage(self))
    }
}
