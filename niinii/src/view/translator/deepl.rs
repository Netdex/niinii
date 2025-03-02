use imgui::*;

use crate::translator::deepl::DeepLTranslation;

use crate::view::{
    mixins::{stroke_text, stroke_text_with_highlight},
    View,
};

pub struct ViewDeepLTranslator;
impl View for ViewDeepLTranslator {
    fn ui(&mut self, _ui: &imgui::Ui) {}
}

pub struct ViewDeepLTranslation<'a>(pub &'a DeepLTranslation);
impl View for ViewDeepLTranslation<'_> {
    fn ui(&mut self, ui: &imgui::Ui) {
        let _wrap_token = ui.push_text_wrap_pos_with_pos(0.0);
        ui.text(""); // anchor for line wrapping
        let draw_list = ui.get_window_draw_list();
        stroke_text_with_highlight(
            ui,
            &draw_list,
            "[DeepL]",
            1.0,
            Some(StyleColor::TextSelectedBg),
        );
        ui.same_line();
        stroke_text(ui, &draw_list, &self.0.deepl_text, 1.0);
    }
}

pub struct ViewDeepLTranslationUsage<'a>(pub &'a DeepLTranslation);
impl View for ViewDeepLTranslationUsage<'_> {
    fn ui(&mut self, ui: &imgui::Ui) {
        let deepl_usage = &self.0.deepl_usage;
        ui.same_line();
        let fraction = deepl_usage.character_count as f32 / deepl_usage.character_limit as f32;
        ProgressBar::new(fraction)
            .overlay_text(format!(
                "usage: {}/{} ({:.2}%)",
                deepl_usage.character_count,
                deepl_usage.character_limit,
                fraction * 100.0
            ))
            .size([350.0, 0.0])
            .build(ui);
    }
}
