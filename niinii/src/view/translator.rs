use enum_dispatch::enum_dispatch;
use imgui::*;
use openai_chat::chat::{Model, Role};

use crate::{
    settings::Settings,
    translator::{
        chatgpt::{ChatGptTranslation, ChatGptTranslator},
        deepl::{DeepLTranslation, DeepLTranslator},
        Translation, Translator,
    },
    view::mixins::drag_handle,
};

use super::mixins::{
    checkbox_option, checkbox_option_with_default, combo_enum, ellipses, stroke_text,
    stroke_text_with_highlight,
};

pub struct TranslatorView<'a>(pub &'a Translator, pub &'a mut Settings);
impl TranslatorView<'_> {
    pub fn ui(&mut self, ui: &Ui) {
        let TranslatorView(translator, settings) = self;
        translator.show_translator(ui, settings);
    }
}
#[enum_dispatch(Translator)]
trait ViewTranslator {
    fn show_translator(&self, ui: &Ui, settings: &mut Settings);
}
impl ViewTranslator for ChatGptTranslator {
    fn show_translator(&self, ui: &Ui, settings: &mut Settings) {
        let mut chat = self.chat.blocking_lock();
        let chatgpt = &mut settings.chatgpt;
        ui.menu_bar(|| {
            ui.menu("Settings", || {
                ui.menu_item_config("Moderation")
                    .build_with_ref(&mut chatgpt.moderation);
            });
            ui.separator();
            ui.disabled(chat.pending_response(), || {
                if ui.menu_item("Clear") {
                    chat.clear();
                }
            });
        });
        if ui.collapsing_header("Tuning", TreeNodeFlags::DEFAULT_OPEN) {
            if let Some(_token) = ui.begin_table("##", 2) {
                ui.table_next_column();
                ui.set_next_item_width(ui.current_font_size() * -8.0);
                ui.input_scalar("Max context tokens", &mut chatgpt.max_context_tokens)
                    .build();
                ui.table_next_column();
                checkbox_option(ui, &mut chatgpt.max_tokens, |ui, max_tokens| {
                    ui.set_next_item_width(ui.current_font_size() * -8.0);
                    ui.input_scalar("Max tokens", max_tokens).build();
                });
                ui.table_next_column();
                checkbox_option_with_default(
                    ui,
                    &mut chatgpt.temperature,
                    1.0,
                    |ui, temperature| {
                        ui.set_next_item_width(ui.current_font_size() * -8.0);
                        ui.slider_config("Temperature", 0.0f32, 2.0f32)
                            .display_format("%.2f")
                            .flags(SliderFlags::ALWAYS_CLAMP)
                            .build(temperature);
                    },
                );
                ui.table_next_column();
                checkbox_option_with_default(ui, &mut chatgpt.top_p, 1.0, |ui, top_p| {
                    ui.set_next_item_width(ui.current_font_size() * -8.0);
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
                        ui.set_next_item_width(ui.current_font_size() * -8.0);
                        ui.slider_config("Presence penalty", -2.0f32, 2.0f32)
                            .display_format("%.2f")
                            .flags(SliderFlags::ALWAYS_CLAMP)
                            .build(presence_penalty);
                    },
                );
                ui.table_next_column();
                ui.set_next_item_width(ui.current_font_size() * -8.0);
                combo_enum(ui, "Model", &mut chatgpt.model);
            }
        }
        ui.child_window("context_window").build(|| {
            if let Some(_t) = ui.begin_table_header_with_flags(
                "context",
                [
                    TableColumnSetup::new(""),
                    TableColumnSetup::new(""),
                    TableColumnSetup::new("Role"),
                    TableColumnSetup::new("Message"),
                ],
                TableFlags::SIZING_STRETCH_PROP,
            ) {
                ui.table_next_column();
                ui.table_next_column();
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

                enum Interaction {
                    Delete(usize),
                    Swap(usize, usize),
                }
                let mut interact = None;
                for (idx, message) in chat.context_mut().iter_mut().enumerate() {
                    let _id = ui.push_id_ptr(message);
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
                    ui.group(|| {
                        ui.set_next_item_width(ui.current_font_size() * 6.0);
                        combo_enum(ui, "##role", &mut message.role);
                    });

                    ui.table_next_column();
                    if let Some(content) = &mut message.content {
                        ui.set_next_item_width(ui.content_region_avail()[0]);
                        ui.input_text("##content", content).build();
                    }
                }

                match interact {
                    Some(Interaction::Delete(idx)) => {
                        chat.context_mut().remove(idx);
                    }
                    Some(Interaction::Swap(src, dst)) => {
                        chat.context_mut().swap(src, dst);
                    }
                    _ => {}
                }

                ui.disabled(true, || {
                    for message in chat.response_mut() {
                        let _id = ui.push_id_ptr(message);
                        ui.table_next_column();
                        ui.table_next_column();
                        ui.table_next_column();
                        ui.set_next_item_width(ui.current_font_size() * 6.0);
                        let mut system_role = openai_chat::chat::Role::Assistant;
                        combo_enum(ui, "##role", &mut system_role);
                        ui.table_next_column();
                        ui.disabled(true, || {
                            if let Some(content) = &mut message.content {
                                ui.set_next_item_width(ui.content_region_avail()[0]);
                                ui.input_text("##content", content).build();
                            }
                        });
                    }
                });

                ui.table_next_column();
                ui.table_next_column();
                if ui.button_with_size("+", [ui.frame_height(), 0.0]) {
                    chat.context_mut().push_back(openai_chat::chat::Message {
                        content: Some(String::new()),
                        ..Default::default()
                    })
                }
                ui.table_next_column();
                ui.table_next_column();
                ui.text_disabled("drag and drop by handle to reorder");
            }
        });
    }
}
impl ViewTranslator for DeepLTranslator {
    fn show_translator(&self, _ui: &Ui, _settings: &mut Settings) {}
}

