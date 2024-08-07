use derive_more::Display;
use serde::{Deserialize, Serialize};

use super::{
    chat::{Message, Model},
    Response,
};

#[derive(Debug, Display, Default, Clone, Deserialize, Serialize)]
#[serde(transparent)]
pub struct FileId(pub(crate) String);

#[derive(Debug, Display, Default, Clone, Deserialize, Serialize)]
#[serde(transparent)]
pub struct AssistantId(pub(crate) String);

#[derive(Debug, Display, Default, Clone, Deserialize, Serialize)]
#[serde(transparent)]
pub struct ThreadId(pub(crate) String);

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum Tool {
    CodeInterpreter,
    Retrieval,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "object", rename = "assistant")]
pub struct Assistant {
    pub id: AssistantId,
    pub created_at: u64,
    pub name: Option<String>,
    pub description: Option<String>,
    pub model: Model,
    pub instructions: Option<String>,
    pub tools: Vec<Tool>,
    pub file_ids: Vec<FileId>,
    // metadata
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Default, Clone, Serialize)]
pub struct CreateAssistantRequest {
    pub model: Model,
    pub name: Option<String>,
    pub description: Option<String>,
    pub instructions: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<Tool>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub file_ids: Vec<FileId>,
    // metadata
}
pub(crate) type CreateAssistantResponse = Response<Assistant>;

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "object", rename = "assistant.deleted")]
pub struct AssistantDeleted {
    pub id: AssistantId,
    pub deleted: bool,
}
pub(crate) type DeleteAssistantResponse = Response<AssistantDeleted>;

pub(crate) type CreateMessageResponse = Response<Message>;

// CreateAssistantFile
// ListAssistants
// ListAssistantFiles
