use std::{collections::VecDeque, sync::Arc};
use tokio::sync::Mutex;

use async_trait::async_trait;
use openai_chat::{
    chat::{self, Message, Usage},
    moderation, Client,
};

use crate::settings::Settings;

use super::{Error, Translate, Translation};

#[derive(Clone)]
pub struct ChatGptTranslator {
    pub(crate) shared: Arc<Shared>,
}
pub(crate) struct Shared {
    client: Client,
    pub(crate) state: Mutex<State>,
}
pub(crate) struct State {
    pub context: VecDeque<Message>,
}
impl ChatGptTranslator {
    pub fn new(settings: &Settings) -> Self {
        let client = openai_chat::Client::new(&settings.openai_api_key);
        Self {
            shared: Arc::new(Shared {
                client,
                state: Mutex::new(State {
                    context: VecDeque::new(),
                }),
            }),
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
        let mod_request = moderation::Request {
            input: text.clone(),
            ..Default::default()
        };
        let moderation = self.shared.client.moderation(&mod_request).await?;
        let translation = if moderation.flagged {
            ChatGptTranslation::Filtered(moderation)
        } else {
            let chat_request = {
                let State { context } = &mut *self.shared.state.lock().await;
                // TODO: experiment with summarizing context
                loop {
                    let estimated_tokens: u32 = context.iter().map(|m| m.estimate_tokens()).sum();
                    if estimated_tokens <= settings.chatgpt_max_context_tokens {
                        break;
                    }
                    context.pop_front();
                    context.pop_front();
                }
                context.push_back(Message {
                    role: chat::Role::User,
                    content: text.clone(),
                });
                let mut messages = vec![chat::Message {
                    role: chat::Role::System,
                    content: settings.chatgpt_system_prompt.clone(),
                }];
                messages.extend(context.iter().cloned());
                chat::Request {
                    messages,
                    max_tokens: Some(settings.chatgpt_max_tokens),
                    ..Default::default()
                }
            };

            // do not hold lock across I/O
            let completion = self.shared.client.chat(&chat_request).await?;
            let message = &completion.choices.first().unwrap().message;
            {
                let State { context, .. } = &mut *self.shared.state.lock().await;
                context.push_back(message.clone());
            }
            let content = &completion.choices.first().unwrap().message.content;
            ChatGptTranslation::Translated {
                content_text: content.to_string(),
                openai_usage: completion.usage,
                max_context_tokens: settings.chatgpt_max_context_tokens,
            }
        };
        Ok(Translation::ChatGpt(translation))
    }
}

#[derive(Debug)]
pub enum ChatGptTranslation {
    Translated {
        content_text: String,
        openai_usage: Usage,
        max_context_tokens: u32,
    },
    Filtered(moderation::Result),
}
