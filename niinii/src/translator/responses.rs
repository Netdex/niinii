//! Responses API backend for the translator runtime.
//!
//! Implements [`Backend`] for [`ResponsesHandle`]. Same command/event/state +
//! `ArcSwap` shape as the chat backend, but conversation state lives
//! server-side: each completed response's id is captured and sent as the next
//! turn's `previous_response_id`, so a turn only uploads the new user line plus
//! `instructions`. There is no local editable context buffer.
//!
//! Latency: streaming for low TTFT, `service_tier` / `reasoning.effort` /
//! `verbosity` from settings, a stable per-process `prompt_cache_key` to keep
//! the instructions prefix cache-hot, and an opt-in reasoning summary (off by
//! default since generating it costs latency).

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use arc_swap::ArcSwap;
use openai::{
    chat::ToolCallAccumulator,
    responses::{self, Input, InputItem, Reasoning, Request, StreamEvent, TextConfig},
    ConnectionPolicy, ModelId, Role,
};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;

use super::{Backend, ExchangeId, ExchangeView, Response, TranslateConfig, UsageView};
use crate::settings::Settings;

#[derive(Clone, Debug, Default)]
pub struct ResponsesState {
    pub exchanges: Vec<ExchangeView>,
    pub models: Vec<ModelId>,
    pub last_error: Option<Arc<str>>,
    /// Extra text appended to `instructions` at request-build time (e.g. the
    /// VNDB-derived character info). Pushed in via `set_system_addendum`.
    pub system_addendum: Option<Arc<str>>,
    /// Id of the last completed response. Sent as `previous_response_id` on the
    /// next turn so the server continues the same conversation. `None` starts a
    /// fresh chain.
    pub previous_response_id: Option<String>,
}

impl ResponsesState {
    pub fn exchange(&self, id: ExchangeId) -> Option<&ExchangeView> {
        self.exchanges.iter().find(|e| e.id == id)
    }
}

pub enum ResponsesCommand {
    Translate {
        id: ExchangeId,
        text: String,
        config: Arc<TranslateConfig>,
    },
    Cancel(ExchangeId),
    /// Start a fresh conversation: drop `previous_response_id` and clear the
    /// exchange readout.
    ResetChain,
    RefreshModels,
    SetSystemAddendum(Option<Arc<str>>),
}

