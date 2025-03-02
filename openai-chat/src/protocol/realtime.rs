use derive_more::derive::Display;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use strum_macros::{EnumIter, IntoStaticStr};

use super::{untagged_ok_result, Error, Result};

// TODO: maybe check that the string starts with X in the ctor
#[derive(Debug, Clone, Deserialize, Serialize, Display)]
#[serde(transparent)]
pub struct SessionId(String);

#[derive(Debug, Clone, Deserialize, Serialize, Display)]
#[serde(transparent)]
pub struct ConversationId(String);

#[derive(Debug, Clone, Deserialize, Serialize, Display)]
#[serde(transparent)]
pub struct ConversationItemId(String);

#[derive(Debug, Clone, Deserialize, Serialize, Display)]
#[serde(transparent)]
pub struct FunctionCallId(String);

#[derive(Debug, Clone, Deserialize, Serialize, Display)]
#[serde(transparent)]
pub struct EventId(String);

#[derive(Debug, Clone, Deserialize, Serialize, Display)]
#[serde(transparent)]
pub struct ResponseId(String);

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, EnumIter, IntoStaticStr)]
pub enum Model {
    #[serde(rename = "gpt-4o-realtime-preview")]
    Gpt4oRealtimePreview,
    #[serde(rename = "gpt-4o-realtime-preview-2024-12-17")]
    Gpt4oRealtimePreview20241217,
    #[default]
    #[serde(rename = "gpt-4o-mini-realtime-preview")]
    Gpt4oMiniRealtimePreview,
    #[serde(rename = "gpt-4o-mini-realtime-preview-2024-12-17")]
    Gpt4oMiniRealtimePreview20241217,
}

