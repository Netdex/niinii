//! Realtime API (WebSocket) backend for the translator runtime.
//!
//! Implements [`Backend`] for [`RealtimeHandle`]. Same command/event/state +
//! `ArcSwap` shape as the other backends, but the Realtime API is a *stateful,
//! persistent* session: a dedicated connection task owns one WebSocket and the
//! server holds conversation state. This is unlike `chat`/`responses`, which
//! make a fresh stateless HTTP request per turn.
//!
//! Flow per translation: ensure a live connection, then `session.update`
//! (instructions = system prompt + VNDB addendum) -> `conversation.item.create`
//! (the user line) -> `response.create`, then stream `response.output_text`
//! deltas back. Translations are serialized by the UI (the prior is cancelled
//! before a new one is submitted), so at most one response is in flight.
//!
//! `chain` keeps the server-side conversation across turns; with it off, the
//! socket is reconnected per turn so context never accumulates. "Reset
//! conversation" likewise reconnects, dropping all server-side history.
//!
//! Latency (TTFT) is a first-class concern, so:
//! - the WebSocket is opened with TCP_NODELAY (no Nagle delay on the small
//!   client-event frames -- see `Client::connect_realtime`);
//! - when `chain` is on, the connection is kept open and reused across turns so
//!   only the first turn pays the TCP+TLS+upgrade handshake;
//! - `session.update` is re-sent only when the session config actually changes,
//!   avoiding redundant server-side work before generation and keeping the
//!   instructions prefix prompt-cache-hot.
//!
//! Requires a realtime-capable model (e.g. `gpt-realtime`) in the shared
//! `openai_model` setting; the Realtime API rejects non-realtime models.

