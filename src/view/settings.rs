use imgui::*;
use serde::{Deserialize, Serialize};

use crate::common::Env;

#[derive(Default, Deserialize, Serialize)]
pub struct SettingsView {
    pub ichiran_path: String,
    pub transparent: bool,
    pub on_top: bool,
    pub postgres_path: String,
    pub db_path: String,
}
impl SettingsView {
    pub fn ui(&mut self, _env: &mut Env, ui: &Ui) {
        ui.spacing();
        ui.input_text("ichiran-cli (restart)", &mut self.ichiran_path)
            .build();
        ui.input_text("postgres (restart)", &mut self.postgres_path)
            .build();
        ui.input_text("db (restart)", &mut self.db_path).build();
        ui.separator();
        ui.checkbox("Transparent (restart)", &mut self.transparent);
        ui.checkbox("Always on-top (restart)", &mut self.on_top);
    }
}
