use imgui::*;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use serde::{Deserialize, Serialize};
use strum::VariantNames;
use strum_macros::{EnumString, EnumVariantNames};

use crate::backend::env::{Env, EnvFlags};

use super::mixins;

#[derive(FromPrimitive, EnumString, EnumVariantNames)]
pub enum SupportedRenderer {
    Glow = 0,
    #[cfg(windows)]
    Direct3D11 = 1,
}

#[derive(Copy, Clone, PartialEq, Eq, FromPrimitive, EnumString, EnumVariantNames)]
pub enum DisplayRubyText {
    None = 0,
    Furigana = 1,
    Romaji = 2,
}

#[derive(Deserialize, Serialize)]
#[serde(default)]
pub struct Settings {
    pub ichiran_path: String,
    pub postgres_path: String,
    pub db_path: String,

    renderer_type_idx: usize,
    pub transparent: bool,
    pub on_top: bool,
    pub overlay_mode: bool,

    ruby_text_type_idx: usize,
    pub more_variants: bool,
    pub stroke_text: bool,

    pub auto_translate: bool,
    pub deepl_api_key: String,

    pub watch_clipboard: bool,
    pub show_manual_input: bool,
    pub style: Option<Vec<u8>>,

    pub regex_match: String,
    pub regex_replace: String,

    pub inject_proc_name: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            ichiran_path: Default::default(),
            postgres_path: Default::default(),
            db_path: Default::default(),

            renderer_type_idx: Default::default(),
            transparent: Default::default(),
            on_top: false,
            overlay_mode: false,

            ruby_text_type_idx: DisplayRubyText::None as usize,
            more_variants: true,
            stroke_text: true,

            auto_translate: false,
            deepl_api_key: Default::default(),

            watch_clipboard: true,
            show_manual_input: true,
            style: None,

            regex_match: Default::default(),
            regex_replace: Default::default(),

            inject_proc_name: Default::default(),
        }
    }
}
impl Settings {
    pub fn ui(&mut self, env: &mut Env, ui: &Ui) {
        if CollapsingHeader::new("Ichiran")
            .default_open(true)
            .build(ui)
        {
            ui.input_text("ichiran-cli*", &mut self.ichiran_path)
                .build();
            ui.same_line();
            mixins::help_marker(ui, "Path of ichiran-cli executable");

            ui.input_text("postgres*", &mut self.postgres_path).build();
            ui.same_line();
            mixins::help_marker(ui, "Path of postgres 'bin' directory");

            ui.input_text("db*", &mut self.db_path).build();
            ui.same_line();
            mixins::help_marker(ui, "Path of postgres database directory");
        }

        if CollapsingHeader::new("Advanced")
            .default_open(true)
            .build(ui)
        {
            ui.input_text("Regex match", &mut self.regex_match).build();
            ui.input_text("Regex replace", &mut self.regex_replace)
                .build();
        }

        if CollapsingHeader::new("Interface")
            .default_open(true)
            .build(ui)
        {
            ui.combo_simple_string(
                "Ruby text",
                &mut self.ruby_text_type_idx,
                DisplayRubyText::VARIANTS,
            );
            ui.checkbox("Alternate interpretations", &mut self.more_variants);
            ui.same_line();
            mixins::help_marker(ui, "Search for different ways to interpret a phrase");
            ui.checkbox("Stroke text", &mut self.stroke_text);
        }
        if CollapsingHeader::new("DeepL").default_open(true).build(ui) {
            ui.checkbox("Auto-translate", &mut self.auto_translate);
            ui.input_text("DeepL API key", &mut self.deepl_api_key)
                .password(true)
                .build();
        }

        if CollapsingHeader::new("Rendering")
            .default_open(true)
            .build(ui)
        {
            if !env.flags().contains(EnvFlags::SHARED_RENDER_CONTEXT) {
                ui.combo_simple_string(
                    "Renderer*",
                    &mut self.renderer_type_idx,
                    SupportedRenderer::VARIANTS,
                );

                ui.checkbox("Always on-top*", &mut self.on_top);
                ui.same_line();
                mixins::help_marker(
                    ui,
                    "Whether to always put the window on top of others or not",
                );

                #[cfg(windows)]
                {
                    ui.checkbox("Overlay mode*", &mut self.overlay_mode);
                    ui.same_line();
                    mixins::help_marker(
                        ui,
                        "Turns the window into an overlay on top of all other windows (D3D11 only)",
                    );
                }
            }
            ui.checkbox("Transparent*", &mut self.transparent);
            ui.same_line();
            mixins::help_marker(ui, "Whether to make the window transparent or not");
        }
    }

    pub fn active_renderer(&self) -> SupportedRenderer {
        SupportedRenderer::from_usize(self.renderer_type_idx).unwrap()
    }

    pub fn display_ruby_text(&self) -> DisplayRubyText {
        DisplayRubyText::from_usize(self.ruby_text_type_idx).unwrap()
    }

    pub fn set_style(&mut self, style: Option<&imgui::Style>) {
        if let Some(style) = style {
            self.style = Some(
                unsafe {
                    std::slice::from_raw_parts(
                        (style as *const _) as *const u8,
                        std::mem::size_of::<imgui::Style>(),
                    )
                }
                .to_vec(),
            );
        } else {
            self.style = None;
        }
    }
    pub fn style(&self) -> Option<imgui::Style> {
        self.style
            .as_ref()
            .map(|style| unsafe { std::ptr::read(style.as_ptr() as *const _) })
    }

    const CONFIG_FILE: &str = "niinii.toml";
    pub fn from_file() -> Self {
        let user_config = dirs::config_dir().map(|x| x.join("niinii").join(Self::CONFIG_FILE));
        let settings: Settings = std::fs::read_to_string(Self::CONFIG_FILE)
            .ok()
            .or_else(|| user_config.and_then(|x| std::fs::read_to_string(x).ok()))
            .and_then(|x| toml::from_str(&x).ok())
            .unwrap_or_default();
        settings
    }
    pub fn write_to_file(&self) -> std::io::Result<()> {
        std::fs::write(Self::CONFIG_FILE, toml::to_string(self).unwrap())
    }
}
