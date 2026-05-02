use imgui::*;

use crate::{
    renderer::context::{Context, ContextFlags},
    settings::Settings,
};

use super::mixins::{self, checkbox_option, combo_enum};

#[derive(Default)]
pub struct SettingsView {
    pub open: bool,
}

impl SettingsView {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn show_menu_item(&mut self, ui: &Ui) {
        if ui.menu_item("Settings") {
            self.open = true;
        }
    }

    pub fn ui(&mut self, ctx: &mut Context, ui: &Ui, settings: &mut Settings) {
        if !self.open {
            return;
        }
        let Some(_window) = ui
            .window("Settings")
            .always_auto_resize(true)
            .opened(&mut self.open)
            .begin()
        else {
            return;
        };
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

            let mut pool_size = settings.ichiran_pool_size as i32;
            if ui
                .input_int("pool size*", &mut pool_size)
                .step(1)
                .build()
            {
                settings.ichiran_pool_size = pool_size.max(1) as usize;
            }
            ui.same_line();
            mixins::help_marker(
                ui,
                "Number of resident ichiran-cli workers. \
                 More = better parse parallelism, but each worker holds a Postgres \
                 connection. Takes effect on restart.",
            );
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
        ui.separator();
        ui.text_disabled("* Restart to apply these changes");
    }
}
