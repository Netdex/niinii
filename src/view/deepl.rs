use imgui::*;

use crate::translation::Translation;

use super::mixins::stroke_text;

pub struct DeepLView<'a>(&'a Translation);
impl<'a> DeepLView<'a> {
    pub fn new(translation: &'a Translation) -> Self {
        DeepLView(translation)
    }
    pub fn ui(&self, ui: &Ui) {
        let _wrap_token = ui.push_text_wrap_pos_with_pos(0.0);
        let Translation::DeepL { deepl_text, .. } = self.0;

        let draw_list = ui.get_window_draw_list();
        lang_marker(ui, &draw_list, "en");
        ui.same_line();
        stroke_text(ui, &draw_list, deepl_text, ui.cursor_screen_pos(), 1.0);
        ui.new_line();
    }
}

fn lang_marker(ui: &Ui, draw_list: &DrawListMut, lang: impl AsRef<str>) {
    let lang = lang.as_ref();
    let text = format!("[{}]", lang);
    let p = ui.cursor_screen_pos();
    let sz = ui.calc_text_size(&text);
    draw_list
        .add_rect(
            p,
            [p[0] + sz[0], p[1] + sz[1]],
            ui.style_color(StyleColor::TextSelectedBg),
        )
        .filled(true)
        .build();

    stroke_text(ui, draw_list, &text, ui.cursor_screen_pos(), 1.0);
    ui.dummy(sz);
}
