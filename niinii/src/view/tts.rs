use imgui::*;

use crate::{settings::Settings, tts::TtsEngine};

pub struct TtsEngineView<'a>(pub &'a TtsEngine, pub &'a mut Settings);
impl TtsEngineView<'_> {
    pub fn ui(&mut self, ui: &Ui) {
        let TtsEngineView(tts_engine, settings) = self;
        tts_engine.show_tts(ui, settings);
    }
}

trait ViewTtsEngine {
    fn show_tts(&self, ui: &Ui, settings: &mut Settings);
}
impl ViewTtsEngine for TtsEngine {
    fn show_tts(&self, ui: &Ui, _settings: &mut Settings) {
        if ui.button("shut up") {
            self.stop();
        }
    }
}