#[derive(Debug, Clone)]
pub enum MaxResponseOutputTokens {
    Finite(u32),
    Infinite,
}
impl Serialize for MaxResponseOutputTokens {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Finite(value) => serializer.serialize_u32(*value),
            Self::Infinite => serializer.serialize_str("inf"),
        }
    }
}
impl<'de> Deserialize<'de> for MaxResponseOutputTokens {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "inf" => Ok(Self::Infinite),
            _ => match s.parse::<u32>() {
                Ok(value) => Ok(Self::Finite(value)),
                Err(_) => Err(serde::de::Error::invalid_type(
                    serde::de::Unexpected::Str(&s),
                    &"an integer",
                )),
            },
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Modality {
    Audio,
    Text,
}

#[derive(Debug, Clone, Serialize)]
#[serde(transparent)]
pub struct CreateSessionRequest(pub SessionParameters);

#[derive(Debug, Clone, Deserialize)]
pub struct CreateSessionResponseInner {
    #[serde(flatten)]
    pub session_parameters: SessionParameters,
    pub client_secret: ClientSecret,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(transparent)]
pub struct CreateSessionResponse(
    #[serde(deserialize_with = "untagged_ok_result::deserialize")]
    pub  Result<CreateSessionResponseInner>,
);

#[derive(Debug, Clone, Deserialize)]
pub struct ClientSecret {
    pub value: String,
    pub expires_at: u64,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct InferenceParameters {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub modalities: Vec<Modality>,
    pub model: Option<Model>,
    pub instructions: Option<String>,
    pub temperature: Option<f32>,
    pub max_response_output_tokens: Option<MaxResponseOutputTokens>,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SessionParameters {
    // turn_detection
    // input_audio_format
    // input_audio_transcription
    // voice
    // output_audio_format
    // tools
    // tool_choice
    #[serde(flatten)]
    pub inference_parameters: InferenceParameters,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationItemStatus {
    #[default]
    Incomplete,
    InProgress,
    Completed,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    #[default]
    User,
    Assistant,
    Function,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConversationItemContent {
    InputText { text: String },
    InputAudio { audio: String, transcript: String },
    ItemReference { id: ConversationItemId },
    Text { text: String },
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConversationItemBody {
    Message {
        role: Role,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        content: Vec<ConversationItemContent>,
    },
    FunctionCall {
        call_id: FunctionCallId,
        name: String,
        arguments: String, // TODO: nested json?
    },
    FunctionCallOutput {
        call_id: FunctionCallId,
        output: String, // TODO: nested json?
    },
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConversationItem {
    pub id: Option<ConversationItemId>,
    #[serde(default)]
    pub status: ConversationItemStatus,
    #[serde(flatten)]
    pub body: ConversationItemBody,
}
impl ConversationItem {
    pub fn input_text(message: impl Into<String>) -> Self {
        ConversationItem {
            id: None,
            status: ConversationItemStatus::Incomplete,
            body: ConversationItemBody::Message {
                role: Role::User,
                content: vec![ConversationItemContent::InputText {
                    text: message.into(),
                }],
            },
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseConversation {
    #[default]
    Auto,
    None,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Default, Serialize)]
pub struct ResponseParameters {
    #[serde(flatten)]
    pub inference_parameters: Option<InferenceParameters>,
    pub conversation: ResponseConversation,
    // metadata
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub input: Vec<ConversationItem>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseStatusDetails {
    Completed,
    Cancelled { reason: String },
    Incomplete { reason: String },
    Failed { error: Error },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseStatus {
    Completed,
    Cancelled,
    Incomplete,
    Failed,
    InProgress,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InputTokenDetails {
    pub cached_tokens: u32,
    pub text_tokens: u32,
    pub audio_tokens: u32,
}
#[derive(Debug, Clone, Deserialize)]
pub struct OutputTokenDetails {
    pub text_tokens: u32,
    pub audio_tokens: u32,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Usage {
    pub total_tokens: u32,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub input_token_details: InputTokenDetails,
    pub output_token_details: OutputTokenDetails,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Response {
    pub id: ResponseId,
    pub status: ResponseStatus,
    pub status_details: Option<ResponseStatusDetails>,
    pub output: Vec<ConversationItem>,
    // metadata
    pub usage: Option<Usage>,
    pub conversation_id: ConversationId,
    #[serde(flatten)]
    pub inference_parameters: InferenceParameters,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Part {
    Text {
        text: String,
    },
    Audio {
        audio: Option<String>,
        transcript: String,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RateLimitName {
    Requests,
    Tokens,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimits {
    pub name: RateLimitName,
    pub limit: u32,
    pub remaining: u32,
    pub reset_seconds: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConversationItemCreated {
    pub previous_item_id: Option<ConversationItemId>,
    pub item: ConversationItem,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResponseOutputItem {
    pub response_id: ResponseId,
    pub output_index: u32,
    pub item: ConversationItem,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResponseContentPart {
    pub response_id: ResponseId,
    pub item_id: ConversationItemId,
    pub output_index: u32,
    pub content_index: u32,
    pub part: Part,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResponseTextDelta {
    pub response_id: ResponseId,
    pub item_id: ConversationItemId,
    pub output_index: u32,
    pub content_index: u32,
    pub delta: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResponseTextDone {
    pub response_id: ResponseId,
    pub item_id: ConversationItemId,
    pub output_index: u32,
    pub content_index: u32,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResponseFunctionCallArgumentsDelta {
    pub response_id: ResponseId,
    pub item_id: ConversationItemId,
    pub output_index: u32,
    pub call_id: FunctionCallId,
    pub delta: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResponseFunctionCallArgumentsDone {
    pub response_id: ResponseId,
    pub item_id: ConversationItemId,
    pub output_index: u32,
    pub call_id: FunctionCallId,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClientEventRequest {
    pub event_id: Option<EventId>,
    #[serde(flatten)]
    pub event: ClientEvent,
}
impl<'a> TryFrom<&'a ClientEventRequest> for tokio_tungstenite::tungstenite::Message {
    type Error = serde_json::Error;

    fn try_from(value: &'a ClientEventRequest) -> std::result::Result<Self, Self::Error> {
        Ok(Self::Text(serde_json::to_string(value)?.into()))
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum ClientEvent {
    #[serde(rename = "session.update")]
    SessionUpdate { session: SessionParameters },
    #[serde(rename = "conversation.item.create")]
    ConversationItemCreate { item: ConversationItem },
    // conversation.item.truncate
    #[serde(rename = "conversation.item.delete")]
    ConversationItemDelete { item_id: ConversationItemId },
    #[serde(rename = "response.create")]
    ResponseCreate { response: ResponseParameters },
    #[serde(rename = "response.cancel")]
    ResponseCancel { response_id: Option<ResponseId> },
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerEventResponse {
    pub event_id: String,
    #[serde(flatten)]
    #[serde(deserialize_with = "untagged_ok_result::deserialize")]
    pub event: Result<ServerEvent>,
}
impl<'a> TryFrom<&'a tokio_tungstenite::tungstenite::Message> for ServerEventResponse {
    type Error = serde_json::Error;

    fn try_from(
        value: &'a tokio_tungstenite::tungstenite::Message,
    ) -> std::result::Result<Self, Self::Error> {
        match value {
            tokio_tungstenite::tungstenite::Message::Text(value) => {
                Ok(serde_json::from_str(value)?)
            }
            _ => Err(serde::de::Error::custom(format!(
                "cannot convert to ServerEventResponse from: {:?}",
                value
            ))),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Session {
    pub id: SessionId,
    #[serde(flatten)]
    pub session_parameters: SessionParameters,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum ServerEvent {
    #[serde(rename = "session.created")]
    SessionCreated { session: Session },
    #[serde(rename = "session.updated")]
    SessionUpdated { session: Session },
    #[serde(rename = "conversation.updated")]
    ConversationCreated { id: ConversationId },
    #[serde(rename = "conversation.item.created")]
    ConversationItemCreated(ConversationItemCreated),
    #[serde(rename = "conversation.item.deleted")]
    ConversationItemDeleted { item_id: ConversationItemId },
    #[serde(rename = "response.created")]
    ResponseCreated { response: Response },
    #[serde(rename = "response.done")]
    ResponseDone { response: Response },
    #[serde(rename = "response.output_item.added")]
    ResponseOutputItemAdded(ResponseOutputItem),
    #[serde(rename = "response.output_item.done")]
    ResponseOutputItemDone(ResponseOutputItem),
    #[serde(rename = "response.content_part.added")]
    ResponseContentPartAdded(ResponseContentPart),
    #[serde(rename = "response.content_part.done")]
    ResponseContentPartDone(ResponseContentPart),
    #[serde(rename = "response.text.delta")]
    ResponseTextDelta(ResponseTextDelta),
    #[serde(rename = "response.text.done")]
    ResponseTextDone(ResponseTextDone),
    #[serde(rename = "response.function_call_arguments.delta")]
    ResponseFunctionCallArgumentsDelta(ResponseFunctionCallArgumentsDelta),
    #[serde(rename = "response.function_call_arguments.done")]
    ResponseFunctionCallArgumentsDone(ResponseFunctionCallArgumentsDone),
    #[serde(rename = "rate_limits.updated")]
    RateLimitsUpdated { rate_limits: Vec<RateLimits> },
}
