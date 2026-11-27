use dear_imgui_reflect::ReflectSession;
pub mod chat;
mod history;
pub mod realtime;
pub mod responses;

use std::collections::HashMap;
use std::sync::Arc;

use crate::gui::px2;
use dear_imgui_rs::*;
use openai::ModelId;

use crate::{
    gui::FontSize,
    settings::{Settings, TranslatorType},
    translator::{
        self, Backend, ChatHandle, ExchangeId, ExchangeView, MsgId, RealtimeHandle, Response,
        ResponsesHandle, TranslateConfig, UsageView,
    },
    view::{
        mixins::{ellipses, push_earlier_line_highlight, wrapped_field_height, Flow},
        panel::Panel,
    },
};

/// The active backend's handle, with the view state of its window.
enum BackendKind {
    Chat {
        handle: ChatHandle,
        /// Per-message edit buffers for the context editor, keyed by stable
        /// `MsgId`.
        buffers: HashMap<MsgId, String>,
    },
    Responses {
        handle: ResponsesHandle,
    },
    Realtime {
        handle: RealtimeHandle,
        /// Memoized token counts for the instruction blocks (system prompt +
        /// VNDB addendum) so tiktoken runs only when the text changes.
        instr_tokens: realtime::InstrTokens,
    },
}

impl BackendKind {
    fn backend(&self) -> &dyn Backend {
        match self {
            BackendKind::Chat { handle, .. } => handle,
            BackendKind::Responses { handle } => handle,
            BackendKind::Realtime { handle, .. } => handle,
        }
    }
}

/// Owns the active translator backend. Acts as both the controller
/// (submit/cancel translations) and the view; which exchange to show is up to
/// the caller.
pub struct TranslatorWindow {
    kind: BackendKind,
    /// Most recently submitted exchange, cancelled when the next one starts.
    latest: Option<ExchangeId>,
}

impl TranslatorWindow {
    pub fn new(settings: &Settings) -> Self {
        let kind = match settings.translation.translator_type {
            TranslatorType::Chat => BackendKind::Chat {
                handle: translator::chat::spawn(settings),
                buffers: HashMap::new(),
            },
            TranslatorType::Responses => BackendKind::Responses {
                handle: translator::responses::spawn(settings),
            },
            TranslatorType::Realtime => BackendKind::Realtime {
                handle: translator::realtime::spawn(settings),
                instr_tokens: realtime::InstrTokens::default(),
            },
        };
        Self { kind, latest: None }
    }

    /// Cancel any in-flight translation and submit a new one.
    pub fn translate(&mut self, settings: &Settings, text: String) -> ExchangeId {
        if let Some(prev) = self.latest {
            self.kind.backend().cancel(prev);
        }
        let config = Arc::new(TranslateConfig::from_settings(settings));
        let id = self.kind.backend().translate(text, config);
        self.latest = Some(id);
        id
    }

    /// Render the usage bar for an exchange.
    pub fn draw_usage(&self, ui: &Ui, id: ExchangeId) {
        let Some(ex) = self.kind.backend().exchange(id) else {
            return;
        };
        if let Some(usage) = &ex.usage {
            ui.same_line();
            ui.text_disabled(usage_text(&ex.model, usage));
        }
    }

    /// Render an exchange's translation; `earlier_line` marks it as not
    /// belonging to the latest line.
    pub fn draw_exchange(&self, ui: &Ui, id: ExchangeId, earlier_line: bool) {
        if let Some(ex) = self.kind.backend().exchange(id) {
            let _hl = earlier_line.then(|| push_earlier_line_highlight(ui));
            draw_exchange(ui, &ex);
        }
    }

    /// The translation so far (partial while streaming), or `None` if there
    /// is none to show.
    pub fn translation_text(&self, id: ExchangeId) -> Option<String> {
        let response = self.kind.backend().exchange(id)?.response;
        Some(response.content().to_owned()).filter(|t| !t.is_empty())
    }

