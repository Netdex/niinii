pub mod chat;
pub mod realtime;
pub mod responses;

use std::collections::HashMap;
use std::sync::Arc;

use imgui::*;
use openai::ModelId;

use crate::{
    renderer::context::{Context, TextStyle},
    settings::{Settings, TranslatorType},
    translator::{
        self, Backend, ChatHandle, ExchangeId, ExchangeView, MsgId, RealtimeHandle, Response,
        ResponsesHandle, TranslateConfig, UsageView,
    },
    view::mixins::{
        ellipses, push_earlier_line_highlight, stroke_text_with_highlight, with_scaled_font,
    },
};

/// Concrete backend handle for backend-specific window rendering. The
/// type-erased `Arc<dyn Backend>` on [`TranslatorWindow`] covers the shared
/// control/read surface; this enum is only consulted to render the backend's
/// own window (chat context editor vs responses chain).
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

/// Owns the active translator backend (both as a type-erased `Arc<dyn Backend>`
/// for shared control + the concrete handle for backend-specific UI) and the
/// window open flag. Acts as both the controller (submit/cancel translations)
/// and the view; which exchange to show is up to the caller.
pub struct TranslatorWindow {
    backend: Arc<dyn Backend>,
    kind: BackendKind,
    /// Most recently submitted exchange, cancelled when the next one starts.
    latest: Option<ExchangeId>,
    pub open: bool,
}

impl TranslatorWindow {
    pub fn new(settings: &Settings) -> Self {
        let (backend, kind): (Arc<dyn Backend>, BackendKind) = match settings.translator_type {
            TranslatorType::Chat => {
                let handle = translator::chat::spawn(settings);
                (
                    Arc::new(handle.clone()),
                    BackendKind::Chat {
                        handle,
                        buffers: HashMap::new(),
                    },
                )
            }
            TranslatorType::Responses => {
                let handle = translator::responses::spawn(settings);
                (Arc::new(handle.clone()), BackendKind::Responses { handle })
            }
            TranslatorType::Realtime => {
                let handle = translator::realtime::spawn(settings);
                (
                    Arc::new(handle.clone()),
                    BackendKind::Realtime {
                        handle,
                        instr_tokens: realtime::InstrTokens::default(),
                    },
                )
            }
        };
        Self {
            backend,
            kind,
            latest: None,
            open: false,
        }
    }

    /// Cancel any in-flight translation and submit a new one.
    pub fn translate(&mut self, settings: &Settings, text: String) -> ExchangeId {
        if let Some(prev) = self.latest {
            self.backend.cancel(prev);
        }
        let config = Arc::new(TranslateConfig::from_settings(settings));
        let id = self.backend.translate(text, config);
        self.latest = Some(id);
        id
    }

    /// Render the usage bar for an exchange.
    pub fn draw_usage(&self, ui: &Ui, id: ExchangeId) {
        if let Some(ex) = self.backend.exchange(id) {
            if let Some(usage) = &ex.usage {
                draw_usage(ui, &ex.model, usage);
            }
        }
    }

    /// Render an exchange's translation; `earlier_line` marks it as not
    /// belonging to the latest line.
    pub fn draw_exchange(&self, ui: &Ui, ctx: &Context, id: ExchangeId, earlier_line: bool) {
        if let Some(ex) = self.backend.exchange(id) {
            let _hl = earlier_line.then(|| push_earlier_line_highlight(ui));
            draw_exchange(ui, ctx, &ex);
        }
    }

    /// The translation so far (partial while streaming), or `None` if there
    /// is none to show.
    pub fn translation_text(&self, id: ExchangeId) -> Option<String> {
        let response = self.backend.exchange(id)?.response;
        Some(response.content().to_owned()).filter(|t| !t.is_empty())
    }

