use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, RwLock},
};

use openai_chat::{Client, Completion, Message, Request, Role};

use crate::settings::Settings;

use super::{Error, Translate, Translation};

#[derive(Clone)]
pub struct ChatGptTranslator {
    pub(crate) shared: Arc<Shared>,
}
pub(crate) struct Shared {
    client: Client,
    pub max_tokens: u32,
    pub max_context_tokens: u32,
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
                max_tokens: 128,
                max_context_tokens: 128,
                state: Mutex::new(State {
                    context: VecDeque::new(),
                }),
            }),
        }
    }
    pub fn submit(
        &self,
        system_prompt: impl Into<String>,
        content: impl Into<String>,
    ) -> Result<Completion, Error> {
        let Shared {
            max_context_tokens,
            max_tokens,
            ..
        } = &*self.shared;
        let request = {
            let State { context } = &mut *self.shared.state.lock().unwrap();
            context.push_back(Message {
                role: Role::User,
                content: content.into(),
            });
            // TODO: experiment with summarizing context
            loop {
                let estimated_tokens: u32 = context.iter().map(|m| m.estimate_tokens()).sum();
                if estimated_tokens <= *max_context_tokens {
                    break;
                }
                context.pop_front();
                context.pop_front();
            }
            let mut messages = vec![Message {
                role: Role::System,
                content: system_prompt.into(),
            }];
            messages.extend(context.iter().cloned());
            Request {
                messages,
                max_tokens: Some(*max_tokens),
                ..Default::default()
            }
        };

        // do not hold lock across blocking I/O
        let completion = self.shared.client.completions(&request)?;
        let response = &completion.choices.first().unwrap().message;
        {
            let State { context, .. } = &mut *self.shared.state.lock().unwrap();
            context.push_back(response.clone());
        }
        Ok(completion)
    }
}
impl Translate for ChatGptTranslator {
    fn translate(
        &mut self,
        settings: &Settings,
        text: impl Into<String>,
    ) -> Result<Translation, Error> {
        let completion = self.submit(&settings.chatgpt_system_prompt, text)?;
        let content = &completion.choices.first().unwrap().message.content;
        Ok(Translation::ChatGpt(ChatGptTranslation {
            content_text: content.to_string(),
            openai_usage: completion.usage,
            max_context_tokens: self.shared.max_context_tokens,
        }))
    }
}

#[derive(Debug)]
pub struct ChatGptTranslation {
    pub content_text: String,
    pub openai_usage: openai_chat::Usage,
    pub max_context_tokens: u32,
}
