//! Translator runtime: a backend-agnostic surface over the concrete backends
//! (`chat`, `responses`).
//!
//! Both backends share the same command/event/state shape (commands from the
//! UI, events from adapter tasks, immutable `ArcSwap` state snapshots for
//! wait-free reads). The [`Backend`] trait exposes the operations the UI and the
//! VNDB integration need without caring which backend is active. Backend-private
//! surface (the chat context editor, the responses chain) stays on the concrete
//! handles.

pub mod chat;
pub mod realtime;
pub mod responses;

use std::sync::Arc;

use openai::{
    chat::{Tool, ToolCall, ToolCallAccumulator, ToolChoice},
    realtime::RealtimeTruncation,
    ModelId, ReasoningEffort, ServiceTier, Verbosity,
};

use crate::settings::{Settings, TranslatorType, TruncationMode};

pub use chat::{ChatHandle, ChatState, ContextEdit, MsgId};
pub use realtime::RealtimeHandle;
pub use responses::ResponsesHandle;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExchangeId(pub u64);

/// Per-request parameters snapshotted from `Settings` when a translation is
/// submitted. This is the superset across backends; each backend reads only the
/// subset it needs. The backends never read `Settings` live.
#[derive(Clone, Debug)]
pub struct TranslateConfig {
    pub model: ModelId,
    pub system_prompt: String,
    /// Chat only: local context-buffer trim target/threshold.
    pub max_context_tokens: [u32; 2],
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    /// Chat: `max_completion_tokens`. Responses: `max_output_tokens`.
    pub max_tokens: Option<u32>,
    /// Chat only.
    pub presence_penalty: Option<f32>,
    pub service_tier: Option<ServiceTier>,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub verbosity: Option<Verbosity>,
    /// Responses only: chain turns server-side via `previous_response_id`. When
    /// false, each turn is independent (no chaining, `store: false`).
    pub chain: bool,
    pub stream: bool,
    /// Realtime only: conversation-truncation strategy sent on the session.
    /// `None` for the other backends (which have no such concept).
    pub truncation: Option<RealtimeTruncation>,
    /// Chat only.
    pub tools: Vec<Tool>,
    /// Chat only.
    pub tool_choice: Option<ToolChoice>,
}

impl TranslateConfig {
    /// Snapshot the per-request knobs for whichever backend is active.
    pub fn from_settings(settings: &Settings) -> Self {
        match settings.translator_type {
            TranslatorType::Chat => {
                let c = &settings.chat;
                Self {
                    model: settings.openai_model.clone(),
                    system_prompt: c.system_prompt.clone(),
                    max_context_tokens: c.max_context_tokens,
                    temperature: c.temperature,
                    top_p: c.top_p,
                    max_tokens: c.max_tokens,
                    presence_penalty: c.presence_penalty,
                    service_tier: c.service_tier,
                    reasoning_effort: c.reasoning_effort,
                    verbosity: c.verbosity,
                    chain: true,
                    stream: c.stream,
                    truncation: None,
                    tools: Vec::new(),
                    tool_choice: None,
                }
            }
            TranslatorType::Responses => {
                let r = &settings.responses;
                Self {
                    model: settings.openai_model.clone(),
                    system_prompt: r.system_prompt.clone(),
                    max_context_tokens: [0, 0],
                    temperature: r.temperature,
                    top_p: r.top_p,
                    max_tokens: r.max_tokens,
                    presence_penalty: None,
                    service_tier: r.service_tier,
                    reasoning_effort: r.reasoning_effort,
                    verbosity: r.verbosity,
                    chain: r.chain,
                    stream: r.stream,
                    truncation: None,
                    tools: Vec::new(),
                    tool_choice: None,
                }
            }
            TranslatorType::Realtime => {
                let rt = &settings.realtime;
                Self {
                    model: settings.openai_model.clone(),
                    system_prompt: rt.system_prompt.clone(),
                    max_context_tokens: [0, 0],
                    temperature: None,
                    top_p: None,
                    max_tokens: rt.max_tokens,
                    presence_penalty: None,
                    service_tier: None,
                    reasoning_effort: rt.reasoning_effort,
                    verbosity: None,
                    chain: rt.chain,
                    // The Realtime API is always a streaming session.
                    stream: true,
                    // Always send an explicit truncation: a `session.update`
                    // omitting the field leaves the prior value in place, so a
                    // mode change (e.g. Disabled -> Auto) would otherwise be a
                    // no-op on the server.
                    truncation: Some(match rt.truncation {
                        TruncationMode::Auto => RealtimeTruncation::Auto,
                        TruncationMode::Disabled => RealtimeTruncation::Disabled,
                        TruncationMode::RetentionRatio => RealtimeTruncation::RetentionRatio {
                            ratio: rt.truncation_retention_ratio,
                            post_instructions_token_limit: rt
                                .truncation_post_instructions_token_limit,
                        },
                    }),
                    tools: Vec::new(),
                    tool_choice: None,
                }
            }
        }
    }
}

