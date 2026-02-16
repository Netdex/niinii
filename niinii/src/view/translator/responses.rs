use imgui::{ProgressBar, SliderFlags, StyleColor, TreeNodeFlags};

use crate::{
    settings::Settings,
    translator::responses::{ResponsesTranslation, ResponsesTranslator},
    view::{
        mixins::{
            checkbox_option, checkbox_option_with_default, combo_enum, combo_list, ellipses,
            help_marker, stroke_text_with_highlight,
        },
        View,
    },
};

pub struct ViewResponsesTranslator<'a>(pub &'a ResponsesTranslator, pub &'a mut Settings);
impl View for ViewResponsesTranslator<'_> {
    fn ui(&mut self, ui: &imgui::Ui) {
        let Self(translator, settings) = self;
        let mut reset_requested = false;
        ui.menu_bar(|| {
            ui.menu("Conversation", || {
                if ui.menu_item("Reset") {
                    reset_requested = true;
                }
            });
        });
        if reset_requested {
            translator.conversation().blocking_lock().take();
        }

        let responses = &mut settings.responses;

        if ui.collapsing_header("Conversation", TreeNodeFlags::DEFAULT_OPEN) {
            let guard = translator.conversation().blocking_lock();
            if let Some(info) = guard.as_ref() {
                let mut id = info.id.clone();
                ui.input_text("Conversation ID", &mut id)
                    .read_only(true)
                    .build();
                ui.text_disabled(format!("Created at: {}", info.created_at));
            } else {
                ui.text_disabled("Conversation will be created on next translation");
            }
            drop(guard);
        }

        if ui.collapsing_header("Parameters", TreeNodeFlags::DEFAULT_OPEN) {
            if let Some(_token) = ui.begin_table("##responses_parameters", 2) {
                ui.table_next_column();
                ui.set_next_item_width(ui.current_font_size() * -8.0);
                combo_list(ui, "Model", &translator.models, &mut responses.model);
                ui.table_next_column();
                ui.checkbox("Stream", &mut responses.stream);
                ui.same_line();
                help_marker(ui, "Use SSE responses for partial updates");

                ui.table_next_column();
                responses.store = true;
                ui.disabled(true, || {
                    ui.checkbox("Store response", &mut responses.store);
                });
                ui.same_line();
                help_marker(ui, "Required for server-side conversation context");

                ui.table_next_column();
                checkbox_option(ui, &mut responses.max_output_tokens, |ui, max_tokens| {
                    ui.set_next_item_width(ui.current_font_size() * -8.0);
                    ui.input_scalar("Max output tokens", max_tokens).build();
                });

                ui.table_next_column();
                checkbox_option(
                    ui,
                    &mut responses.compact_threshold,
                    |ui, compact_threshold| {
                        ui.set_next_item_width(ui.current_font_size() * -8.0);
                        ui.input_scalar("Compact threshold", compact_threshold)
                            .build();
                        ui.same_line();
                        help_marker(ui, "Minimum 1000 tokens");
                    },
                );

                ui.table_next_column();
                checkbox_option_with_default(
                    ui,
                    &mut responses.temperature,
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
                checkbox_option_with_default(ui, &mut responses.top_p, 1.0, |ui, top_p| {
                    ui.set_next_item_width(ui.current_font_size() * -8.0);
                    ui.slider_config("Top P", 0.0f32, 1.0f32)
                        .display_format("%.2f")
                        .flags(SliderFlags::ALWAYS_CLAMP)
                        .build(top_p);
                });

                ui.table_next_column();
                checkbox_option_with_default(
                    ui,
                    &mut responses.reasoning_effort,
                    openai::ReasoningEffort::Medium,
                    |ui, reasoning_effort| {
                        ui.set_next_item_width(ui.current_font_size() * -8.0);
                        combo_enum(ui, "Reasoning effort", reasoning_effort);
                    },
                );
                ui.table_next_column();
                checkbox_option_with_default(
                    ui,
                    &mut responses.verbosity,
                    openai::Verbosity::Medium,
                    |ui, verbosity| {
                        ui.set_next_item_width(ui.current_font_size() * -8.0);
                        combo_enum(ui, "Verbosity", verbosity);
                    },
                );
            }
        }

        ui.child_window("responses_prompt").build(|| {
            ui.input_text_multiline(
                "System prompt",
                &mut responses.system_prompt,
                [ui.content_region_avail()[0], 200.0],
            )
            .build();
        });
    }
}

pub struct ViewResponsesTranslation<'a>(pub &'a ResponsesTranslation);
impl View for ViewResponsesTranslation<'_> {
    fn ui(&mut self, ui: &imgui::Ui) {
        let draw_list = ui.get_window_draw_list();
        let _wrap_token = ui.push_text_wrap_pos_with_pos(0.0);
        ui.text("");
        ui.same_line();
        stroke_text_with_highlight(
            ui,
            &draw_list,
            &format!("[{}]", self.0.model.as_ref()),
            1.0,
            Some(StyleColor::NavHighlight),
        );
        let state = self.0.state().blocking_lock();
        if !state.text.is_empty() {
            ui.same_line();
            stroke_text_with_highlight(
                ui,
                &draw_list,
                &state.text,
                1.0,
                Some(StyleColor::TextSelectedBg),
            );
        }
        if !state.completed {
            ui.same_line_with_spacing(0.0, 0.0);
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

pub struct ViewResponsesTranslationUsage<'a>(pub &'a ResponsesTranslation);
impl View for ViewResponsesTranslationUsage<'_> {
    fn ui(&mut self, ui: &imgui::Ui) {
        let state = self.0.state().blocking_lock();
        if let Some(usage) = &state.usage {
            ui.same_line();
            ProgressBar::new(0.0)
                .overlay_text(format!(
                    "{}: {} input + {} output ({} cached) = {}",
                    self.0.model.as_ref(),
                    usage.input_tokens,
                    usage.output_tokens,
                    usage
                        .input_tokens_details
                        .as_ref()
                        .map(|d| d.cached_tokens)
                        .unwrap_or_default(),
                    usage.total_tokens
                ))
                .size([500.0, 0.0])
                .build(ui);
        }
    }
}
