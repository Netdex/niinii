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
        if let Some(role) = &delta.role {
            let message = Message {
                role: role.clone(),
                content: Some(delta.content.clone()),
                ..Default::default()
            };
            self.messages.push_back(message)
        } else {
            if let Some(last) = self.back_mut() {
                if let Some(content) = &mut last.content {
                    content.push_str(&delta.content)
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
