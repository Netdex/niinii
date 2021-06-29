use eframe::egui::Ui;
use serde::{Deserialize, Serialize};

use crate::View;

#[derive(Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub ichiran_path: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ichiran_path: "ichiran-cli.exe".to_owned(),
        }
    }
}
impl View for Config {
    fn ui(&mut self, ui: &mut eframe::egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("ichiran-cli: ");
            ui.text_edit_singleline(&mut self.ichiran_path);
        });
    }
}