    /// Problems to report under the transcript: the exchange's own failure,
    /// and the backend's last error when it says something different.
    pub fn notes(&self, id: Option<ExchangeId>) -> Vec<String> {
        let mut notes = Vec::new();
        match id
            .and_then(|id| self.backend.exchange(id))
            .map(|ex| ex.response)
        {
            Some(Response::Errored(err)) => notes.push(format!("translation failed: {err}")),
            Some(Response::Cancelled) => notes.push("translation cancelled".to_owned()),
            _ => {}
        }
        if let Some(err) = self.backend.last_error() {
            if !notes.iter().any(|n| n.ends_with(&*err)) {
                notes.push(format!("translator: {err}"));
            }
        }
        notes
    }

    /// Type-erased handle to the active backend. Used to wire external producers
    /// (e.g. the VNDB integration) to the translator's prompt without caring
    /// which backend is active.
    pub fn backend(&self) -> Arc<dyn Backend> {
        self.backend.clone()
    }

    pub fn show_menu_item(&mut self, ui: &Ui) {
        if ui.menu_item("Translator") {
            self.open = true;
        }
    }

    pub fn ui(&mut self, ui: &Ui, settings: &mut Settings) {
        if !self.open {
            return;
        }
        let Some(_window) = ui
            .window("Translator")
            .size_constraints([600.0, 300.0], [1200.0, 1200.0])
            .opened(&mut self.open)
            .menu_bar(true)
            .begin()
        else {
            return;
        };
        if let Some(err) = self.backend.last_error() {
            ui.text_colored([1.0, 0.4, 0.4, 1.0], format!("error: {}", err));
        }
        match &mut self.kind {
            BackendKind::Chat { handle, buffers } => chat::window(ui, settings, handle, buffers),
            BackendKind::Responses { handle } => responses::window(ui, settings, handle),
            BackendKind::Realtime {
                handle,
                instr_tokens,
            } => realtime::window(ui, settings, handle, instr_tokens),
        }
    }
}

/// Render one exchange's assistant turn, streaming-aware. Shared by both
/// backends (operates on the backend-agnostic [`ExchangeView`]).
pub(crate) fn draw_exchange(ui: &Ui, ctx: &Context, ex: &ExchangeView) {
    let _wrap_token = ui.push_text_wrap_pos_with_pos(0.0);
    let reasoning = ex.response.reasoning();
    if !reasoning.is_empty() {
        ui.tree_node_config("thinking").build(|| {
            ui.text_disabled(reasoning);
        });
    }
    with_scaled_font(ui, ctx, TextStyle::Kanji, TRANSLATION_FONT_SCALE, || {
        ui.text(""); // anchor for line wrapping
        ui.same_line();
        let draw_list = ui.get_window_draw_list();
        let content = ex.response.content();
        if !content.is_empty() {
            ui.same_line();
            stroke_text_with_highlight(
                ui,
                &draw_list,
                content,
                1.0,
                Some(StyleColor::TextSelectedBg),
            );
        }
        // Failures are reported with the other notes under the transcript.
        if let Response::Streaming { .. } = &ex.response {
            if content.is_empty() {
                ui.same_line();
            } else {
                ui.same_line_with_spacing(0.0, 0.0);
            }
            stroke_text_with_highlight(
                ui,
                &draw_list,
                ellipses(ui),
                1.0,
                Some(StyleColor::TextSelectedBg),
            );
        }
    });
}

/// Translations are drawn with the gloss font scaled down to roughly 1.25x
/// the UI text size.
const TRANSLATION_FONT_SCALE: f32 = 0.6;

/// Render the usage progress bar for one exchange. `cached_tokens` is surfaced
/// so prompt-cache hits (a key latency optimization) are visible.
pub(crate) fn draw_usage(ui: &Ui, model: &ModelId, usage: &UsageView) {
    ui.same_line();
    ProgressBar::new(0.0)
        .overlay_text(format!(
            "{}: {} input ({} cached) + {} output ({} reasoning) = {}",
            model.as_ref(),
            usage.input_tokens,
            usage.cached_tokens,
            usage.output_tokens,
            usage.reasoning_tokens,
            usage.total_tokens,
        ))
        .size([500.0, 0.0])
        .build(ui);
}
