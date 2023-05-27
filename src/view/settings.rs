use imgui::*;
use strum::VariantNames;

use crate::{
    renderer::context::{Context, ContextFlags},
    settings::{RendererType, RubyTextType, Settings, TranslatorType},
};

use super::mixins;

pub struct SettingsView<'a>(pub &'a mut Settings);

impl<'a> SettingsView<'a> {
    pub fn ui(&mut self, ctx: &mut Context, ui: &Ui) {
        let settings = &mut self.0;
        if CollapsingHeader::new("Ichiran")
            .default_open(true)
            .build(ui)
        {
            ui.input_text("ichiran-cli*", &mut settings.ichiran_path)
                .build();
            ui.same_line();
            mixins::help_marker(ui, "Path of ichiran-cli executable");

            ui.input_text("postgres*", &mut settings.postgres_path)
                .build();
            ui.same_line();
            mixins::help_marker(ui, "Path of postgres 'bin' directory");

            ui.input_text("db*", &mut settings.db_path).build();
            ui.same_line();
            mixins::help_marker(ui, "Path of postgres database directory");
        }

        if CollapsingHeader::new("Advanced")
            .default_open(true)
            .build(ui)
        {
            ui.input_text("Regex match", &mut settings.regex_match)
                .build();
            ui.input_text("Regex replace", &mut settings.regex_replace)
                .build();
        }

        if CollapsingHeader::new("Interface")
            .default_open(true)
            .build(ui)
        {
            ui.combo_simple_string(
                "Ruby text",
                &mut settings.ruby_text_type_idx,
                RubyTextType::VARIANTS,
            );
            ui.checkbox("Alternate interpretations", &mut settings.more_variants);
            ui.same_line();
            mixins::help_marker(ui, "Search for different ways to interpret a phrase");
            ui.checkbox("Stroke text", &mut settings.stroke_text);
        }
        if CollapsingHeader::new("Translation")
            .default_open(true)
            .build(ui)
        {
            ui.combo_simple_string(
                "Translator*",
                &mut settings.translator_type_idx,
                TranslatorType::VARIANTS,
            );
            ui.checkbox("Auto-translate", &mut settings.auto_translate);
            ui.input_text("DeepL API key", &mut settings.deepl_api_key)
                .password(true)
                .build();
            ui.input_text("OpenAI API key", &mut settings.openai_api_key)
                .password(true)
                .build();
            ui.input_scalar(
                "ChatGPT max context tokens",
                &mut settings.chatgpt_max_context_tokens,
            )
            .build();
            ui.input_scalar("ChatGPT max tokens", &mut settings.chatgpt_max_tokens)
                .build();
            ui.checkbox("ChatGPT moderation", &mut settings.chatgpt_moderation);
        }

        if CollapsingHeader::new("Text-to-speech")
            .default_open(true)
            .build(ui)
        {
            ui.input_text("VOICEVOX*", &mut settings.vv_model_path)
                .build();
            ui.same_line();
            mixins::help_marker(ui, "Path of VOICEVOX models");
        }

        if CollapsingHeader::new("Rendering")
            .default_open(true)
            .build(ui)
        {
            if !ctx.flags().contains(ContextFlags::SHARED_RENDER_CONTEXT) {
                ui.combo_simple_string(
                    "Renderer*",
                    &mut settings.renderer_type_idx,
                    RendererType::VARIANTS,
                );
                ui.checkbox("##", &mut settings.use_force_dpi);
                ui.same_line();
                ui.disabled(!settings.use_force_dpi, || {
                    ui.slider_config("Force DPI*", 0.5f64, 2.0f64)
                        .display_format("%.2f")
                        .flags(SliderFlags::ALWAYS_CLAMP)
                        .build(&mut settings.force_dpi);
                    ui.same_line();
                });
                mixins::help_marker(
                    ui,
                    "Force DPI used for global scaling factor (CTRL+click to type)",
                );

                ui.checkbox("Always on-top*", &mut settings.on_top);
                ui.same_line();
                mixins::help_marker(
                    ui,
                    "Whether to always put the window on top of others or not",
                );

                #[cfg(windows)]
                {
                    ui.checkbox("Overlay mode*", &mut settings.overlay_mode);
                    ui.same_line();
                    mixins::help_marker(
                        ui,
                        "Turns the window into an overlay on top of all other windows (D3D11 only)",
                    );
                }
            }
            ui.checkbox("Transparent*", &mut settings.transparent);
            ui.same_line();
            mixins::help_marker(ui, "Whether to make the window transparent or not");
        }
    }
}