    /// Problems to report under the transcript: what went wrong with the
    /// exchange, and the backend's last error.
    pub fn notes(&self, id: Option<ExchangeId>) -> Vec<String> {
        let backend = self.kind.backend();
        let exchange = id
            .and_then(|id| backend.exchange(id))
            .and_then(|ex| ex.response.problem());
        let backend = backend.last_error().map(|err| format!("translator: {err}"));
        exchange.into_iter().chain(backend).collect()
    }

    /// Type-erased handle to the active backend. Used to wire external producers
    /// (e.g. the VNDB integration) to the translator's prompt without caring
    /// which backend is active.
    pub fn backend(&self) -> Arc<dyn Backend> {
        match &self.kind {
            BackendKind::Chat { handle, .. } => Arc::new(handle.clone()),
            BackendKind::Responses { handle } => Arc::new(handle.clone()),
            BackendKind::Realtime { handle, .. } => Arc::new(handle.clone()),
        }
    }

    pub fn show(
        &mut self,
        panel: &mut Panel,
        ui: &Ui,
        reflect: &ReflectSession,
        settings: &mut Settings,
    ) {
        panel.show(
            ui,
            |w| {
                w.size_constraints(px2(ui, [600.0, 300.0]), [f32::MAX, f32::MAX])
                    .flags(WindowFlags::MENU_BAR)
            },
            || match &mut self.kind {
                BackendKind::Chat { handle, buffers } => {
                    chat::window(ui, reflect, settings, handle, buffers)
                }
                BackendKind::Responses { handle } => {
                    responses::window(ui, reflect, settings, handle)
                }
                BackendKind::Realtime {
                    handle,
                    instr_tokens,
                } => realtime::window(ui, reflect, settings, handle, instr_tokens),
            },
        );
    }
}

/// Render one exchange's assistant turn, streaming-aware. Shared by both
/// backends (operates on the backend-agnostic [`ExchangeView`]).
pub(crate) fn draw_exchange(ui: &Ui, ex: &ExchangeView) {
    let _wrap_token = ui.push_text_wrap_pos(0.0);
    let reasoning = ex.response.reasoning();
    if !reasoning.is_empty() {
        if let Some(_node) = ui.tree_node_config("thinking").push() {
            ui.text_disabled(reasoning);
        }
    }
    TRANSLATION_SIZE.with(ui, || {
        let draw_list = ui.get_window_draw_list();
        let highlight = Some(StyleColor::TextSelectedBg);
        let mut flow = Flow::new(ui);
        flow.stroke_text(ui, &draw_list, ex.response.content(), 1.0, highlight);
        // Failures are reported with the other notes under the transcript.
        if let Response::Streaming { .. } = &ex.response {
            flow.stroke_text(ui, &draw_list, ellipses(ui), 1.0, highlight);
        }
    });
}

const TRANSLATION_SIZE: FontSize = FontSize::BODY.scaled(1.25);

/// One exchange's token usage. `cached_tokens` is shown so prompt-cache hits
/// (a key latency optimization) are visible.
fn usage_text(model: &ModelId, usage: &UsageView) -> String {
    format!(
        "{}: {} input ({} cached) + {} output ({} reasoning) = {}",
        model.as_ref(),
        usage.input_tokens,
        usage.cached_tokens,
        usage.output_tokens,
        usage.reasoning_tokens,
        usage.total_tokens,
    )
}

/// Text the user can scroll and copy but not edit, as tall as its wrapped
/// contents up to `max_height`.
fn read_only_text(ui: &Ui, id: &str, text: &str, max_height: f32) {
    let height = wrapped_field_height(ui, text, max_height);
    let mut buf = text.to_owned();
    ui.input_text_multiline(id, &mut buf, [ui.content_region_avail()[0], height])
        .flags(InputTextMultilineFlags::WORD_WRAP)
        .read_only(true)
        .build();
}

/// An editable instruction or message, word-wrapped.
fn wrapped_editor(ui: &Ui, id: &str, text: &mut String, height: f32) -> bool {
    ui.input_text_multiline(id, text, [ui.content_region_avail()[0], height])
        .flags(InputTextMultilineFlags::WORD_WRAP)
        .build()
}
