use crate::Client;

pub fn client() -> Client<backon::ConstantBuilder> {
    let token = std::env::var("OPENAI_APIKEY").expect("OPENAI_APIKEY not specified");
    Client::new(token, Default::default())
}
