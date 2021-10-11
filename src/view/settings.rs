use imgui::*;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use serde::{Deserialize, Serialize};
use strum::VariantNames;
use strum_macros::{EnumString, EnumVariantNames};

use crate::common::Env;

use super::mixins;

#[derive(FromPrimitive, EnumString, EnumVariantNames)]
pub enum RendererType {
    Glow = 0,
    Direct3D11 = 1,
}

#[derive(Copy, Clone, PartialEq, Eq, FromPrimitive, EnumString, EnumVariantNames)]
pub enum RubyTextType {
    None = 0,
    Furigana = 1,
    Romaji = 2,
}

#[derive(Default, Deserialize, Serialize)]
pub struct SettingsView {
    pub ichiran_path: String,
    pub transparent: bool,
    pub on_top: bool,
    pub postgres_path: String,
    pub db_path: String,
    pub renderer_type_idx: usize,

    pub ruby_text_type_idx: usize,
    pub kanji: bool,
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
            RendererType::VARIANTS,
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

        ui.separator();

        ui.combo_simple_string(
            "Ruby text",
            &mut self.ruby_text_type_idx,
            RubyTextType::VARIANTS,
        );
        ui.checkbox("Kanji lookup (slow)", &mut self.kanji);
    }

    pub fn renderer(&self) -> RendererType {
        RendererType::from_usize(self.renderer_type_idx).unwrap()
    }

    pub fn ruby_text(&self) -> RubyTextType {
        RubyTextType::from_usize(self.ruby_text_type_idx).unwrap()
    }
}
