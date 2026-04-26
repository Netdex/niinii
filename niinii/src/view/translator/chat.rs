use std::collections::HashMap;
use std::sync::Arc;

use imgui::*;
use openai::chat::{Message, Role, Usage};
use openai::ModelId;

use crate::{
    settings::{Settings, TranslatorType},
    translator::chat::{
        self, ChatHandle, ContextEdit, ExchangeId, ExchangeView, MsgId, Response, TranslateConfig,
    },
    view::mixins::{
        checkbox_option, checkbox_option_with_default, combo_enum, combo_list, drag_handle,
        ellipses, help_marker, stroke_text_with_highlight,
    },
};

/// Owns the translator backend handle, the currently displayed exchange id,
/// and the per-message edit buffers for the context editor. Acts as both the
/// controller (submit/cancel translations) and the view (render the window
/// plus exchange readouts embedded in the main UI).
pub struct TranslatorWindow {
    translator: ChatHandle,
    current: Option<ExchangeId>,
    buffers: HashMap<MsgId, String>,
    pub open: bool,
}

impl TranslatorWindow {
    pub fn new(settings: &Settings) -> Self {
        let translator = match settings.translator_type {
            TranslatorType::Chat => chat::spawn(settings),
        };
        Self {
            translator,
            current: None,
            buffers: HashMap::new(),
            open: false,
        }
    }

    /// Cancel any in-flight translation and submit a new one. The new id
    /// becomes the "current" exchange rendered in the main UI.
    pub fn translate(&mut self, settings: &Settings, text: String) {
        if let Some(prev) = self.current {
            self.translator.cancel(prev);
        }
        let config = Arc::new(TranslateConfig::from_settings(settings));
        self.current = Some(self.translator.translate(text, config));
    }

    /// Forget the current exchange without cancelling it. Used when a new
    /// gloss arrives and the user has not opted into auto-translate.
    pub fn clear_current(&mut self) {
        self.current = None;
    }

    /// Render the usage bar for the current exchange, if any.
    pub fn draw_current_usage(&self, ui: &Ui) {
        let Some(id) = self.current else { return };
        let state = self.translator.state();
        if let Some(ex) = state.exchange(id) {
            if let Some(usage) = &ex.usage {
                draw_usage(ui, &ex.model, usage);
            }
        }
    }

