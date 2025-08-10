use crate::Client;

pub fn client() -> Client {
    let token = std::env::var("OPENAI_KEY").expect("OPENAI_KEY not specified");
    Client::new(token, "https://api.openai.com", Default::default())
    // let token = "no-key";
    // Client::new(token, "http://localhost:8080", Default::default())
}
