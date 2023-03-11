use std::{collections::VecDeque, sync::Arc};
use tokio::sync::Mutex;

use async_trait::async_trait;
use openai_chat::{Client, Message, Request, Role};

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
        let request = {
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
                role: Role::User,
                content: text.into(),
            });
            let mut messages = vec![Message {
                role: Role::System,
                content: settings.chatgpt_system_prompt.clone(),
            }];
            messages.extend(context.iter().cloned());
            Request {
                messages,
                max_tokens: Some(settings.chatgpt_max_tokens),
                ..Default::default()
            }
        };

        // do not hold lock across I/O
        let completion = self.shared.client.completions(&request).await?;
        let response = &completion.choices.first().unwrap().message;
        {
            let State { context, .. } = &mut *self.shared.state.lock().await;
            context.push_back(response.clone());
        }
        let content = &completion.choices.first().unwrap().message.content;
        Ok(Translation::ChatGpt(ChatGptTranslation {
            content_text: content.to_string(),
            openai_usage: completion.usage,
            max_context_tokens: settings.chatgpt_max_context_tokens,
        }))
    }
}

#[derive(Debug)]
pub struct ChatGptTranslation {
    pub content_text: String,
    pub openai_usage: openai_chat::Usage,
    pub max_context_tokens: u32,
}
