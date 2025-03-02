use imgui::*;

use crate::{
    renderer::context::{Context, ContextFlags},
    settings::Settings,
};

use super::mixins::{self, checkbox_option, combo_enum};

pub struct SettingsView<'a>(pub &'a mut Settings);

impl SettingsView<'_> {
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
            combo_enum(ui, "Ruby text", &mut settings.ruby_text_type);
            ui.checkbox("Alternate interpretations", &mut settings.more_variants);
            ui.same_line();
            mixins::help_marker(ui, "Search for different ways to interpret a phrase");
            ui.checkbox("Stroke text", &mut settings.stroke_text);
        }
        if CollapsingHeader::new("Translation")
            .default_open(true)
            .build(ui)
        {
            combo_enum(ui, "Translator*", &mut settings.translator_type);
            ui.checkbox("Auto-translate", &mut settings.auto_translate);
            ui.input_text("DeepL API key", &mut settings.deepl_api_key)
                .password(true)
                .build();
            ui.input_text("OpenAI API key*", &mut settings.openai_api_key)
                .password(true)
                .build();
            ui.input_text("OpenAI API endpoint*", &mut settings.chat.api_endpoint)
                .build();
            ui.slider_config("OpenAI connection timeout (ms)*", 100, 10000)
                .build(&mut settings.chat.connection_timeout);
            ui.slider_config("OpenAI timeout (ms)*", 100, 10000)
                .build(&mut settings.chat.timeout);
        }

        if cfg!(feature = "voicevox")
            && CollapsingHeader::new("Text-to-speech")
                .default_open(true)
                .build(ui)
        {
            ui.input_text("VOICEVOX*", &mut settings.vv_model_path)
                .build();
            ui.same_line();
            mixins::help_marker(ui, "Path of VOICEVOX models");
            checkbox_option(ui, &mut settings.auto_tts_regex, |ui, auto_tts_regex| {
                ui.set_next_item_width(ui.current_font_size() * -8.0);
                ui.input_text("Auto TTS regex", auto_tts_regex).build();
            });
        }

        if CollapsingHeader::new("Rendering")
            .default_open(true)
            .build(ui)
        {
            if !ctx.flags().contains(ContextFlags::SHARED_RENDER_CONTEXT) {
                combo_enum(ui, "Renderer*", &mut settings.renderer_type);
                ui.same_line();
                mixins::help_marker(ui, "Renderer backend (Direct3D11 recommended)");

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

                #[cfg(windows)]
                {
                    ui.checkbox("Overlay mode*", &mut settings.overlay_mode);
                    ui.same_line();
                    mixins::help_marker(ui, "Overlay on top of all other windows (D3D11 only)");
                }
            }
            ui.checkbox("Transparent", &mut settings.transparent);
        }
    }
}
