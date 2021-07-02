use imgui::*;
use serde::{Deserialize, Serialize};

use crate::support::{Env, ImStringDef};

#[derive(Default, Deserialize, Serialize)]
pub struct SettingsView {
    #[serde(with = "ImStringDef")]
    ichiran_path: ImString,
}
impl SettingsView {
    pub fn ichiran_path(&self) -> &str {
        self.ichiran_path.to_str()
    }
    pub fn ui(&mut self, env: &mut Env, ui: &Ui) {
        ui.input_text(im_str!("ichiran-cli"), &mut self.ichiran_path)
            .resize_buffer(true)
            .build();
    }
}
