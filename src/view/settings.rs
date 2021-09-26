use imgui::*;
use serde::{Deserialize, Serialize};

use crate::common::Env;

#[derive(Default, Deserialize, Serialize)]
pub struct SettingsView {
    pub ichiran_path: String,
    pub show_raw: bool,
}
impl SettingsView {
    pub fn ui(&mut self, _env: &mut Env, ui: &Ui) {
        ui.input_text("ichiran-cli", &mut self.ichiran_path).build();
        ui.checkbox("Show raw", &mut self.show_raw);
    }
}