use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use arc_swap::ArcSwap;
use openai::{
    chat::ToolCallAccumulator,
    realtime::{
        ClientEvent, Item, Realtime, RealtimeReasoning, RealtimeTruncation, RealtimeUsage,
        ServerEvent, SessionConfig,
    },
    ConnectionPolicy, ModelId, ReasoningEffort,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;

use super::{Backend, ExchangeId, ExchangeView, Response, TranslateConfig, UsageView};
use crate::settings::Settings;

#[derive(Clone, Debug, Default)]
pub struct RealtimeState {
    pub exchanges: Vec<ExchangeView>,
    pub models: Vec<ModelId>,
    pub last_error: Option<Arc<str>>,
    /// Extra text appended to the session instructions at turn-build time (e.g.
    /// the VNDB-derived character info). Pushed in via `set_system_addendum`.
    pub system_addendum: Option<Arc<str>>,
    /// Whether a WebSocket session is currently open. Surfaced in the view.
    pub connected: bool,
}

impl RealtimeState {
    pub fn exchange(&self, id: ExchangeId) -> Option<&ExchangeView> {
        self.exchanges.iter().find(|e| e.id == id)
    }
}

pub enum RealtimeCommand {
    Translate {
        id: ExchangeId,
        text: String,
        config: Arc<TranslateConfig>,
    },
    Cancel(ExchangeId),
    /// Drop the server-side conversation (reconnect) and clear the readout.
    ResetConversation,
    RefreshModels,
    SetSystemAddendum(Option<Arc<str>>),
}

/// Events emitted by the connection task (and the models refresh) into the
/// writer task, which reduces them into [`RealtimeState`].
enum RealtimeEvent {
    Started { id: ExchangeId, model: ModelId },
    Delta { id: ExchangeId, content: String },
    Completed { id: ExchangeId, usage: Option<UsageView> },
    Failed { id: ExchangeId, error: Arc<str> },
    Cancelled { id: ExchangeId },
    Connected,
    Disconnected,
    ModelsRefreshed(Vec<ModelId>),
    Error(Arc<str>),
}

/// Commands sent from the writer task to the connection task that owns the
/// socket. Translation knobs are flattened here so the connection task never
/// touches `Settings` or `TranslateConfig`.
enum ConnCommand {
    Translate {
        id: ExchangeId,
        model: ModelId,
        instructions: String,
        user_text: String,
        reasoning_effort: Option<ReasoningEffort>,
        max_tokens: Option<u32>,
        truncation: Option<RealtimeTruncation>,
    },
    Cancel(ExchangeId),
}

fn usage_view(usage: &RealtimeUsage) -> UsageView {
    UsageView {
        input_tokens: usage.input_tokens,
        cached_tokens: usage
            .input_token_details
            .as_ref()
            .map(|d| d.cached_tokens)
            .unwrap_or_default(),
        output_tokens: usage.output_tokens,
        // The Realtime API has no reasoning-token accounting.
        reasoning_tokens: 0,
        total_tokens: usage.total_tokens,
    }
}

/// Build the session instructions: system prompt + optional VNDB addendum,
/// concatenated the same way the other backends build the system message.
fn build_instructions(state: &RealtimeState, config: &TranslateConfig) -> String {
    let mut instructions = config.system_prompt.clone();
    if let Some(extra) = &state.system_addendum {
        if !extra.is_empty() {
            if !instructions.is_empty() {
                instructions.push_str("\n\n");
            }
            instructions.push_str(extra);
        }
    }
    instructions
}

/// Send one full turn on an open connection: (re)configure the session if it
/// changed, append the user message, and request a response. Skipping a
/// redundant `session.update` lowers TTFT and keeps the instructions prefix
/// cache-hot.
async fn send_turn(
    conn: &mut Realtime,
    session: SessionConfig,
    last_session: &mut Option<SessionConfig>,
    user_text: String,
) -> Result<(), openai::Error> {
    // Coalesce the turn's events into a single flush so item.create and
    // response.create reach the server together (generation starts on one read).
    let mut events: Vec<ClientEvent> = Vec::with_capacity(3);
    if last_session.as_ref() != Some(&session) {
        events.push(ClientEvent::SessionUpdate {
            session: session.clone(),
        });
        *last_session = Some(session);
    }
    events.push(ClientEvent::ConversationItemCreate {
        item: Item::user_text(user_text),
    });
    events.push(ClientEvent::ResponseCreate { response: None });
    conn.send_all(&events).await
}

/// The response currently being driven on a connection. `response_id` is `None`
/// between sending `response.create` and receiving `response.created`; once the
/// server assigns an id, all subsequent events for the turn are routed by it.
///
/// Routing by id (rather than by occupancy) is what keeps an interrupted turn
/// correct: when the prior response is cancelled and a new one starts on the
/// same connection, the cancelled response's late `response.done` carries the
/// old id and must not be attributed to the new exchange.
struct Active {
    exchange: ExchangeId,
    response_id: Option<String>,
}

impl Active {
    fn is_response(&self, response_id: &str) -> bool {
        self.response_id.as_deref() == Some(response_id)
    }
}

/// Translate one server event into a `RealtimeEvent`, routing it to the in-flight
/// exchange (`active`) by the server's response id.
async fn on_server_event(
    ev: ServerEvent,
    active: &mut Option<Active>,
    evt_tx: &mpsc::Sender<RealtimeEvent>,
) {
    match ev {
        ServerEvent::ResponseCreated { response } => {
            // Bind the server's response id to the exchange we just started so
            // later events for this turn are routed by it. A `response.created`
            // for an already-finished (e.g. cancelled) response has no waiting
            // exchange and is ignored.
            if let Some(a) = active.as_mut() {
                if a.response_id.is_none() {
                    a.response_id = Some(response.id);
                }
            }
        }
        ServerEvent::OutputTextDelta { response_id, delta } => {
            if let Some(a) = active.as_ref().filter(|a| a.is_response(&response_id)) {
                let _ = evt_tx
                    .send(RealtimeEvent::Delta {
                        id: a.exchange,
                        content: delta.replace('\n', ""),
                    })
                    .await;
            }
        }
        // Text is accumulated from deltas; the `done` event would duplicate it.
        ServerEvent::OutputTextDone { .. } => {}
        ServerEvent::ResponseDone { response } => {
            // Only the done for the response we're currently streaming counts. A
            // stale done (a just-cancelled prior response, whose cancellation we
            // already surfaced) carries a different id and must not be attributed
            // to the next exchange.
            if !active.as_ref().is_some_and(|a| a.is_response(&response.id)) {
                return;
            }
            let id = active.take().unwrap().exchange;
            match response.status.as_deref() {
                Some("cancelled") => {
                    let _ = evt_tx.send(RealtimeEvent::Cancelled { id }).await;
                }
                Some("failed") | Some("incomplete") => {
                    let msg = response
                        .status_details
                        .and_then(|d| d.error)
                        .map(|e| e.message)
                        .unwrap_or_else(|| "response failed".to_string());
                    let _ = evt_tx
                        .send(RealtimeEvent::Failed {
                            id,
                            error: Arc::from(msg),
                        })
                        .await;
                }
                _ => {
                    let usage = response.usage.as_ref().map(usage_view);
                    let _ = evt_tx.send(RealtimeEvent::Completed { id, usage }).await;
                }
            }
        }
        ServerEvent::Error { error } => {
            // An error mid-response fails the in-flight turn; otherwise it is a
            // session-level error.
            if let Some(a) = active.take() {
                let _ = evt_tx
                    .send(RealtimeEvent::Failed {
                        id: a.exchange,
                        error: Arc::from(error.message),
                    })
                    .await;
            } else {
                let _ = evt_tx
                    .send(RealtimeEvent::Error(Arc::from(error.message)))
                    .await;
            }
        }
        ServerEvent::Other => {}
    }
}

/// Owns one WebSocket for its lifetime. Connects, then multiplexes outbound
/// commands (from the writer) against inbound server events. Returns when
/// cancelled, when the command channel closes, or when the socket drops -- the
/// writer reconnects on the next translation.
async fn run_connection(
    client: openai::Client,
    model: ModelId,
    connect_timeout: Duration,
    mut cmd_rx: mpsc::Receiver<ConnCommand>,
    evt_tx: mpsc::Sender<RealtimeEvent>,
    cancel: CancellationToken,
) {
    let connect = client.connect_realtime(&model);
    let conn = tokio::select! {
        biased;
        _ = cancel.cancelled() => return,
        res = tokio::time::timeout(connect_timeout, connect) => match res {
            Ok(Ok(conn)) => conn,
            Ok(Err(err)) => {
                fail_pending(&mut cmd_rx, &evt_tx, Arc::from(err.to_string()), &cancel).await;
                return;
            }
            Err(_) => {
                fail_pending(&mut cmd_rx, &evt_tx, Arc::from("realtime connect timed out"), &cancel).await;
                return;
            }
        },
    };
    let _ = evt_tx.send(RealtimeEvent::Connected).await;

    let mut conn = conn;
    let mut active: Option<Active> = None;
    // Last session config pushed on this connection; lets us skip a redundant
    // session.update when nothing changed.
    let mut last_session: Option<SessionConfig> = None;
    // Keep the idle connection warm across reading pauses so a turn after a gap
    // does not pay a reconnect. The first tick is deferred so connect + first
    // turn aren't preceded by a stray ping.
    let keepalive_period = Duration::from_secs(20);
    let mut keepalive = tokio::time::interval_at(
        tokio::time::Instant::now() + keepalive_period,
        keepalive_period,
    );
    keepalive.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                let _ = conn.close().await;
                return;
            }
            cmd = cmd_rx.recv() => match cmd {
                Some(ConnCommand::Translate { id, model, instructions, user_text, reasoning_effort, max_tokens, truncation }) => {
                    active = Some(Active { exchange: id, response_id: None });
                    let _ = evt_tx.send(RealtimeEvent::Started { id, model }).await;
                    let mut session = SessionConfig::text(instructions);
                    session.max_output_tokens = max_tokens;
                    session.truncation = truncation;
                    // Realtime reasoning effort is one of minimal/low/medium/high/
                    // xhigh -- there is no "none" (that exists on the shared enum
                    // only for Chat/Responses). Treat `None` as "omit reasoning".
                    session.reasoning = reasoning_effort
                        .filter(|e| *e != ReasoningEffort::None)
                        .map(|effort| RealtimeReasoning {
                            effort: Some(effort),
                        });
                    if let Err(err) = send_turn(&mut conn, session, &mut last_session, user_text).await {
                        let _ = evt_tx.send(RealtimeEvent::Failed { id, error: Arc::from(err.to_string()) }).await;
                        let _ = evt_tx.send(RealtimeEvent::Disconnected).await;
                        return;
                    }
                }
                Some(ConnCommand::Cancel(id)) => {
                    // Only cancel if this exchange is actually in flight. The UI
                    // cancels the previous exchange before each new translation,
                    // but that exchange has usually already completed -- sending
                    // `response.cancel` then would draw a spurious "no active
                    // response found" error that the server attributes to the
                    // next response. Clearing `active` here also detaches the
                    // cancelled turn so its trailing deltas/`response.done` (which
                    // carry the old id) are dropped rather than landing on the
                    // next exchange.
                    if active.as_ref().is_some_and(|a| a.exchange == id) {
                        let _ = conn.send(&ClientEvent::ResponseCancel).await;
                        active = None;
                        let _ = evt_tx.send(RealtimeEvent::Cancelled { id }).await;
                    }
                }
                None => {
                    let _ = conn.close().await;
                    return;
                }
            },
            event = conn.next_event() => match event {
                Some(Ok(ev)) => on_server_event(ev, &mut active, &evt_tx).await,
                Some(Err(err)) => {
                    if let Some(a) = active.take() {
                        let _ = evt_tx.send(RealtimeEvent::Failed { id: a.exchange, error: Arc::from(err.to_string()) }).await;
                    }
                    let _ = evt_tx.send(RealtimeEvent::Disconnected).await;
                    return;
                }
                None => {
                    if let Some(a) = active.take() {
                        let _ = evt_tx.send(RealtimeEvent::Failed { id: a.exchange, error: Arc::from("connection closed") }).await;
                    }
                    let _ = evt_tx.send(RealtimeEvent::Disconnected).await;
                    return;
                }
            },
            _ = keepalive.tick() => {
                // A failed ping means the socket is gone; tear down so the next
                // translation cleanly reconnects rather than failing a line.
                if let Err(err) = conn.ping().await {
                    tracing::debug!(?err, "realtime keepalive ping failed; disconnecting");
                    let _ = evt_tx.send(RealtimeEvent::Disconnected).await;
                    return;
                }
            }
        }
    }
}

