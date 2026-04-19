//! Shared helpers for integration tests against a live OpenAI-compatible server
//! (OpenAI, llama.cpp, vLLM, Ollama, etc.).
//!
//! Config is read from the workspace `niinii.toml`. Tests acquire the client
//! and model via [`live_server!`], which skips (prints a notice and returns)
//! when the config file or required fields are missing — so `cargo test` is
//! always safe to run.
//!
//! Required in `niinii.toml`:
//! - `[chat].api_endpoint`
//! - `[chat].model`
//!
//! Optional:
//! - `openai_api_key` (defaults to `"no-key"` for local servers)

use openai::{Client, ModelId};

const CONFIG_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../niinii.toml");

pub struct LiveConfig {
    pub endpoint: String,
    pub model: String,
    pub api_key: String,
}

pub fn load_config() -> Option<LiveConfig> {
    let text = std::fs::read_to_string(CONFIG_PATH).ok()?;
    let v: toml::Value = toml::from_str(&text).ok()?;
    let chat = v.get("chat")?;
    let endpoint = chat.get("api_endpoint")?.as_str()?.to_string();
    let model = chat.get("model")?.as_str()?.to_string();
    let api_key = v
        .get("openai_api_key")
        .and_then(|k| k.as_str())
        .unwrap_or("no-key")
        .to_string();
    Some(LiveConfig {
        endpoint,
        model,
        api_key,
    })
}

pub fn build(cfg: LiveConfig) -> (Client, ModelId) {
    let model = ModelId(cfg.model);
    let client = Client::new(cfg.api_key, cfg.endpoint, Default::default());
    (client, model)
}

/// Acquire `(Client, ModelId)` or skip the enclosing test (prints a notice
/// and `return`s) if `niinii.toml` is missing or incomplete. The skip branch
/// is why this is a macro — a function can't return from its caller.
#[macro_export]
macro_rules! live_server {
    () => {
        match $crate::common::load_config() {
            Some(cfg) => $crate::common::build(cfg),
            None => {
                fn f() {}
                fn type_name_of<T>(_: T) -> &'static str {
                    std::any::type_name::<T>()
                }
                let test_name = type_name_of(f).strip_suffix("::f").unwrap_or("<test>");
                eprintln!(
                    "SKIP {}: niinii.toml missing [chat].api_endpoint / [chat].model",
                    test_name
                );
                return;
            }
        }
    };
}
