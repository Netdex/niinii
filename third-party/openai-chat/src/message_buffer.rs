use std::collections::VecDeque;

use crate::chat::{Message, PartialMessage};

#[derive(Debug)]
pub struct MessageBuffer {
    messages: VecDeque<Message>,
}
impl MessageBuffer {
    pub fn new() -> Self {
        MessageBuffer {
            messages: VecDeque::new(),
        }
    }
    pub fn apply_delta(&mut self, delta: &PartialMessage) {
        match delta {
            PartialMessage::Role(role) => {
                let message = Message {
                    role: role.clone(),
                    content: "".into(),
                    name: None,
                };
                self.messages.push_back(message)
            }
            PartialMessage::Content(content) => {
                if let Some(last) = self.back_mut() {
                    last.content.push_str(&content)
                }
            }
        }
    }
}

impl std::ops::Deref for MessageBuffer {
    type Target = VecDeque<Message>;
    fn deref(&self) -> &VecDeque<Message> {
        &self.messages
    }
}

impl std::ops::DerefMut for MessageBuffer {
    fn deref_mut(&mut self) -> &mut VecDeque<Message> {
        &mut self.messages
    }
}