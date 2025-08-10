use imgui::*;
use openai::chat::Role;

use crate::{
    settings::Settings,
    translator::chat::{ChatTranslation, ChatTranslator},
    view::mixins::{combo_list, drag_handle, help_marker},
};

use crate::view::{
    mixins::{
        checkbox_option, checkbox_option_with_default, combo_enum, ellipses,
        stroke_text_with_highlight,
    },
    View,
};

pub struct ViewChatTranslator<'a>(pub &'a ChatTranslator, pub &'a mut Settings);
impl View for ViewChatTranslator<'_> {
    fn ui(&mut self, ui: &Ui) {
        let ViewChatTranslator(translator, settings) = self;
        let mut chat = translator.buffer.blocking_lock();
        let chatgpt = &mut settings.chat;
        ui.menu_bar(|| {
            // ui.menu("Settings", || {
            // });
            // ui.separator();
            if ui.menu_item("Clear") {
                chat.clear();
            }
        });
        if ui.collapsing_header("Tuning", TreeNodeFlags::DEFAULT_OPEN) {
            if let Some(_token) = ui.begin_table("##", 2) {
                ui.table_next_column();
                ui.set_next_item_width(ui.current_font_size() * -8.0);
                combo_list(ui, "Model", &translator.models, &mut chatgpt.model);
                ui.table_next_column();
                ui.checkbox("Stream", &mut chatgpt.stream);
                ui.same_line();
                help_marker(
                    ui,
                    "Use streaming API (may require ID verification for some models)",
                );
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
                checkbox_option_with_default(
                    ui,
                    &mut chatgpt.service_tier,
                    openai::chat::ServiceTier::Auto,
                    |ui, service_tier| {
                        ui.set_next_item_width(ui.current_font_size() * -8.0);
                        combo_enum(ui, "Service tier", service_tier);
                    },
                );
                ui.table_next_column();
                checkbox_option_with_default(
                    ui,
                    &mut chatgpt.reasoning_effort,
                    openai::chat::ReasoningEffort::Medium,
                    |ui, reasoning_effort| {
                        ui.set_next_item_width(ui.current_font_size() * -8.0);
                        combo_enum(ui, "Effort", reasoning_effort);
                    },
                );
                ui.same_line();
                help_marker(ui, "Effort for reasoning models (GPT-5 and o3 models)");
                ui.table_next_column();
                checkbox_option_with_default(
                    ui,
                    &mut chatgpt.verbosity,
                    openai::chat::Verbosity::Medium,
                    |ui, verbosity| {
                        ui.set_next_item_width(ui.current_font_size() * -8.0);
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
                ui.table_next_column();
                ui.disabled(true, || {
                    drag_handle(ui);
                });
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
                    let mut lock = message.name.is_some();
                    ui.disabled(message.role != Role::User, || {
                        ui.checkbox("##lock", &mut lock);
                    });
                    message.name = if lock { Some("info".into()) } else { None };
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

                ui.table_next_column();
                ui.table_next_column();
                if ui.button_with_size("+", [ui.frame_height(), 0.0]) {
                    chat.context_mut().push_back(openai::chat::Message {
                        content: Some(String::new()),
                        ..Default::default()
                    })
                }
                ui.table_next_column();
                ui.table_next_column();
                ui.table_next_column();
                ui.text_disabled("drag and drop by handle to reorder");
            }
        });
    }
}

pub struct ViewChatTranslation<'a>(pub &'a ChatTranslation);
impl View for ViewChatTranslation<'_> {
    fn ui(&mut self, ui: &imgui::Ui) {
        let _wrap_token = ui.push_text_wrap_pos_with_pos(0.0);
        ui.text(""); // anchor for line wrapping
        ui.same_line();
        let ChatTranslation {
            model, exchange, ..
        } = self.0;
        let exchange = exchange.blocking_lock();
        let draw_list = ui.get_window_draw_list();
        stroke_text_with_highlight(
            ui,
            &draw_list,
            &format!("[{}]", model.as_ref()),
            1.0,
            Some(StyleColor::NavHighlight),
        );
        for content in exchange.response().iter().flat_map(|c| c.content.as_ref()) {
            ui.same_line();
            stroke_text_with_highlight(
                ui,
                &draw_list,
                content,
                1.0,
                Some(StyleColor::TextSelectedBg),
            );
        }
        if !exchange.completed() {
            if exchange.response().is_none() {
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
}

pub struct ViewChatTranslationUsage<'a>(pub &'a ChatTranslation);
impl View for ViewChatTranslationUsage<'_> {
    fn ui(&mut self, ui: &imgui::Ui) {
        let ChatTranslation {
            model, exchange, ..
        } = self.0;
        let exchange = exchange.blocking_lock();
        let usage = exchange.usage();
        if let Some(usage) = usage {
            ui.same_line();
            ProgressBar::new(0.0)
                .overlay_text(format!(
                    "{}: {} input + {} output ({} reasoning) = {}",
                    model.as_ref(),
                    usage.prompt_tokens,
                    usage.completion_tokens,
                    usage.completion_tokens_details.reasoning_tokens,
                    usage.total_tokens,
                ))
                .size([500.0, 0.0])
                .build(ui);
        }
    }
}
