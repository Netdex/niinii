use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use enclose::enclose;
use openai_chat::{
    chat::{self, Message},
    moderation, Client, MessageBuffer,
};
use tokio_stream::StreamExt;
use tokio_util::sync::{CancellationToken, DropGuard};
use tracing::Instrument;

use crate::settings::Settings;

use super::{Error, Translate, Translation};

#[derive(Clone)]
pub struct ChatGptTranslator {
    client: Client,
    pub context: Arc<Mutex<MessageBuffer>>,
}
impl ChatGptTranslator {
    pub fn new(settings: &Settings) -> Self {
        Self {
            client: openai_chat::Client::new(&settings.openai_api_key),
            context: Arc::new(Mutex::new(MessageBuffer::new())),
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
            let mut context = self.context.lock().unwrap();
            // TODO: experiment with summarizing context
            loop {
                let estimated_tokens: u32 = context.iter().map(|m| m.estimate_tokens()).sum();
                if estimated_tokens <= chatgpt.max_context_tokens {
                    break;
                }
                context.pop_front();
                context.pop_front();
            }
            context.push_back(Message {
                role: chat::Role::User,
                content: Some(text.clone()),
                ..Default::default()
            });
            let mut messages = vec![chat::Message {
                role: chat::Role::System,
                content: Some(chatgpt.system_prompt.clone()),
                ..Default::default()
            }];
            messages.extend(context.iter().cloned());
            chat::Request {
                model: chat::Model::Gpt35Turbo0613,
                messages,
                temperature: chatgpt.temperature,
                top_p: chatgpt.top_p,
                max_tokens: chatgpt.max_tokens,
                presence_penalty: chatgpt.presence_penalty,
                ..Default::default()
            }
        };

        let mut stream = self
            .client
            .stream(chat_request)
            .await
            .map_err(openai_chat::Error::Network)?;

        let token = CancellationToken::new();
        let context = &self.context;
        tokio::spawn(enclose! { (context, token) async move {
            loop {
                tokio::select! {
                    msg = stream.next() => match msg {
                        Some(Ok(completion)) => {
                            let mut context = context.lock().unwrap();
                            let message = &completion.choices.first().unwrap().delta;
                            context.apply_delta(message)
                        },
                        _ => break
                    },
                    _ = token.cancelled() => break
                }
            }
        }.instrument(tracing::Span::current())});

        Ok(ChatGptTranslation::Translated {
            context: context.clone(),
            max_context_tokens: chatgpt.max_context_tokens,
            _guard: token.drop_guard(),
        }
        .into())
    }
}

pub enum ChatGptTranslation {
    Translated {
        context: Arc<Mutex<MessageBuffer>>,
        max_context_tokens: u32,
        _guard: DropGuard,
    },
    Filtered {
        moderation: moderation::Result,
    },
}
