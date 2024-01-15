use std::collections::VecDeque;

use openai_chat::chat::{Message, PartialMessage, Role, Usage};

/// TODO: this code sucks ass, use the Assistants API instead

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ChatState {
    AcceptPrompt,
    AcceptResponse,
}

#[derive(Debug)]
pub struct ChatBuffer {
    state: ChatState,
    system: Option<Message>,
    context: VecDeque<Message>,
    response: VecDeque<Message>,
    usage: Option<Usage>,
}
impl ChatBuffer {
    pub fn new() -> Self {
        ChatBuffer {
            state: ChatState::AcceptPrompt,
            system: None,
            context: VecDeque::new(),
            response: VecDeque::new(),
            usage: None,
        }
    }

    pub fn begin_exchange(&mut self, system: Message, request: Message) {
        assert_eq!(self.state, ChatState::AcceptPrompt);

        self.system = Some(system);
        self.context.extend(self.response.drain(..));
        self.context.push_back(request);

        self.state = ChatState::AcceptResponse;
        self.usage = None;
    }

    pub fn cancel_exchange(&mut self) {
        assert_eq!(self.state, ChatState::AcceptResponse);

        self.context.pop_back();
        self.end_exchange();
    }

    pub fn append_partial_response(&mut self, partial: &PartialMessage) {
        assert_eq!(self.state, ChatState::AcceptResponse);

        if let Some(role) = &partial.role {
            let message = Message {
                role: role.clone(),
                content: Some(partial.content.clone()),
                ..Default::default()
            };
            self.response.push_back(message)
        } else if let Some(last) = self.response.back_mut() {
            if let Some(content) = &mut last.content {
                content.push_str(&partial.content)
            }
        }
    }

    pub fn end_exchange(&mut self) {
        assert_eq!(self.state, ChatState::AcceptResponse);

        self.state = ChatState::AcceptPrompt;
        self.system = None;
        self.usage = Some(self.estimate_usage());
    }

    pub fn enforce_context_limit(&mut self, limit: u32) {
        loop {
            if self.context_tokens() <= limit || self.context.len() == 1 {
                break;
            }
            self.context.pop_front();
            while let Some(message) = self.context.front() {
                if message.role == Role::User {
                    break;
                }
                self.context.pop_front();
            }
        }
    }

    pub fn clear(&mut self) {
        assert_eq!(self.state, ChatState::AcceptPrompt);

        self.context.clear();
        self.response.clear();
    }

    fn context_tokens(&self) -> u32 {
        self.context
            .iter()
            .map(|m| m.estimate_tokens())
            .sum::<u32>()
    }

    pub fn estimate_usage(&self) -> Usage {
        // every reply is primed with <im_start>assistant
        let prompt_tokens =
            self.context_tokens() + self.system.as_ref().map_or(0, |m| m.estimate_tokens()) + 2;
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

    pub fn context(&self) -> &VecDeque<Message> {
        &self.context
    }
    pub fn context_mut(&mut self) -> &mut VecDeque<Message> {
        &mut self.context
    }
    pub fn response(&self) -> &VecDeque<Message> {
        &self.response
    }
    pub fn state(&self) -> ChatState {
        self.state
    }
    pub fn usage(&self) -> Option<&Usage> {
        self.usage.as_ref()
    }
    pub fn prompt(&self) -> Vec<Message> {
        let mut messages = vec![];
        if let Some(system) = &self.system {
            messages.push(system.clone());
        }
        messages.extend(self.context.iter().cloned());
        messages
    }
}