/// Assistant turn state, shared by both backends. The `tool_calls` fields are
/// chat-only; the responses backend leaves them empty. Rendering ignores them.
#[derive(Clone, Debug)]
pub enum Response {
    Streaming {
        content: String,
        reasoning: String,
        tool_calls: ToolCallAccumulator,
    },
    Completed {
        content: String,
        reasoning: String,
        tool_calls: Vec<ToolCall>,
    },
    Errored(Arc<str>),
    Cancelled,
}

impl Response {
    /// Text rendered for the assistant turn so far. Works during streaming and
    /// post-completion.
    pub fn content(&self) -> &str {
        match self {
            Response::Streaming { content, .. } | Response::Completed { content, .. } => content,
            Response::Errored(_) | Response::Cancelled => "",
        }
    }
    /// Reasoning trace/summary, if any. Not echoed back into context.
    pub fn reasoning(&self) -> &str {
        match self {
            Response::Streaming { reasoning, .. } | Response::Completed { reasoning, .. } => {
                reasoning
            }
            Response::Errored(_) | Response::Cancelled => "",
        }
    }
    pub fn is_terminal(&self) -> bool {
        !matches!(self, Response::Streaming { .. })
    }
}

/// Token accounting normalized across backends. `cached_tokens` surfaces
/// prompt-cache hits so the latency optimizations are observable.
#[derive(Clone, Debug, Default)]
pub struct UsageView {
    pub input_tokens: u32,
    pub cached_tokens: u32,
    pub output_tokens: u32,
    pub reasoning_tokens: u32,
    pub total_tokens: u32,
}

/// Render-facing snapshot of one exchange. Returned by [`Backend::exchange`] and
/// consumed by the shared exchange/usage renderers.
#[derive(Clone, Debug)]
pub struct ExchangeView {
    pub id: ExchangeId,
    pub model: ModelId,
    pub response: Response,
    pub usage: Option<UsageView>,
}

/// Backend-agnostic operations the UI and the VNDB integration use. Both
/// `ChatHandle` and `ResponsesHandle` implement this; handles are cheap to clone
/// and the methods are all wait-free (commands are queued, reads are snapshots).
pub trait Backend: Send + Sync + 'static {
    fn translate(&self, text: String, config: Arc<TranslateConfig>) -> ExchangeId;
    fn cancel(&self, id: ExchangeId);
    /// Chat: clear the context buffer. Responses: reset the `previous_response_id`
    /// chain so the next turn starts a fresh conversation.
    fn clear(&self);
    fn refresh_models(&self);
    fn set_system_addendum(&self, text: Option<Arc<str>>);
    fn exchange(&self, id: ExchangeId) -> Option<ExchangeView>;
    fn models(&self) -> Vec<ModelId>;
    fn last_error(&self) -> Option<Arc<str>>;
}
