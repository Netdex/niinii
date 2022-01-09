use imgui::*;

use crate::translation::Translation;

pub struct DeepLView<'a>(&'a Translation);

impl<'a> DeepLView<'a> {
    pub fn new(translation: &'a Translation) -> Self {
        DeepLView(translation)
    }
    pub fn ui(&self, ui: &Ui) {
        let _wrap_token = ui.push_text_wrap_pos_with_pos(0.0);
        let Translation::DeepL { deepl_text, .. } = self.0;
        ui.text(deepl_text);
    }
}