    /// Render the full current exchange, if any.
    pub fn draw_current_exchange(&self, ui: &Ui) {
        let Some(id) = self.current else { return };
        let state = self.translator.state();
        if let Some(ex) = state.exchange(id) {
            draw_exchange(ui, ex);
        }
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
        let handle = &self.translator;
        let state = handle.state();
        let chatgpt = &mut settings.chat;

        ui.menu_bar(|| {
            if ui.menu_item("Clear") {
                handle.clear_context();
            }
        });

        if ui.collapsing_header("Tuning", TreeNodeFlags::DEFAULT_OPEN) {
            let align = 10.0;
            if let Some(_token) = ui.begin_table("##", 2) {
                ui.table_next_column();
                ui.set_next_item_width(ui.current_font_size() * -align);
                combo_list(ui, "Model", &state.models, &mut chatgpt.model);
                ui.table_next_column();
                ui.checkbox("Stream", &mut chatgpt.stream);
                ui.same_line();
                help_marker(
                    ui,
                    "Use streaming API (may require ID verification for some models)",
                );
                ui.table_next_column();
                ui.set_next_item_width(ui.current_font_size() * -align);
                ui.input_scalar_n("Context tokens", &mut chatgpt.max_context_tokens)
                    .build();
                ui.same_line();
                help_marker(
                    ui,
                    "Target/threshold. For limiting context size while optimizing usage of prefix caches",
                );
                ui.table_next_column();
                checkbox_option(ui, &mut chatgpt.max_tokens, |ui, max_tokens| {
                    ui.set_next_item_width(ui.current_font_size() * -align);
                    ui.input_scalar("Max response tokens", max_tokens).build();
                });
                ui.table_next_column();
                checkbox_option_with_default(
                    ui,
                    &mut chatgpt.temperature,
                    1.0,
                    |ui, temperature| {
                        ui.set_next_item_width(ui.current_font_size() * -align);
                        ui.slider_config("Temperature", 0.0f32, 2.0f32)
                            .display_format("%.2f")
                            .flags(SliderFlags::ALWAYS_CLAMP)
                            .build(temperature);
                    },
                );
                ui.table_next_column();
                checkbox_option_with_default(ui, &mut chatgpt.top_p, 1.0, |ui, top_p| {
                    ui.set_next_item_width(ui.current_font_size() * -align);
                    ui.slider_config("Top P", 0.0f32, 1.0f32)
                        .display_format("%.2f")
                        .flags(SliderFlags::ALWAYS_CLAMP)
                        .build(top_p);
                });
                ui.table_next_column();
                checkbox_option_with_default(
                    ui,
                    &mut chatgpt.presence_penalty,
                    0.0,
                    |ui, presence_penalty| {
                        ui.set_next_item_width(ui.current_font_size() * -align);
                        ui.slider_config("Presence penalty", -2.0f32, 2.0f32)
                            .display_format("%.2f")
                            .flags(SliderFlags::ALWAYS_CLAMP)
                            .build(presence_penalty);
                    },
                );
                ui.table_next_column();
                checkbox_option_with_default(
                    ui,
                    &mut chatgpt.service_tier,
                    openai::ServiceTier::Auto,
                    |ui, service_tier| {
                        ui.set_next_item_width(ui.current_font_size() * -align);
                        combo_enum(ui, "Service tier", service_tier);
                    },
                );
                ui.table_next_column();
                checkbox_option_with_default(
                    ui,
                    &mut chatgpt.reasoning_effort,
                    openai::ReasoningEffort::Medium,
                    |ui, reasoning_effort| {
                        ui.set_next_item_width(ui.current_font_size() * -align);
                        combo_enum(ui, "Reasoning effort", reasoning_effort);
                    },
                );
                ui.same_line();
                help_marker(ui, "Effort for reasoning models (GPT-5 and o3 models)");
                ui.table_next_column();
                checkbox_option_with_default(
                    ui,
                    &mut chatgpt.verbosity,
                    openai::Verbosity::Medium,
                    |ui, verbosity| {
                        ui.set_next_item_width(ui.current_font_size() * -align);
                        combo_enum(ui, "Verbosity", verbosity);
                    },
                );
            }
        }

        ui.child_window("context_window").build(|| {
            if let Some(_t) = ui.begin_table_header_with_flags(
                "context",
                [
                    TableColumnSetup::new(""),
                    TableColumnSetup::new(""),
                    TableColumnSetup::new("Lock"),
                    TableColumnSetup::new("Role"),
                    TableColumnSetup::new("Message"),
                ],
                TableFlags::SIZING_STRETCH_PROP,
            ) {
                // System prompt row (static, from Settings).
                ui.table_next_column();
                ui.disabled(true, || drag_handle(ui));
                ui.table_next_column();
                ui.table_next_column();
                ui.disabled(true, || {
                    let mut dummy = true;
                    ui.checkbox("##lock", &mut dummy);
                });
                ui.table_next_column();
                ui.disabled(true, || {
                    ui.set_next_item_width(ui.current_font_size() * 6.0);
                    let mut system_role = Role::System;
                    combo_enum(ui, "##role", &mut system_role);
                });
                ui.table_next_column();
                ui.input_text_multiline(
                    "##",
                    &mut chatgpt.system_prompt,
                    [ui.content_region_avail()[0], 200.0],
                )
                .build();

                // Sweep buffers whose messages are gone; keep the rest. Stable
                // MsgIds make this robust to reorder/insert/delete.
                let live: std::collections::HashSet<MsgId> =
                    state.context.iter().map(|e| e.id).collect();
                self.buffers.retain(|id, _| live.contains(id));

                enum Interaction {
                    Delete(usize),
                    Swap(usize, usize),
                    SetRole(usize, Role),
                    SetName(usize, Option<String>),
                    CommitContent(usize, String),
                }
                let mut interact: Option<Interaction> = None;

                for (idx, entry) in state.context.iter().enumerate() {
                    let message = &entry.message;
                    let _id = ui.push_id_usize(idx);
                    ui.table_next_column();
                    drag_handle(ui);
                    if let Some(_tooltip) = ui
                        .drag_drop_source_config("##dd_message")
                        .flags(DragDropFlags::SOURCE_ALLOW_NULL_ID)
                        .begin_payload(idx)
                    {
                        ui.text(format!("{:?}", message));
                    }
                    if let Some(target) = ui.drag_drop_target() {
                        if let Some(Ok(payload)) = target
                            .accept_payload::<usize, _>("##dd_message", DragDropFlags::empty())
                        {
                            interact = Some(Interaction::Swap(payload.data, idx));
                        }
                    }
                    ui.table_next_column();
                    if ui.button_with_size("\u{00d7}", [ui.frame_height(), 0.0]) {
                        interact = Some(Interaction::Delete(idx));
                    }
                    ui.table_next_column();
                    let mut lock = message.name.is_some();
                    ui.disabled(message.role != Role::User, || {
                        if ui.checkbox("##lock", &mut lock) {
                            interact =
                                Some(Interaction::SetName(idx, lock.then(|| "info".to_string())));
                        }
                    });
                    ui.table_next_column();
                    ui.group(|| {
                        ui.set_next_item_width(ui.current_font_size() * 6.0);
                        let mut role = message.role.clone();
                        combo_enum(ui, "##role", &mut role);
                        if role != message.role {
                            interact = Some(Interaction::SetRole(idx, role));
                        }
                    });
                    ui.table_next_column();
                    let buf = self
                        .buffers
                        .entry(entry.id)
                        .or_insert_with(|| message.content.clone().unwrap_or_default());
                    ui.set_next_item_width(ui.content_region_avail()[0]);
                    ui.input_text("##content", buf).build();
                    if ui.is_item_deactivated_after_edit() {
                        interact = Some(Interaction::CommitContent(idx, buf.clone()));
                    }
                }

                match interact {
                    Some(Interaction::Delete(idx)) => {
                        handle.edit_context(ContextEdit::Delete(idx));
                    }
                    Some(Interaction::Swap(a, b)) => {
                        handle.edit_context(ContextEdit::Swap(a, b));
                    }
                    Some(Interaction::SetRole(idx, role)) => {
                        handle.edit_context(ContextEdit::SetRole { idx, role });
                    }
                    Some(Interaction::SetName(idx, name)) => {
                        handle.edit_context(ContextEdit::SetName { idx, name });
                    }
                    Some(Interaction::CommitContent(idx, content)) => {
                        handle.edit_context(ContextEdit::SetContent { idx, content });
                    }
                    None => {}
                }

                ui.table_next_column();
                ui.table_next_column();
                if ui.button_with_size("+", [ui.frame_height(), 0.0]) {
                    handle.edit_context(ContextEdit::Insert {
                        idx: state.context.len(),
                        message: Message {
                            content: Some(String::new()),
                            ..Default::default()
                        },
                    });
                }
                ui.table_next_column();
                ui.table_next_column();
                ui.table_next_column();
                ui.text_disabled("drag and drop by handle to reorder");
            }
        });
    }
}

/// Render one exchange's assistant turn, streaming-aware.
fn draw_exchange(ui: &Ui, ex: &ExchangeView) {
    let _wrap_token = ui.push_text_wrap_pos_with_pos(0.0);
    ui.text(""); // anchor for line wrapping
    ui.same_line();
    let draw_list = ui.get_window_draw_list();
    stroke_text_with_highlight(
        ui,
        &draw_list,
        &format!("[{}]", ex.model.as_ref()),
        1.0,
        Some(StyleColor::NavHighlight),
    );
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

/// Render the usage progress bar for one exchange.
fn draw_usage(ui: &Ui, model: &ModelId, usage: &Usage) {
    ui.same_line();
    ProgressBar::new(0.0)
        .overlay_text(format!(
            "{}: {} input + {} output ({} reasoning) = {}",
            model.as_ref(),
            usage.prompt_tokens,
            usage.completion_tokens,
            usage
                .completion_tokens_details
                .as_ref()
                .map(|x| x.reasoning_tokens)
                .unwrap_or_default(),
            usage.total_tokens,
        ))
        .size([500.0, 0.0])
        .build(ui);
}
