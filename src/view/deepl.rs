use imgui::*;

use crate::translation::Translation;

pub struct DeepLView<'a>(&'a Translation);

impl<'a> DeepLView<'a> {
    pub fn new(translation: &'a Translation) -> Self {
        DeepLView(translation)
    }
    pub fn ui(&self, ui: &Ui) {
        let _wrap_token = ui.push_text_wrap_pos_with_pos(0.0);
        let fraction =
            self.0.deepl_usage.character_count as f32 / self.0.deepl_usage.character_limit as f32;
        ProgressBar::new(fraction)
            .overlay_text(format!(
                "DeepL API usage: {}/{} ({:.2}%)",
                self.0.deepl_usage.character_count,
                self.0.deepl_usage.character_limit,
                fraction * 100.0
            ))
            .build(ui);
        ui.text(&self.0.deepl_text);
    }
}