/// Connection failed before opening: report the error and fail any queued
/// translate commands until the channel closes or we are cancelled.
async fn fail_pending(
    cmd_rx: &mut mpsc::Receiver<ConnCommand>,
    evt_tx: &mpsc::Sender<RealtimeEvent>,
    error: Arc<str>,
    cancel: &CancellationToken,
) {
    let _ = evt_tx.send(RealtimeEvent::Error(error.clone())).await;
    let _ = evt_tx.send(RealtimeEvent::Disconnected).await;
    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => return,
            cmd = cmd_rx.recv() => match cmd {
                Some(ConnCommand::Translate { id, .. }) => {
                    let _ = evt_tx.send(RealtimeEvent::Failed { id, error: error.clone() }).await;
                }
                Some(ConnCommand::Cancel(_)) => {}
                None => return,
            },
        }
    }
}

/// A live connection task: the channel to send it commands, and the token that
/// shuts it down.
struct Conn {
    tx: mpsc::Sender<ConnCommand>,
    cancel: CancellationToken,
}

impl Conn {
    fn shutdown(self) {
        self.cancel.cancel();
    }
}

/// Writer-side connection manager. Holds the active connection (if any) and the
/// model it was opened with so a model change forces a reconnect.
struct ConnManager {
    client: openai::Client,
    connect_timeout: Duration,
    evt_tx: mpsc::Sender<RealtimeEvent>,
    conn: Option<Conn>,
    model: Option<ModelId>,
}

