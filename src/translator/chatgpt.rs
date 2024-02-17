use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use enclose::enclose;
use openai_chat::{
    chat::{self, Message, Model},
    moderation, Client, ConnectionPolicy,
};
use tokio_stream::StreamExt;
use tokio_util::sync::{CancellationToken, DropGuard};
use tracing::Instrument;

use crate::settings::Settings;

use super::{chat_buffer::ChatBuffer, Error, Translate, Translation};

#[derive(Clone)]
pub struct ChatGptTranslator {
    client: Client<backon::ConstantBuilder>,
    pub chat: Arc<Mutex<ChatBuffer>>,
}
impl ChatGptTranslator {
    pub fn new(settings: &Settings) -> Self {
        Self {
            client: Client::new(
                &settings.openai_api_key,
                ConnectionPolicy {
                    backoff: Default::default(),
                    timeout: Duration::from_millis(settings.chatgpt.timeout),
                    connect_timeout: Duration::from_millis(settings.chatgpt.connection_timeout),
                },
            ),
            chat: Arc::new(Mutex::new(ChatBuffer::new())),
        }
    }
}
#[async_trait]
impl Translate for ChatGptTranslator {
    async fn translate(
        &mut self,
        settings: &Settings,
        text: impl 'async_trait + Into<String> + Send,
    ) -> Result<Translation, Error> {
        let text = text.into();
        let chatgpt = &settings.chatgpt;

        if chatgpt.moderation {
            let mod_request = moderation::Request {
                input: text.clone(),
                ..Default::default()
            };
            let moderation = self.client.moderation(&mod_request).await?;
            if moderation.flagged {
                return Ok(Translation::ChatGpt(ChatGptTranslation::Filtered {
                    moderation,
                }));
            }
        }

        let chat_request = {
            let mut chat = self.chat.lock().unwrap();
            // TODO: experiment with summarizing context
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
                let mut chat = self.chat.lock().unwrap();
                chat.cancel_exchange();
                return Err(err.into());
            }
        };

        let token = CancellationToken::new();
        let chat = &self.chat;
        tokio::spawn(enclose! { (chat, token) async move {
            loop {
                tokio::select! {
                    msg = stream.next() => match msg {
                        Some(Ok(completion)) => {
                            let mut chat = chat.lock().unwrap();
                            let message = &completion.choices.first().unwrap().delta;
                            chat.append_partial_response(message)
                        },
                        // TODO: need to pipe this error to the event loop somehow
                        Some(Err(err)) => tracing::error!(%err, "stream"),
                        _ => break
                    },
                    _ = token.cancelled() => break
                }
            }
            let mut chat = chat.lock().unwrap();
            chat.end_exchange();
        }.instrument(tracing::Span::current())});

        Ok(ChatGptTranslation::Translated {
            model: chatgpt.model,
            chat: chat.clone(),
            _guard: token.drop_guard(),
        }
        .into())
    }
}

pub enum ChatGptTranslation {
    Translated {
        model: Model,
        chat: Arc<Mutex<ChatBuffer>>,
        _guard: DropGuard,
    },
    Filtered {
        moderation: moderation::Moderation,
    },
}
