//! Simple wrapper for a conversation using the completions API

use std::collections::VecDeque;

use crate::chat::{Message, PartialMessage, Role, Usage};

#[derive(Debug)]
pub struct ChatBuffer {
    context: VecDeque<Message>,
}

impl Default for ChatBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl ChatBuffer {
    pub fn new() -> Self {
        ChatBuffer {
            context: VecDeque::new(),
        }
    }

    pub fn start_exchange(&mut self, system: Message, request: Message) -> Exchange {
        Exchange {
            system,
            context: self.context.clone(),
            request,
            response: None,
            usage: None,
        }
    }

    pub fn commit(&mut self, exchange: &mut Exchange) {
        if exchange.usage.is_none() {
            self.context = exchange.context.clone();
            self.context.push_back(exchange.request.clone());
            self.context.extend(exchange.response.iter().cloned());
            exchange.usage = Some(exchange.estimate_usage());
        }
    }

    pub fn enforce_context_limit(&mut self, limit: u32) {
        let mut idx = 0;
        loop {
            if self.context_tokens() <= limit || idx >= self.context.len() {
                break;
            }
            if self.context[idx].name.is_some() {
                idx += 1;
            } else {
                self.context.remove(idx);
            }
            while let Some(message) = self.context.get(idx) {
                if message.role == Role::User {
                    break;
                }
                self.context.remove(idx);
            }
        }
    }

    pub fn clear(&mut self) {
        self.context.clear();
    }

    fn context_tokens(&self) -> u32 {
        self.context
            .iter()
            .map(|m| m.estimate_tokens())
            .sum::<u32>()
    }

    pub fn context(&self) -> &VecDeque<Message> {
        &self.context
    }

    pub fn context_mut(&mut self) -> &mut VecDeque<Message> {
        &mut self.context
    }
}

pub struct Exchange {
    system: Message,
    context: VecDeque<Message>,
    request: Message,
    response: Option<Message>,
    usage: Option<Usage>,
}
impl Exchange {
    pub fn append_partial(&mut self, partial: &PartialMessage) {
        if let Some(last) = &mut self.response {
            if let Some(content) = &mut last.content {
                content.push_str(&partial.content)
            }
        } else {
            let message = Message {
                role: Role::Assistant,
                content: Some(partial.content.clone()),
                ..Default::default()
            };
            self.response = Some(message);
        }
    }

    pub fn set_complete(&mut self, message: Message) {
        self.response = Some(message);
    }

    pub fn prompt(&self) -> Vec<Message> {
        let mut messages = vec![];
        messages.push(self.system.clone());
        messages.extend(self.context.iter().cloned());
        messages.push(self.request.clone());
        messages
    }

    fn estimate_usage(&self) -> Usage {
        // every reply is primed with <im_start>assistant
        let prompt_tokens = self
            .context
            .iter()
            .map(|m| m.estimate_tokens())
            .sum::<u32>()
            + self.system.estimate_tokens()
            + self.request.estimate_tokens()
            + 2;
        let completion_tokens = self
            .response
            .iter()
            .map(|m| m.estimate_tokens())
            .sum::<u32>();
        Usage {
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens + completion_tokens,
        }
    }

    pub fn response(&self) -> Option<&Message> {
        self.response.as_ref()
    }

    pub fn usage(&self) -> Option<&Usage> {
        self.usage.as_ref()
    }
}
