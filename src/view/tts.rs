use imgui::*;

use crate::{settings::Settings, tts::TtsEngine};

use super::mixins::{stroke_text, stroke_token_with_color};

pub struct TtsEngineView<'a>(pub &'a TtsEngine, pub &'a mut Settings);
impl<'a> TtsEngineView<'a> {
    pub fn ui(&mut self, ui: &Ui) {
        let TtsEngineView(tts_engine, settings) = self;
        tts_engine.show_tts(ui, settings);
    }
}

trait ViewTtsEngine {
    fn show_tts(&self, ui: &Ui, settings: &mut Settings);
}
impl ViewTtsEngine for TtsEngine {
    fn show_tts(&self, ui: &Ui, settings: &mut Settings) {
        if ui.button("shut up") {
            self.stop();
        }
    }
}
