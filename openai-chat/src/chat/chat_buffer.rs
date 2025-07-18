//! Simple wrapper for a conversation using the completions API

use std::collections::VecDeque;

use crate::chat::{Message, PartialMessage, Role, Usage};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum State {
    AcceptPrompt,
    AcceptResponse,
}

#[derive(Debug)]
pub struct ChatBuffer {
    state: State,
    system: Option<Message>,
    context: VecDeque<Message>,
    response: VecDeque<Message>,
    usage: Option<Usage>,
}
impl Default for ChatBuffer {
    fn default() -> Self {
        Self::new()
    }
}
impl ChatBuffer {
    pub fn new() -> Self {
        ChatBuffer {
            state: State::AcceptPrompt,
            system: None,
            context: VecDeque::new(),
            response: VecDeque::new(),
            usage: None,
        }
    }

    pub fn begin_exchange(&mut self, system: Message, request: Message) {
        assert_eq!(self.state, State::AcceptPrompt);
        // TODO: This would be better if it returned a transaction which we
        // operate upon and requires finalization, so we don't have to cancel
        // a transaction in progress.

        self.system = Some(system);
        self.context.extend(self.response.drain(..));
        self.context.push_back(request);

        self.state = State::AcceptResponse;
        self.usage = None;
    }

    pub fn cancel_exchange(&mut self) {
        assert_eq!(self.state, State::AcceptResponse);

        self.context.pop_back();
        self.end_exchange();
    }

    pub fn append_partial_response(&mut self, partial: &PartialMessage) {
        assert_eq!(self.state, State::AcceptResponse);

        if let Some(last) = self.response.back_mut() {
            if let Some(content) = &mut last.content {
                content.push_str(&partial.content)
            }
        } else {
            let message = Message {
                // I would use the role from the response instead of hardcoding
                // 'Assistant' here, but llama.cpp doesn't put a role in the
                // response unlike OpenAI.
                role: Role::Assistant,
                content: Some(partial.content.clone()),
                ..Default::default()
            };
            self.response.push_back(message)
        }
    }

    pub fn end_exchange(&mut self) {
        assert_eq!(self.state, State::AcceptResponse);

        self.state = State::AcceptPrompt;
        self.system = None;
        self.usage = Some(self.estimate_usage());
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
        assert_eq!(self.state, State::AcceptPrompt);

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
    pub fn response_mut(&mut self) -> &mut VecDeque<Message> {
        &mut self.response
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

    pub fn pending_response(&self) -> bool {
        self.state == State::AcceptResponse
    }
}
