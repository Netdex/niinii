use std::sync::{Arc, Mutex};

use crate::settings::Settings;

#[derive(Debug)]
pub struct ChatGptTranslation {
    pub content_text: String,
    pub openai_usage: openai_chat::Usage,
}

pub struct ChatGpt {
    client: openai_chat::Client,
    conversation: openai_chat::Conversation,
}
impl ChatGpt {
    pub fn new(settings: &Settings) -> Self {
        let client = openai_chat::Client::new(&settings.openai_api_key);
        let conversation = client.conversation(openai_chat::Request {
            messages: vec![openai_chat::Message {
                role: openai_chat::Role::System,
                content: "Translate the following conversation from Japanese to English"
                    .to_string(),
            }],
            ..Default::default()
        });
        Self {
            client,
            conversation,
        }
    }
    pub fn translate(&mut self, text: &str) -> Result<ChatGptTranslation, openai_chat::Error> {
        let Self {
            client,
            conversation,
        } = self;
        let completion = conversation.prompt(text)?;
        let content = &completion.choices.first().unwrap().message.content;
        Ok(ChatGptTranslation {
            content_text: content.to_string(),
            openai_usage: completion.usage,
        })
    }
}
