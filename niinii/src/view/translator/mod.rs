pub mod chat;
pub mod realtime;
pub mod responses;

use std::collections::HashMap;
use std::sync::Arc;

use imgui::*;
use openai::ModelId;

use crate::{
    settings::{Settings, TranslatorType},
    translator::{
        self, Backend, ChatHandle, ExchangeId, ExchangeView, MsgId, RealtimeHandle, Response,
        ResponsesHandle, TranslateConfig, UsageView,
    },
    view::mixins::{ellipses, stroke_text_with_highlight},
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
/// for shared control + the concrete handle for backend-specific UI), the
/// currently displayed exchange id, and the window open flag. Acts as both the
/// controller (submit/cancel translations) and the view.
pub struct TranslatorWindow {
    backend: Arc<dyn Backend>,
    kind: BackendKind,
    current: Option<ExchangeId>,
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
            current: None,
            open: false,
        }
    }

    /// Cancel any in-flight translation and submit a new one. The new id
    /// becomes the "current" exchange rendered in the main UI.
    pub fn translate(&mut self, settings: &Settings, text: String) {
        if let Some(prev) = self.current {
            self.backend.cancel(prev);
        }
        let config = Arc::new(TranslateConfig::from_settings(settings));
        self.current = Some(self.backend.translate(text, config));
    }

    /// Forget the current exchange without cancelling it. Used when a new gloss
    /// arrives and the user has not opted into auto-translate.
    pub fn clear_current(&mut self) {
        self.current = None;
    }

    /// Render the usage bar for the current exchange, if any.
    pub fn draw_current_usage(&self, ui: &Ui) {
        let Some(id) = self.current else { return };
        if let Some(ex) = self.backend.exchange(id) {
            if let Some(usage) = &ex.usage {
                draw_usage(ui, &ex.model, usage);
            }
        }
    }

    /// Render the full current exchange, if any.
    pub fn draw_current_exchange(&self, ui: &Ui) {
        let Some(id) = self.current else { return };
        if let Some(ex) = self.backend.exchange(id) {
            draw_exchange(ui, &ex);
        }
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
pub(crate) fn draw_exchange(ui: &Ui, ex: &ExchangeView) {
    let _wrap_token = ui.push_text_wrap_pos_with_pos(0.0);
    let reasoning = ex.response.reasoning();
    if !reasoning.is_empty() {
        ui.tree_node_config("thinking").build(|| {
            ui.text_disabled(reasoning);
        });
    }
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
    match &ex.response {
        Response::Streaming { .. } => {
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
        Response::Errored(err) => {
            ui.same_line();
            stroke_text_with_highlight(
                ui,
                &draw_list,
                &format!("(error: {})", err),
                1.0,
                Some(StyleColor::PlotLinesHovered),
            );
        }
        Response::Cancelled => {
            ui.same_line();
            stroke_text_with_highlight(
                ui,
                &draw_list,
                "(cancelled)",
                1.0,
                Some(StyleColor::PlotLinesHovered),
            );
        }
        Response::Completed { .. } => {}
    }
}

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