impl ConnManager {
    fn spawn_conn(&mut self, model: ModelId) {
        let (tx, rx) = mpsc::channel::<ConnCommand>(32);
        let cancel = CancellationToken::new();
        let client = self.client.clone();
        let evt_tx = self.evt_tx.clone();
        let connect_timeout = self.connect_timeout;
        let child = cancel.clone();
        tokio::spawn(
            run_connection(client, model.clone(), connect_timeout, rx, evt_tx, child)
                .instrument(tracing::Span::current()),
        );
        self.conn = Some(Conn { tx, cancel });
        self.model = Some(model);
    }

    /// Drop the current connection (server-side state goes with it).
    fn reset(&mut self) {
        if let Some(conn) = self.conn.take() {
            conn.shutdown();
        }
        self.model = None;
    }

    /// Submit a translation, (re)connecting if needed. A model change or
    /// `chain == false` forces a fresh connection so server-side history is not
    /// carried.
    fn translate(&mut self, cmd: ConnCommand, model: ModelId, chain: bool) {
        // A closed channel means the connection task has exited (failed connect,
        // socket error, or graceful close) -- reconnect. A model change or
        // `chain == false` also forces a fresh connection.
        let dead = self.conn.as_ref().is_none_or(|c| c.tx.is_closed());
        let stale = self.model.as_ref() != Some(&model);
        if dead || stale || !chain {
            self.reset();
            self.spawn_conn(model);
        }
        if let Some(conn) = &self.conn {
            if conn.tx.try_send(cmd).is_err() {
                tracing::error!("realtime connection command queue full or closed");
            }
        }
    }

