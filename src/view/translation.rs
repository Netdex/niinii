use imgui::*;

use crate::translator::{ChatGptTranslation, DeepLTranslation, Translation};

use super::mixins::stroke_text;

pub struct TranslationView<'a>(&'a Translation);
impl<'a> TranslationView<'a> {
    pub fn new(translation: &'a Translation) -> Self {
        TranslationView(translation)
    }
    pub fn ui(&self, ui: &Ui) {
        let _wrap_token = ui.push_text_wrap_pos_with_pos(0.0);
        match self.0 {
            Translation::DeepL(DeepLTranslation {
                source_text,
                deepl_text,
                deepl_usage,
            }) => {
                let draw_list = ui.get_window_draw_list();
                lang_marker(ui, &draw_list, "en");
                ui.same_line();
                stroke_text(ui, &draw_list, deepl_text, ui.cursor_screen_pos(), 1.0);
                ui.new_line();
            }
            Translation::ChatGpt(ChatGptTranslation {
                content_text,
                openai_usage,
            }) => {
                let draw_list = ui.get_window_draw_list();
                lang_marker(ui, &draw_list, "en");
                ui.same_line();
                stroke_text(ui, &draw_list, content_text, ui.cursor_screen_pos(), 1.0);
                ui.new_line();
            }
        }
    }
    pub fn show_usage(&self, ui: &Ui) {
        match self.0 {
            Translation::DeepL(DeepLTranslation {
                source_text,
                deepl_text,
                deepl_usage,
            }) => {
                ui.same_line();
                let fraction =
                    deepl_usage.character_count as f32 / deepl_usage.character_limit as f32;
                ProgressBar::new(fraction)
                    .overlay_text(format!(
                        "DeepL API usage: {}/{} ({:.2}%)",
                        deepl_usage.character_count,
                        deepl_usage.character_limit,
                        fraction * 100.0
                    ))
                    .size([350.0, 0.0])
                    .build(ui);
            }
            Translation::ChatGpt(ChatGptTranslation {
                content_text,
                openai_usage,
            }) => {
                ui.same_line();
                let fraction = openai_usage.prompt_tokens as f32 / openai_usage.total_tokens as f32;
                ProgressBar::new(fraction)
                    .overlay_text(format!(
                        "OpenAI API tokens: {}/{} ({:.2}%)",
                        openai_usage.prompt_tokens,
                        openai_usage.total_tokens,
                        fraction * 100.0
                    ))
                    .size([350.0, 0.0])
                    .build(ui);
            }
        }
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
