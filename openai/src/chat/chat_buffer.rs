//! Simple wrapper for a conversation using the completions API

use std::collections::VecDeque;

use crate::{
    chat::{Message, Role, Usage},
    protocol::chat::{Completion, PartialCompletion},
};

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
            completed: false,
        }
    }

    pub fn commit(&mut self, exchange: &Exchange) {
        self.context = exchange.context.clone();
        self.context.push_back(exchange.request.clone());
        self.context.extend(exchange.response.iter().cloned());
    }

    pub fn enforce_context_limit(&mut self, limit_range: &[u32; 2]) {
        if self.context_tokens() <= limit_range[1] {
            return;
        }
        let mut idx = 0;
        loop {
            if self.context_tokens() <= limit_range[0] || idx >= self.context.len() {
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
    completed: bool,
}
impl Exchange {
    pub fn partial(&mut self, cmpl: PartialCompletion) {
        if let Some(message) = cmpl.choices.into_iter().next() {
            let message = message.delta;
            if let Some(content) = &message.content {
                let content = content.replace("\n", "");
                if let Some(last_content) = self.response.as_mut().and_then(|x| x.content.as_mut())
                {
                    last_content.push_str(&content)
                } else {
                    let message = Message {
                        role: Role::Assistant,
                        content: Some(content),
                        ..Default::default()
                    };
                    self.response = Some(message);
                }
            }
        }
        if let Some(usage) = cmpl.usage {
            self.usage = Some(usage);
        }
    }

    pub fn complete(&mut self, cmpl: Completion) {
        let message = cmpl.choices.into_iter().next().unwrap().message;
        self.usage = Some(cmpl.usage);
        self.response = Some(message);
    }

    pub fn set_completed(&mut self) {
        self.completed = true;
    }

    pub fn prompt(&self) -> Vec<Message> {
        let mut messages = vec![];
        messages.push(self.system.clone());
        messages.extend(self.context.iter().cloned());
        messages.push(self.request.clone());
        messages
    }

    pub fn response(&self) -> Option<&Message> {
        self.response.as_ref()
    }

    pub fn usage(&self) -> Option<&Usage> {
        self.usage.as_ref()
    }

    pub fn is_completed(&self) -> bool {
        self.completed
    }
}