    fn cancel(&self, id: ExchangeId) {
        if let Some(conn) = &self.conn {
            let _ = conn.tx.try_send(ConnCommand::Cancel(id));
        }
    }
}

fn handle_command(
    cmd: RealtimeCommand,
    state: &mut RealtimeState,
    mgr: &mut ConnManager,
    client: &openai::Client,
    evt_tx: &mpsc::Sender<RealtimeEvent>,
) {
    match cmd {
        RealtimeCommand::Translate { id, text, config } => {
            let instructions = build_instructions(state, &config);
            let user_text = format!("Translate this line:\n{}", text);
            mgr.translate(
                ConnCommand::Translate {
                    id,
                    model: config.model.clone(),
                    instructions,
                    user_text,
                    reasoning_effort: config.reasoning_effort,
                    max_tokens: config.max_tokens,
                    truncation: config.truncation,
                },
                config.model.clone(),
                config.chain,
            );
        }
        RealtimeCommand::Cancel(id) => mgr.cancel(id),
        RealtimeCommand::ResetConversation => {
            mgr.reset();
            state.exchanges.clear();
            state.connected = false;
        }
        RealtimeCommand::SetSystemAddendum(text) => {
            state.system_addendum = text.filter(|s| !s.is_empty());
        }
        RealtimeCommand::RefreshModels => {
            let client = client.clone();
            let tx = evt_tx.clone();
            tokio::spawn(async move {
                match client.models().await {
                    Ok(mut models) => {
                        models.sort();
                        let _ = tx.send(RealtimeEvent::ModelsRefreshed(models)).await;
                    }
                    Err(err) => {
                        tracing::error!(?err, "failed to refresh models");
                        let _ = tx.send(RealtimeEvent::Error(Arc::from(err.to_string()))).await;
                    }
                }
            });
        }
    }
}

