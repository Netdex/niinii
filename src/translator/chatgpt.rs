use std::collections::VecDeque;

use openai_chat::{Client, Completion, Error, Message, Request, Role};

use crate::settings::Settings;

pub struct Conversation {
    pub client: Client,
    pub max_tokens: u32,
    pub max_context_tokens: u32,

    pub context: VecDeque<Message>,
}
impl Conversation {
    fn new(client: Client, max_tokens: u32, max_context_tokens: u32) -> Self {
        assert!(max_context_tokens >= max_tokens);
        Self {
            client,
            context: VecDeque::new(),
            max_tokens,
            max_context_tokens,
        }
    }
    pub fn prompt(&mut self, content: impl Into<String>) -> Result<Completion, Error> {
        let Self {
            client,
            context,
            max_tokens,
            max_context_tokens,
        } = self;
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
        }
        let mut messages = vec![Message {
            role: Role::System,
            content: "Translate the following visual novel script into English".to_string(),
        }];
        messages.extend(context.iter().cloned());
        let request = Request {
            messages: messages,
            max_tokens: Some(*max_tokens),
            ..Default::default()
        };

        let completion = client.completions(&request)?;
        let response = &completion.choices.first().unwrap().message;
        context.push_back(response.clone());
        Ok(completion)
    }
}

#[derive(Debug)]
pub struct ChatGptTranslation {
    pub content_text: String,
    pub openai_usage: openai_chat::Usage,
    pub max_context_tokens: u32,
}

pub struct ChatGptTranslator {
    pub(crate) _client: Client,
    pub(crate) conversation: Conversation,
}
impl ChatGptTranslator {
    pub fn new(settings: &Settings) -> Self {
        let client = openai_chat::Client::new(&settings.openai_api_key);
        let conversation = Conversation::new(client.clone(), 64, 128);
        Self {
            _client: client,
            conversation,
        }
    }
    pub fn translate(&mut self, text: &str) -> Result<ChatGptTranslation, openai_chat::Error> {
        let Self { conversation, .. } = self;
        let completion = conversation.prompt(text)?;
        let content = &completion.choices.first().unwrap().message.content;
        Ok(ChatGptTranslation {
            content_text: content.to_string(),
            openai_usage: completion.usage,
            max_context_tokens: conversation.max_context_tokens,
        })
    }
}
