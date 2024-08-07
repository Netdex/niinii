use crate::Client;

pub fn client() -> Client<backon::ConstantBuilder> {
    let token = std::env::var("OPENAI_KEY").expect("OPENAI_KEY not specified");
    Client::new(token, Default::default())
}
