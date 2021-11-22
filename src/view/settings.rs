use imgui::*;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use serde::{Deserialize, Serialize};
use strum::VariantNames;
use strum_macros::{EnumString, EnumVariantNames};

use crate::common::Env;

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

#[derive(Default, Deserialize, Serialize)]
#[serde(default)]
pub struct SettingsView {
    pub ichiran_path: String,
    pub transparent: bool,
    pub on_top: bool,
    pub postgres_path: String,
    pub db_path: String,
    renderer_type_idx: usize,

    #[cfg(windows)]
    pub overlay_mode: bool,

    pub show_manual_input: bool,
    ruby_text_type_idx: usize,
    pub show_variant_switcher: bool,
}
impl SettingsView {
    pub fn ui(&mut self, _env: &mut Env, ui: &Ui) {
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

        ui.separator();

        ui.combo_simple_string(
            "Renderer*",
            &mut self.renderer_type_idx,
            SupportedRenderer::VARIANTS,
        );
        ui.checkbox("Transparent*", &mut self.transparent);
        ui.same_line();
        mixins::help_marker(ui, "Whether to make the window transparent or not");

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

        ui.separator();

        ui.combo_simple_string(
            "Ruby text",
            &mut self.ruby_text_type_idx,
            DisplayRubyText::VARIANTS,
        );
        ui.checkbox("Show manual input", &mut self.show_manual_input);
        ui.checkbox("Variant switcher", &mut self.show_variant_switcher);
    }

    pub fn active_renderer(&self) -> SupportedRenderer {
        SupportedRenderer::from_usize(self.renderer_type_idx).unwrap()
    }

    pub fn display_ruby_text(&self) -> DisplayRubyText {
        DisplayRubyText::from_usize(self.ruby_text_type_idx).unwrap()
    }
}
