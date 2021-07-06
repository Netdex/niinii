use imgui::*;
use serde::{Deserialize, Serialize};

use crate::support::{Env, ImStringDef};

#[derive(Default, Deserialize, Serialize)]
pub struct SettingsView {
    #[serde(with = "ImStringDef")]
    pub ichiran_path: ImString,
    pub show_raw: bool,
}
impl SettingsView {
    pub fn ui(&mut self, env: &mut Env, ui: &Ui) {
        ui.input_text(im_str!("ichiran-cli"), &mut self.ichiran_path)
            .resize_buffer(true)
            .build();
        ui.checkbox(im_str!("Show raw"), &mut self.show_raw);
    }
}