fn reduce(state: &mut RealtimeState, event: RealtimeEvent) {
    match event {
        RealtimeEvent::Started { id, model } => {
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
        RealtimeEvent::Delta { id, content } => {
            if let Some(ex) = find_mut(&mut state.exchanges, id) {
                if let Response::Streaming { content: acc, .. } = &mut ex.response {
                    acc.push_str(&content);
                }
            }
        }
        RealtimeEvent::Completed { id, usage } => {
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
        }
        RealtimeEvent::Failed { id, error } => {
            if let Some(ex) = find_mut(&mut state.exchanges, id) {
                ex.response = Response::Errored(error.clone());
            }
            state.last_error = Some(error);
        }
        RealtimeEvent::Cancelled { id } => {
            if let Some(ex) = find_mut(&mut state.exchanges, id) {
                if let Response::Streaming { .. } = ex.response {
                    ex.response = Response::Cancelled;
                }
            }
        }
        RealtimeEvent::Connected => state.connected = true,
        RealtimeEvent::Disconnected => state.connected = false,
        RealtimeEvent::ModelsRefreshed(models) => state.models = models,
        RealtimeEvent::Error(err) => state.last_error = Some(err),
    }
}

fn find_mut(exchanges: &mut [ExchangeView], id: ExchangeId) -> Option<&mut ExchangeView> {
    exchanges.iter_mut().find(|e| e.id == id)
}

/// Handle to the realtime backend task. Cheap to clone. Mutations go through
/// `cmd_tx`; reads through `state` (wait-free snapshot).
#[derive(Clone)]
pub struct RealtimeHandle {
    cmd_tx: mpsc::Sender<RealtimeCommand>,
    state: Arc<ArcSwap<RealtimeState>>,
    next_id: Arc<AtomicU64>,
}

impl RealtimeHandle {
    pub fn state(&self) -> Arc<RealtimeState> {
        self.state.load_full()
    }
    fn send(&self, cmd: RealtimeCommand) {
        let _ = self.cmd_tx.try_send(cmd);
    }
    /// Drop the server-side conversation and reconnect on the next turn.
    pub fn reset_conversation(&self) {
        self.send(RealtimeCommand::ResetConversation);
    }
}

impl Backend for RealtimeHandle {
    fn translate(&self, text: String, config: Arc<TranslateConfig>) -> ExchangeId {
        let id = ExchangeId(self.next_id.fetch_add(1, Ordering::Relaxed));
        self.send(RealtimeCommand::Translate { id, text, config });
        id
    }
    fn cancel(&self, id: ExchangeId) {
        self.send(RealtimeCommand::Cancel(id));
    }
    fn clear(&self) {
        self.send(RealtimeCommand::ResetConversation);
    }
    fn refresh_models(&self) {
        self.send(RealtimeCommand::RefreshModels);
    }
    fn set_system_addendum(&self, text: Option<Arc<str>>) {
        self.send(RealtimeCommand::SetSystemAddendum(text));
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

pub fn spawn(settings: &Settings) -> RealtimeHandle {
    let client = openai::Client::new(
        &settings.openai_api_key,
        &settings.openai_api_endpoint,
        ConnectionPolicy {
            timeout: Duration::from_millis(settings.realtime.timeout),
            connect_timeout: Duration::from_millis(settings.realtime.connection_timeout),
        },
    );
    let connect_timeout = Duration::from_millis(settings.realtime.connection_timeout);

    let (cmd_tx, mut cmd_rx) = mpsc::channel::<RealtimeCommand>(32);
    let (evt_tx, mut evt_rx) = mpsc::channel::<RealtimeEvent>(256);
    let state = Arc::new(ArcSwap::from_pointee(RealtimeState::default()));

    let state_writer = state.clone();
    let writer_client = client.clone();
    let writer_evt_tx = evt_tx.clone();
    tokio::spawn(async move {
        let mut local = RealtimeState::default();
        let mut mgr = ConnManager {
            client: writer_client.clone(),
            connect_timeout,
            evt_tx: writer_evt_tx.clone(),
            conn: None,
            model: None,
        };
        loop {
            tokio::select! {
                cmd = cmd_rx.recv() => match cmd {
                    Some(cmd) => {
                        handle_command(cmd, &mut local, &mut mgr, &writer_client, &writer_evt_tx);
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

    let handle = RealtimeHandle {
        cmd_tx,
        state,
        next_id: Arc::new(AtomicU64::new(0)),
    };
    handle.refresh_models();
    handle
}
