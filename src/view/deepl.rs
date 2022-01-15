use imgui::*;

use crate::translation::Translation;

pub struct DeepLView<'a>(&'a Translation);
impl<'a> DeepLView<'a> {
    pub fn new(translation: &'a Translation) -> Self {
        DeepLView(translation)
    }
    pub fn ui(&self, ui: &Ui) {
        let _wrap_token = ui.push_text_wrap_pos_with_pos(0.0);
        let Translation::DeepL {
            source_text,
            deepl_text,
            ..
        } = self.0;
        ui.separator();
        lang_marker(ui, "ja");
        ui.same_line();
        ui.text(source_text);
        ui.separator();
        lang_marker(ui, "en");
        ui.same_line();
        ui.text(deepl_text);
        ui.separator();
    }
}

fn lang_marker<T: AsRef<str>>(ui: &Ui, lang: T) {
    let lang = lang.as_ref();
    ui.text("[");
    ui.same_line_with_spacing(0.0, 0.0);
    ui.text_colored([0.0, 1.0, 1.0, 1.0], lang);
    ui.same_line_with_spacing(0.0, 0.0);
    ui.text("]");
}