pub enum ResponsesEvent {
    Started {
        id: ExchangeId,
        model: ModelId,
    },
    Delta {
        id: ExchangeId,
        content: String,
    },
    Completed {
        id: ExchangeId,
        /// Server id of the completed response; becomes the next
        /// `previous_response_id`. Empty if the server never reported one.
        response_id: String,
        usage: Option<UsageView>,
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

fn usage_view(usage: &responses::Usage) -> UsageView {
    UsageView {
        input_tokens: usage.input_tokens,
        cached_tokens: usage
            .input_tokens_details
            .as_ref()
            .map(|d| d.cached_tokens)
            .unwrap_or_default(),
        output_tokens: usage.output_tokens,
        reasoning_tokens: usage
            .output_tokens_details
            .as_ref()
            .map(|d| d.reasoning_tokens)
            .unwrap_or_default(),
        total_tokens: usage.total_tokens,
    }
}

fn build_request(
    state: &ResponsesState,
    config: &TranslateConfig,
    text: &str,
    prompt_cache_key: &str,
) -> Request {
    // Instructions are NOT inherited across `previous_response_id`, so the
    // system prompt + addendum are resent every turn (mirrors the chat backend's
    // single-system-message concatenation).
    let mut instructions = config.system_prompt.clone();
    if let Some(extra) = &state.system_addendum {
        if !extra.is_empty() {
            if !instructions.is_empty() {
                instructions.push_str("\n\n");
            }
            instructions.push_str(extra);
        }
    }
    // Anchor the latest line like the chat backend does.
    let user_line = format!("Translate this line:\n{}", text);

    let reasoning = config.reasoning_effort.map(|effort| Reasoning {
        effort: Some(effort),
        summary: None,
    });
    let text_cfg = config.verbosity.map(|v| TextConfig { verbosity: Some(v) });

    // Chaining off (option A): each turn is independent -- don't persist the
    // response and don't reference a prior one, so the conversation never grows.
    let (store, previous_response_id) = if config.chain {
        (true, state.previous_response_id.clone())
    } else {
        (false, None)
    };

    Request::builder()
        .model(config.model.clone())
        .input(Input::Items(vec![InputItem {
            role: Role::User,
            content: user_line,
        }]))
        .instructions(instructions)
        .store(store)
        .maybe_previous_response_id(previous_response_id)
        .maybe_max_output_tokens(config.max_tokens)
        .maybe_temperature(config.temperature)
        .maybe_top_p(config.top_p)
        .maybe_reasoning(reasoning)
        .maybe_text(text_cfg)
        .maybe_service_tier(config.service_tier)
        .prompt_cache_key(prompt_cache_key.to_string())
        .build()
}

fn handle_command(
    cmd: ResponsesCommand,
    state: &mut ResponsesState,
    client: &openai::Client,
    prompt_cache_key: &str,
    inflight: &mut HashMap<ExchangeId, CancellationToken>,
    evt_tx: &mpsc::Sender<ResponsesEvent>,
) {
    match cmd {
        ResponsesCommand::Translate { id, text, config } => {
            // Build against the current chain id before seeding the exchange.
            let request = build_request(state, &config, &text, prompt_cache_key);
            reduce(
                state,
                ResponsesEvent::Started {
                    id,
                    model: config.model.clone(),
                },
            );
            let cancel = CancellationToken::new();
            inflight.insert(id, cancel.clone());
            spawn_adapter(
                client.clone(),
                request,
                config.stream,
                config.chain,
                id,
                cancel,
                evt_tx.clone(),
            );
        }
        ResponsesCommand::Cancel(id) => {
            if let Some(tok) = inflight.remove(&id) {
                tok.cancel();
                reduce(state, ResponsesEvent::Cancelled { id });
            }
        }
        ResponsesCommand::ResetChain => {
            // Start a fresh conversation: drop the server-side chain id and clear
            // the readout so the table empties too.
            state.previous_response_id = None;
            state.exchanges.clear();
        }
        ResponsesCommand::SetSystemAddendum(text) => {
            state.system_addendum = text.filter(|s| !s.is_empty());
        }
        ResponsesCommand::RefreshModels => {
            let client = client.clone();
            let tx = evt_tx.clone();
            tokio::spawn(async move {
                match client.models().await {
                    Ok(mut models) => {
                        models.sort();
                        let _ = tx.send(ResponsesEvent::ModelsRefreshed(models)).await;
                    }
                    Err(err) => {
                        tracing::error!(?err, "failed to refresh models");
                        let _ = tx
                            .send(ResponsesEvent::Error(Arc::from(err.to_string())))
                            .await;
                    }
                }
            });
        }
    }
}

fn reduce(state: &mut ResponsesState, event: ResponsesEvent) {
    match event {
        ResponsesEvent::Started { id, model } => {
            state.exchanges.push(ExchangeView {
                id,
                model,
                response: Response::Streaming {
                    content: String::new(),
                    reasoning: String::new(),
                    tool_calls: ToolCallAccumulator::new(),
                },
                usage: None,
            });
        }
        ResponsesEvent::Delta { id, content } => {
            if let Some(ex) = find_mut(&mut state.exchanges, id) {
                if let Response::Streaming { content: acc, .. } = &mut ex.response {
                    acc.push_str(&content);
                }
            }
        }
        ResponsesEvent::Completed {
            id,
            response_id,
            usage,
        } => {
            let Some(ex) = find_mut(&mut state.exchanges, id) else {
                return;
            };
            let prior = std::mem::replace(&mut ex.response, Response::Cancelled);
            let (content, reasoning) = match prior {
                Response::Streaming {
                    content, reasoning, ..
                } => (content, reasoning),
                other => {
                    ex.response = other;
                    return;
                }
            };
            ex.response = Response::Completed {
                content,
                reasoning,
                tool_calls: Vec::new(),
            };
            ex.usage = usage;
            // Advance the chain only on success.
            if !response_id.is_empty() {
                state.previous_response_id = Some(response_id);
            }
        }
        ResponsesEvent::Failed { id, error } => {
            if let Some(ex) = find_mut(&mut state.exchanges, id) {
                ex.response = Response::Errored(error.clone());
            }
            state.last_error = Some(error);
        }
        ResponsesEvent::Cancelled { id } => {
            if let Some(ex) = find_mut(&mut state.exchanges, id) {
                if let Response::Streaming { .. } = ex.response {
                    ex.response = Response::Cancelled;
                }
            }
        }
        ResponsesEvent::ModelsRefreshed(models) => state.models = models,
        ResponsesEvent::Error(err) => state.last_error = Some(err),
    }
}

fn find_mut(exchanges: &mut [ExchangeView], id: ExchangeId) -> Option<&mut ExchangeView> {
    exchanges.iter_mut().find(|e| e.id == id)
}

fn spawn_adapter(
    client: openai::Client,
    request: Request,
    stream: bool,
    chain: bool,
    id: ExchangeId,
    cancel: CancellationToken,
    evt_tx: mpsc::Sender<ResponsesEvent>,
) {
    // A non-chained turn uses `store: false`, so the server-side response is not
    // retained and its id can't seed a later `previous_response_id`. Report an
    // empty id in that case so the chain is never advanced.
    let chain_id = move |id: String| if chain { id } else { String::new() };
    tokio::spawn(
        async move {
            if stream {
                let mut events = match client.stream_responses(request).await {
                    Ok(s) => s,
                    Err(err) => {
                        let _ = evt_tx
                            .send(ResponsesEvent::Failed {
                                id,
                                error: Arc::from(err.to_string()),
                            })
                            .await;
                        return;
                    }
                };
                let mut response_id = String::new();
                let mut usage = None;
                loop {
                    tokio::select! {
                        biased;
                        _ = cancel.cancelled() => {
                            let _ = evt_tx.send(ResponsesEvent::Cancelled { id }).await;
                            return;
                        }
                        event = events.next() => match event {
                            Some(Ok(ev)) => match ev {
                                StreamEvent::Created { response } => {
                                    response_id = response.id;
                                }
                                StreamEvent::OutputTextDelta { delta } => {
                                    let _ = evt_tx.send(ResponsesEvent::Delta {
                                        id,
                                        content: delta.replace('\n', ""),
                                    }).await;
                                }
                                // Reasoning summaries are not requested.
                                StreamEvent::ReasoningSummaryTextDelta { .. } => {}
                                StreamEvent::Completed { response }
                                | StreamEvent::Incomplete { response } => {
                                    usage = response.usage.as_ref().map(usage_view);
                                    let _ = evt_tx.send(ResponsesEvent::Completed {
                                        id, response_id: chain_id(response.id), usage,
                                    }).await;
                                    return;
                                }
                                StreamEvent::Failed { response } => {
                                    let error = response
                                        .error
                                        .map(|e| e.message)
                                        .unwrap_or_else(|| "response failed".to_string());
                                    let _ = evt_tx.send(ResponsesEvent::Failed {
                                        id, error: Arc::from(error),
                                    }).await;
                                    return;
                                }
                                StreamEvent::Error { message, .. } => {
                                    let _ = evt_tx.send(ResponsesEvent::Failed {
                                        id, error: Arc::from(message),
                                    }).await;
                                    return;
                                }
                                StreamEvent::Other => {}
                            }
                            Some(Err(err)) => {
                                let _ = evt_tx.send(ResponsesEvent::Failed {
                                    id, error: Arc::from(err.to_string()),
                                }).await;
                                return;
                            }
                            None => {
                                // Stream ended without a terminal event; finalize
                                // with whatever we accumulated.
                                let _ = evt_tx.send(ResponsesEvent::Completed {
                                    id, response_id: chain_id(response_id), usage,
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
                        let _ = evt_tx.send(ResponsesEvent::Cancelled { id }).await;
                    }
                    res = client.responses(request) => match res {
                        Ok(resp) => {
                            let content = resp.output_text();
                            let usage = resp.usage.as_ref().map(usage_view);
                            if !content.is_empty() {
                                let _ = evt_tx.send(ResponsesEvent::Delta { id, content }).await;
                            }
                            let _ = evt_tx.send(ResponsesEvent::Completed {
                                id, response_id: chain_id(resp.id), usage,
                            }).await;
                        }
                        Err(err) => {
                            let _ = evt_tx.send(ResponsesEvent::Failed {
                                id, error: Arc::from(err.to_string()),
                            }).await;
                        }
                    }
                }
            }
        }
        .instrument(tracing::Span::current()),
    );
}

/// Handle to the responses backend task. Cheap to clone. Mutations go through
/// `cmd_tx`; reads through `state` (wait-free snapshot).
#[derive(Clone)]
pub struct ResponsesHandle {
    cmd_tx: mpsc::Sender<ResponsesCommand>,
    state: Arc<ArcSwap<ResponsesState>>,
    next_id: Arc<AtomicU64>,
}

impl ResponsesHandle {
    pub fn state(&self) -> Arc<ResponsesState> {
        self.state.load_full()
    }
    fn send(&self, cmd: ResponsesCommand) {
        let _ = self.cmd_tx.try_send(cmd);
    }
    /// Start a fresh conversation (drop `previous_response_id`).
    pub fn reset_chain(&self) {
        self.send(ResponsesCommand::ResetChain);
    }
}

impl Backend for ResponsesHandle {
    fn translate(&self, text: String, config: Arc<TranslateConfig>) -> ExchangeId {
        let id = ExchangeId(self.next_id.fetch_add(1, Ordering::Relaxed));
        self.send(ResponsesCommand::Translate { id, text, config });
        id
    }
    fn cancel(&self, id: ExchangeId) {
        self.send(ResponsesCommand::Cancel(id));
    }
    fn clear(&self) {
        self.send(ResponsesCommand::ResetChain);
    }
    fn refresh_models(&self) {
        self.send(ResponsesCommand::RefreshModels);
    }
    fn set_system_addendum(&self, text: Option<Arc<str>>) {
        self.send(ResponsesCommand::SetSystemAddendum(text));
    }
    fn exchange(&self, id: ExchangeId) -> Option<ExchangeView> {
        self.state().exchange(id).cloned()
    }
    fn models(&self) -> Vec<ModelId> {
        self.state().models.clone()
    }
    fn last_error(&self) -> Option<Arc<str>> {
        self.state().last_error.clone()
    }
}

pub fn spawn(settings: &Settings) -> ResponsesHandle {
    let client = openai::Client::new(
        &settings.openai_api_key,
        &settings.openai_api_endpoint,
        ConnectionPolicy {
            timeout: Duration::from_millis(settings.responses.timeout),
            connect_timeout: Duration::from_millis(settings.responses.connection_timeout),
        },
    );
    // Stable per-process key so the instructions/system prefix stays cache-hot
    // across turns, lowering time-to-first-token.
    let prompt_cache_key = format!(
        "niinii-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<ResponsesCommand>(32);
    let (evt_tx, mut evt_rx) = mpsc::channel::<ResponsesEvent>(256);
    let state = Arc::new(ArcSwap::from_pointee(ResponsesState::default()));

    let state_writer = state.clone();
    tokio::spawn(async move {
        let mut local = ResponsesState::default();
        let mut inflight: HashMap<ExchangeId, CancellationToken> = HashMap::new();
        loop {
            tokio::select! {
                cmd = cmd_rx.recv() => match cmd {
                    Some(cmd) => {
                        handle_command(cmd, &mut local, &client, &prompt_cache_key, &mut inflight, &evt_tx);
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

    let handle = ResponsesHandle {
        cmd_tx,
        state,
        next_id: Arc::new(AtomicU64::new(0)),
    };
    handle.refresh_models();
    handle
}
