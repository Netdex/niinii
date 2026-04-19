//! Chat Completions backend for the translator runtime.
//!
//! Implements [`Backend`] for [`ChatBackend`]. Shape:
//!
//! - Commands (external, from the UI): translate, cancel, edit the context
//!   buffer, refresh the models list.
//! - Events (internal, from adapter tasks): stream start, token deltas,
//!   completion, failure, models refreshed.
//! - State (published as immutable snapshots): editable context buffer,
//!   in-flight + completed exchanges, models list, last error.
//!
//! Per-request parameters (`TranslateConfig`) are snapshotted from `Settings`
//! at submission time; nothing in this module reads `Settings` live.

use std::{
    collections::{HashMap, VecDeque},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use arc_swap::ArcSwap;
use enclose::enclose;
use openai::{
    chat::{
        self, Message, PartialToolCall, Role, ToolCall, ToolCallAccumulator, Tool, ToolChoice, Usage,
    },
    ConnectionPolicy, ModelId, ReasoningEffort, ServiceTier, Verbosity,
};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;

use crate::settings::Settings;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExchangeId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MsgId(pub u64);

#[derive(Clone, Debug)]
pub struct ContextMessage {
    pub id: MsgId,
    pub message: Message,
}

/// Per-request parameters snapshotted from `Settings` when a translation is
/// submitted. The backend never reads `Settings` directly.
#[derive(Clone, Debug)]
pub struct TranslateConfig {
    pub model: ModelId,
    pub system_prompt: String,
    pub max_context_tokens: [u32; 2],
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub max_tokens: Option<u32>,
    pub presence_penalty: Option<f32>,
    pub service_tier: Option<ServiceTier>,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub verbosity: Option<Verbosity>,
    pub stream: bool,
    pub tools: Vec<Tool>,
    pub tool_choice: Option<ToolChoice>,
}

impl TranslateConfig {
    pub fn from_settings(settings: &Settings) -> Self {
        let c = &settings.chat;
        Self {
            model: c.model.clone(),
            system_prompt: c.system_prompt.clone(),
            max_context_tokens: c.max_context_tokens,
            temperature: c.temperature,
            top_p: c.top_p,
            max_tokens: c.max_tokens,
            presence_penalty: c.presence_penalty,
            service_tier: c.service_tier,
            reasoning_effort: c.reasoning_effort,
            verbosity: c.verbosity,
            stream: c.stream,
            tools: Vec::new(),
            tool_choice: None,
        }
    }
}

#[derive(Clone, Debug)]
pub enum Response {
    Streaming { content: String, tool_calls: ToolCallAccumulator },
    Completed { content: String, tool_calls: Vec<ToolCall> },
    Errored(Arc<str>),
    Cancelled,
}

impl Response {
    /// Text rendered for the assistant turn so far. Works during streaming
    /// and post-completion.
    pub fn content(&self) -> &str {
        match self {
            Response::Streaming { content, .. } | Response::Completed { content, .. } => content,
            Response::Errored(_) | Response::Cancelled => "",
        }
    }
    pub fn is_terminal(&self) -> bool {
        !matches!(self, Response::Streaming { .. })
    }
}

#[derive(Clone, Debug)]
pub struct ExchangeView {
    pub id: ExchangeId,
    pub model: ModelId,
    pub user_message: Message,
    pub response: Response,
    pub usage: Option<Usage>,
}

#[derive(Clone, Debug, Default)]
pub struct ChatState {
    pub context: VecDeque<ContextMessage>,
    next_msg_id: u64,
    pub exchanges: Vec<ExchangeView>,
    pub models: Vec<ModelId>,
    pub last_error: Option<Arc<str>>,
}

impl ChatState {
    pub fn exchange(&self, id: ExchangeId) -> Option<&ExchangeView> {
        self.exchanges.iter().find(|e| e.id == id)
    }
    fn mint_id(&mut self) -> MsgId {
        let id = MsgId(self.next_msg_id);
        self.next_msg_id += 1;
        id
    }
    fn push_back(&mut self, message: Message) {
        let id = self.mint_id();
        self.context.push_back(ContextMessage { id, message });
    }
}

#[derive(Debug)]
pub enum ContextEdit {
    Insert { idx: usize, message: Message },
    Delete(usize),
    Swap(usize, usize),
    SetContent { idx: usize, content: String },
    SetRole { idx: usize, role: Role },
    SetName { idx: usize, name: Option<String> },
}

pub enum ChatCommand {
    Translate {
        id: ExchangeId,
        text: String,
        config: Arc<TranslateConfig>,
    },
    Cancel(ExchangeId),
    EditContext(ContextEdit),
    ClearContext,
    RefreshModels,
}

pub enum ChatEvent {
    Started {
        id: ExchangeId,
        model: ModelId,
        user_message: Message,
    },
    Delta {
        id: ExchangeId,
        content: String,
    },
    ToolCallDelta {
        id: ExchangeId,
        partials: Vec<PartialToolCall>,
    },
    Completed {
        id: ExchangeId,
        usage: Option<Usage>,
        max_context_tokens: [u32; 2],
    },
    Failed {
        id: ExchangeId,
        error: Arc<str>,
    },
    Cancelled {
        id: ExchangeId,
    },
    ModelsRefreshed(Vec<ModelId>),
    Error(Arc<str>),
}

fn handle_command(
    cmd: ChatCommand,
    state: &mut ChatState,
    client: &openai::Client,
    inflight: &mut HashMap<ExchangeId, CancellationToken>,
    evt_tx: &mpsc::Sender<ChatEvent>,
) {
    match cmd {
        ChatCommand::Translate { id, text, config } => {
            let user_message = Message {
                role: Role::User,
                content: Some(text),
                ..Default::default()
            };
            let prompt = build_prompt(state, &config, &user_message);
            // Synchronously seed the exchange -- no channel trip needed since
            // we're already holding the state.
            reduce(
                state,
                ChatEvent::Started {
                    id,
                    model: config.model.clone(),
                    user_message,
                },
            );
            let cancel = CancellationToken::new();
            inflight.insert(id, cancel.clone());
            spawn_adapter(client.clone(), config, prompt, id, cancel, evt_tx.clone());
        }
        ChatCommand::Cancel(id) => {
            if let Some(tok) = inflight.remove(&id) {
                tok.cancel();
                reduce(state, ChatEvent::Cancelled { id });
            }
        }
        ChatCommand::EditContext(edit) => apply_edit(state, edit),
        ChatCommand::ClearContext => state.context.clear(),
        ChatCommand::RefreshModels => {
            let client = client.clone();
            let tx = evt_tx.clone();
            tokio::spawn(async move {
                match client.models().await {
                    Ok(mut models) => {
                        models.sort();
                        let _ = tx.send(ChatEvent::ModelsRefreshed(models)).await;
                    }
                    Err(err) => {
                        tracing::error!(?err, "failed to refresh models");
                        let _ = tx.send(ChatEvent::Error(Arc::from(err.to_string()))).await;
                    }
                }
            });
        }
    }
}

fn reduce(state: &mut ChatState, event: ChatEvent) {
    match event {
            ChatEvent::Started {
                id,
                model,
                user_message,
            } => {
                state.exchanges.push(ExchangeView {
                    id,
                    model,
                    user_message,
                    response: Response::Streaming {
                        content: String::new(),
                        tool_calls: ToolCallAccumulator::new(),
                    },
                    usage: None,
                });
            }
            ChatEvent::Delta { id, content } => {
                if let Some(ex) = find_mut(&mut state.exchanges, id) {
                    if let Response::Streaming {
                        content: acc,
                        ..
                    } = &mut ex.response
                    {
                        acc.push_str(&content);
                    }
                }
            }
            ChatEvent::ToolCallDelta { id, partials } => {
                if let Some(ex) = find_mut(&mut state.exchanges, id) {
                    if let Response::Streaming { tool_calls, .. } = &mut ex.response {
                        tool_calls.extend(partials);
                    }
                }
            }
            ChatEvent::Completed {
                id,
                usage,
                max_context_tokens,
            } => {
                let Some(ex) = find_mut(&mut state.exchanges, id) else {
                    return;
                };
                let prior = std::mem::replace(&mut ex.response, Response::Cancelled);
                let (content, tool_calls) = match prior {
                    Response::Streaming {
                        content,
                        tool_calls,
                    } => (content, tool_calls.finish()),
                    other => {
                        ex.response = other;
                        return;
                    }
                };
                ex.response = Response::Completed {
                    content: content.clone(),
                    tool_calls: tool_calls.clone(),
                };
                ex.usage = usage;
                let assistant = Message {
                    role: Role::Assistant,
                    content: Some(content),
                    tool_calls: (!tool_calls.is_empty()).then_some(tool_calls),
                    ..Default::default()
                };
                let user_clone = ex.user_message.clone();
                state.push_back(user_clone);
                state.push_back(assistant);
                enforce_context_limit(&mut state.context, &max_context_tokens);
            }
            ChatEvent::Failed { id, error } => {
                if let Some(ex) = find_mut(&mut state.exchanges, id) {
                    ex.response = Response::Errored(error.clone());
                }
                state.last_error = Some(error);
            }
            ChatEvent::Cancelled { id } => {
                if let Some(ex) = find_mut(&mut state.exchanges, id) {
                    if let Response::Streaming { .. } = ex.response {
                        ex.response = Response::Cancelled;
                    }
                }
            }
        ChatEvent::ModelsRefreshed(models) => state.models = models,
        ChatEvent::Error(err) => state.last_error = Some(err),
    }
}

fn find_mut(exchanges: &mut [ExchangeView], id: ExchangeId) -> Option<&mut ExchangeView> {
    exchanges.iter_mut().find(|e| e.id == id)
}

fn build_prompt(
    state: &ChatState,
    config: &TranslateConfig,
    user: &Message,
) -> Vec<Message> {
    let mut prompt = Vec::with_capacity(state.context.len() + 2);
    prompt.push(Message {
        role: Role::System,
        content: Some(config.system_prompt.clone()),
        ..Default::default()
    });
    prompt.extend(state.context.iter().map(|e| e.message.clone()));
    prompt.push(user.clone());
    prompt
}

fn apply_edit(state: &mut ChatState, edit: ContextEdit) {
    let context = &mut state.context;
    match edit {
        ContextEdit::Insert { idx, message } => {
            let idx = idx.min(context.len());
            let id = MsgId(state.next_msg_id);
            state.next_msg_id += 1;
            state.context.insert(idx, ContextMessage { id, message });
        }
        ContextEdit::Delete(idx) => {
            if idx < context.len() {
                context.remove(idx);
            }
        }
        ContextEdit::Swap(a, b) => {
            if a < context.len() && b < context.len() {
                context.swap(a, b);
            }
        }
        ContextEdit::SetContent { idx, content } => {
            if let Some(entry) = context.get_mut(idx) {
                entry.message.content = Some(content);
            }
        }
        ContextEdit::SetRole { idx, role } => {
            if let Some(entry) = context.get_mut(idx) {
                entry.message.role = role;
            }
        }
        ContextEdit::SetName { idx, name } => {
            if let Some(entry) = context.get_mut(idx) {
                entry.message.name = name;
            }
        }
    }
}

/// Trim the oldest non-pinned messages until token count is under
/// `limits[0]`. Messages with a `name` set are treated as pinned.
/// A trimmed message pulls along any following non-user messages so the
/// remaining buffer always starts at a user turn.
fn enforce_context_limit(context: &mut VecDeque<ContextMessage>, limits: &[u32; 2]) {
    if count_tokens(context) <= limits[1] {
        return;
    }
    let mut idx = 0;
    while count_tokens(context) > limits[0] && idx < context.len() {
        if context[idx].message.name.is_some() {
            idx += 1;
            continue;
        }
        context.remove(idx);
        while let Some(entry) = context.get(idx) {
            if entry.message.role == Role::User {
                break;
            }
            context.remove(idx);
        }
    }
}

fn count_tokens(context: &VecDeque<ContextMessage>) -> u32 {
    context.iter().map(|e| e.message.estimate_tokens()).sum()
}

fn spawn_adapter(
    client: openai::Client,
    config: Arc<TranslateConfig>,
    prompt: Vec<Message>,
    id: ExchangeId,
    cancel: CancellationToken,
    evt_tx: mpsc::Sender<ChatEvent>,
) {
    tokio::spawn(
        enclose! { (config) async move {
            let req = chat::Request::builder()
                .model(config.model.clone())
                .messages(prompt)
                .maybe_temperature(config.temperature)
                .maybe_top_p(config.top_p)
                .maybe_max_completion_tokens(config.max_tokens)
                .maybe_presence_penalty(config.presence_penalty)
                .maybe_service_tier(config.service_tier)
                .maybe_reasoning_effort(config.reasoning_effort)
                .maybe_verbosity(config.verbosity)
                .build();

            let max_ctx = config.max_context_tokens;
            if config.stream {
                let mut stream = match client.stream(req).await {
                    Ok(s) => s,
                    Err(err) => {
                        let _ = evt_tx.send(ChatEvent::Failed {
                            id,
                            error: Arc::from(err.to_string()),
                        }).await;
                        return;
                    }
                };
                let mut usage = None;
                loop {
                    tokio::select! {
                        biased;
                        _ = cancel.cancelled() => {
                            let _ = evt_tx.send(ChatEvent::Cancelled { id }).await;
                            return;
                        }
                        chunk = stream.next() => match chunk {
                            Some(Ok(cmpl)) => {
                                if let Some(u) = cmpl.usage { usage = Some(u); }
                                for choice in cmpl.choices {
                                    if let Some(content) = choice.delta.content {
                                        let _ = evt_tx.send(ChatEvent::Delta {
                                            id,
                                            content: content.replace('\n', ""),
                                        }).await;
                                    }
                                    if let Some(calls) = choice.delta.tool_calls {
                                        let _ = evt_tx.send(ChatEvent::ToolCallDelta {
                                            id,
                                            partials: calls,
                                        }).await;
                                    }
                                }
                            }
                            Some(Err(err)) => {
                                let _ = evt_tx.send(ChatEvent::Failed {
                                    id,
                                    error: Arc::from(err.to_string()),
                                }).await;
                                return;
                            }
                            None => {
                                let _ = evt_tx.send(ChatEvent::Completed {
                                    id, usage, max_context_tokens: max_ctx,
                                }).await;
                                return;
                            }
                        }
                    }
                }
            } else {
                tokio::select! {
                    biased;
                    _ = cancel.cancelled() => {
                        let _ = evt_tx.send(ChatEvent::Cancelled { id }).await;
                    }
                    res = client.chat(req) => match res {
                        Ok(cmpl) => {
                            let usage = Some(cmpl.usage.clone());
                            if let Some(choice) = cmpl.choices.into_iter().next() {
                                if let Some(content) = choice.message.content {
                                    let _ = evt_tx.send(ChatEvent::Delta { id, content }).await;
                                }
                                if let Some(calls) = choice.message.tool_calls {
                                    let partials = calls.into_iter().enumerate()
                                        .map(|(i, call)| PartialToolCall {
                                            index: i as u32,
                                            id: Some(call.id),
                                            kind: Some(call.kind),
                                            function: Some(openai::chat::PartialFunctionCall {
                                                name: Some(call.function.name),
                                                arguments: Some(call.function.arguments),
                                            }),
                                        })
                                        .collect();
                                    let _ = evt_tx.send(ChatEvent::ToolCallDelta { id, partials }).await;
                                }
                            }
                            let _ = evt_tx.send(ChatEvent::Completed {
                                id, usage, max_context_tokens: max_ctx,
                            }).await;
                        }
                        Err(err) => {
                            let _ = evt_tx.send(ChatEvent::Failed {
                                id, error: Arc::from(err.to_string()),
                            }).await;
                        }
                    }
                }
            }
        }.instrument(tracing::Span::current())},
    );
}

/// Handle to the chat backend task. Cheap to clone. All mutations go through
/// `cmd_tx`; reads go through `state` (wait-free snapshot). `next_id` is owned
/// here so `translate()` can return an `ExchangeId` synchronously.
#[derive(Clone)]
pub struct ChatHandle {
    cmd_tx: mpsc::Sender<ChatCommand>,
    state: Arc<ArcSwap<ChatState>>,
    next_id: Arc<AtomicU64>,
}

impl ChatHandle {
    pub fn state(&self) -> Arc<ChatState> {
        self.state.load_full()
    }
    fn send(&self, cmd: ChatCommand) {
        let _ = self.cmd_tx.try_send(cmd);
    }
    pub fn translate(&self, text: String, config: Arc<TranslateConfig>) -> ExchangeId {
        let id = ExchangeId(self.next_id.fetch_add(1, Ordering::Relaxed));
        self.send(ChatCommand::Translate { id, text, config });
        id
    }
    pub fn cancel(&self, id: ExchangeId) {
        self.send(ChatCommand::Cancel(id));
    }
    pub fn edit_context(&self, edit: ContextEdit) {
        self.send(ChatCommand::EditContext(edit));
    }
    pub fn clear_context(&self) {
        self.send(ChatCommand::ClearContext);
    }
    pub fn refresh_models(&self) {
        self.send(ChatCommand::RefreshModels);
    }
}

pub fn spawn(settings: &Settings) -> ChatHandle {
    let client = openai::Client::new(
        &settings.openai_api_key,
        &settings.chat.api_endpoint,
        ConnectionPolicy {
            timeout: Duration::from_millis(settings.chat.timeout),
            connect_timeout: Duration::from_millis(settings.chat.connection_timeout),
        },
    );
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<ChatCommand>(32);
    let (evt_tx, mut evt_rx) = mpsc::channel::<ChatEvent>(256);
    let state = Arc::new(ArcSwap::from_pointee(ChatState::default()));

    let state_writer = state.clone();
    let evt_tx_task = evt_tx.clone();
    tokio::spawn(async move {
        let mut local = ChatState::default();
        let mut inflight: HashMap<ExchangeId, CancellationToken> = HashMap::new();
        loop {
            tokio::select! {
                cmd = cmd_rx.recv() => match cmd {
                    Some(cmd) => {
                        handle_command(cmd, &mut local, &client, &mut inflight, &evt_tx_task);
                    }
                    None => break,
                },
                evt = evt_rx.recv() => match evt {
                    Some(evt) => reduce(&mut local, evt),
                    None => break,
                },
            }
            while let Ok(evt) = evt_rx.try_recv() {
                reduce(&mut local, evt);
            }
            state_writer.store(Arc::new(local.clone()));
        }
    });

    let handle = ChatHandle {
        cmd_tx,
        state,
        next_id: Arc::new(AtomicU64::new(0)),
    };
    handle.refresh_models();
    handle
}
