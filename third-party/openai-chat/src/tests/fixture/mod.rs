use crate::Client;

pub fn client() -> Client {
    let token = std::env::var("OPENAI_APIKEY").unwrap();
    Client::new(token)
}
