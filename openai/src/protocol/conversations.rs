use std::collections::HashMap;

use serde::Deserialize;
use serde_json::Value;

/// https://platform.openai.com/docs/api-reference/conversations/create
#[derive(Debug, Clone, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub object: String,
    pub created_at: u64,
    #[serde(default)]
    pub metadata: HashMap<String, Value>,
}
