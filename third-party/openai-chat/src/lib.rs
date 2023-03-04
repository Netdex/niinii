use std::sync::{Arc, Mutex};

use thiserror::Error;

pub use protocol::*;

mod protocol;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Network(#[from] reqwest::Error),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error("Response Error: {0}")]
    Response(#[from] ResponseError),
}

#[derive(Clone)]
pub struct Client {
    shared: Arc<Shared>,
}
struct Shared {
    token: String,
    state: Mutex<State>,
}
struct State {
    client: reqwest::blocking::Client,
}

impl Client {
    pub fn new(token: impl Into<String>) -> Self {
        let token = token.into();
        Self {
            shared: Arc::new(Shared {
                token,
                state: Mutex::new(State {
                    client: reqwest::blocking::Client::new(),
                }),
            }),
        }
    }
    pub fn completions(&self, request: &Request) -> Result<Completion, Error> {
        assert!(!request.stream.unwrap_or(false), "streaming not supported");
        let Shared { token, state } = &*self.shared;
        let State { client } = &mut *state.lock().unwrap();
        let response: Response = client
            .post("https://api.openai.com/v1/chat/completions")
            .bearer_auth(token)
            .json(request)
            .send()?
            .json()?;
        match response {
            Response::Completion(completion) => Ok(completion),
            Response::Error { error } => Err(Error::Response(error)),
        }
    }
}

#[cfg(test)]
mod tests {
    mod fixture;
    use super::*;

    #[test]
    fn test_completions() {
        let client = fixture::client();
        let request = Request {
            messages: vec![Message {
                role: Role::User,
                content: "What is the capital city of Canada?".to_string(),
            }],
            ..Default::default()
        };
        let response = client.completions(&request).unwrap();
        let content = &response.choices.first().unwrap().message.content;
        assert!(content.contains("Ottawa"));
    }
}
