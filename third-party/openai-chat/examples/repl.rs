use std::io::BufRead;

use openai_chat::{Client, Message, Model, Request, Role};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let stdin = std::io::stdin();
    let token = std::env::var("OPENAI_APIKEY").unwrap();
    let client = Client::new(token);
    let mut conversation = client.conversation(Request {
        model: Model::Gpt35Turbo,
        messages: vec![Message {
            role: Role::System,
            content: "Translate the following conversation from Japanese to English".to_string(),
        }],
        ..Default::default()
    });
    for line in stdin.lock().lines() {
        let response = conversation.prompt(line.unwrap())?;
        println!("{}", response.choices.first().unwrap().message.content);
    }
    Ok(())
}