pub struct TranslationView<'a>(pub &'a Translation);
impl TranslationView<'_> {
    pub fn ui(&self, ui: &Ui) {
        self.0.view(ui);
    }
}
pub struct TranslationUsageView<'a>(pub &'a Translation);
impl TranslationUsageView<'_> {
    pub fn ui(&self, ui: &Ui) {
        self.0.show_usage(ui);
    }
}
#[enum_dispatch(Translation)]
trait ViewTranslation {
    fn view(&self, ui: &Ui);
    fn show_usage(&self, ui: &Ui);
}
impl ViewTranslation for ChatGptTranslation {
    fn view(&self, ui: &Ui) {
        let _wrap_token = ui.push_text_wrap_pos_with_pos(0.0);
        ui.text(""); // anchor for line wrapping
        ui.same_line();
        match self {
            ChatGptTranslation::Translated { chat, .. } => {
                let chat = chat.blocking_lock();
                let draw_list = ui.get_window_draw_list();
                stroke_text_with_highlight(
                    ui,
                    &draw_list,
                    "[ChatGPT]",
                    1.0,
                    Some(StyleColor::NavHighlight),
                );
                for content in chat.response().iter().flat_map(|c| c.content.as_ref()) {
                    ui.same_line();
                    stroke_text_with_highlight(
                        ui,
                        &draw_list,
                        content,
                        1.0,
                        Some(StyleColor::TextSelectedBg),
                    );
                }
                if chat.pending_response() {
                    if chat.response().is_empty() {
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
            }
            ChatGptTranslation::Filtered { moderation } => {
                let draw_list = ui.get_window_draw_list();
                for k in moderation
                    .categories
                    .iter()
                    .filter_map(|(k, &v)| if v { Some(k) } else { None })
                {
                    stroke_text_with_highlight(
                        ui,
                        &draw_list,
                        k.as_ref(),
                        1.0,
                        Some(StyleColor::PlotLinesHovered),
                    );
                    if ui.is_item_hovered() {
                        ui.tooltip_text(format!("{:.1}%", moderation.category_scores[k] * 100.0))
                    }
                }
                ui.same_line();
                drop(draw_list);
            }
        }
    }
    fn show_usage(&self, ui: &Ui) {
        if let ChatGptTranslation::Translated { model, chat, .. } = self {
            let chat = chat.blocking_lock();
            let usage = chat.usage();
            if let Some(usage) = usage {
                ui.same_line();
                let cost = model.cost(usage.prompt_tokens, usage.completion_tokens);
                ProgressBar::new(0.0)
                    .overlay_text(format!(
                        "{}: {} input + {} output = {} (${:.6})",
                        <&Model as Into<&'static str>>::into(model),
                        usage.prompt_tokens,
                        usage.completion_tokens,
                        usage.total_tokens,
                        cost
                    ))
                    .size([500.0, 0.0])
                    .build(ui);
            }
        }
    }
}
impl ViewTranslation for DeepLTranslation {
    fn view(&self, ui: &Ui) {
        let _wrap_token = ui.push_text_wrap_pos_with_pos(0.0);
        ui.text(""); // anchor for line wrapping
        let draw_list = ui.get_window_draw_list();
        stroke_text_with_highlight(
            ui,
            &draw_list,
            "[DeepL]",
            1.0,
            Some(StyleColor::TextSelectedBg),
        );
        ui.same_line();
        stroke_text(ui, &draw_list, &self.deepl_text, 1.0);
    }
    fn show_usage(&self, ui: &Ui) {
        ui.same_line();
        let fraction =
            self.deepl_usage.character_count as f32 / self.deepl_usage.character_limit as f32;
        ProgressBar::new(fraction)
            .overlay_text(format!(
                "usage: {}/{} ({:.2}%)",
                self.deepl_usage.character_count,
                self.deepl_usage.character_limit,
                fraction * 100.0
            ))
            .size([350.0, 0.0])
            .build(ui);
    }
}
