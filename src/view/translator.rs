use enum_dispatch::enum_dispatch;
use imgui::*;

use crate::{
    settings::Settings,
    translator::{
        ChatGptTranslation, ChatGptTranslator, DeepLTranslation, DeepLTranslator, Translation,
        Translator,
    },
};

use super::mixins::stroke_text;

#[enum_dispatch(Translator)]
pub trait TranslatorView {
    fn ui(&mut self, ui: &Ui, settings: &mut Settings);
}

impl TranslatorView for ChatGptTranslator {
    fn ui(&mut self, ui: &Ui, settings: &mut Settings) {
        let mut state = self.shared.state.blocking_lock();
        if ui.button("Clear context") {
            state.context.clear();
        }
        if let Some(_t) = ui.begin_table_header_with_flags(
            "context",
            [
                TableColumnSetup::new("Role"),
                TableColumnSetup::new("Message"),
            ],
            TableFlags::SIZING_STRETCH_PROP,
        ) {
            ui.table_next_column();
            ui.text("System");
            ui.table_next_column();
            ui.input_text_multiline(
                "##",
                &mut settings.chatgpt_system_prompt,
                [ui.content_region_avail()[0], 75.0],
            )
            .build();
            for message in &state.context {
                ui.table_next_column();
                ui.text(format!("{:?}", message.role));
                ui.table_next_column();
                ui.text_wrapped(&message.content);
            }
        }
    }
}
impl TranslatorView for DeepLTranslator {
    fn ui(&mut self, _ui: &Ui, _settings: &mut Settings) {}
}

#[enum_dispatch(Translation)]
pub trait TranslationView {
    fn ui(&self, ui: &Ui);
    fn show_usage(&self, ui: &Ui);
}

impl TranslationView for ChatGptTranslation {
    fn ui(&self, ui: &Ui) {
        let _wrap_token = ui.push_text_wrap_pos_with_pos(0.0);
        let draw_list = ui.get_window_draw_list();
        lang_marker(ui, &draw_list, "en");
        ui.same_line();
        stroke_text(
            ui,
            &draw_list,
            &self.content_text,
            ui.cursor_screen_pos(),
            1.0,
        );
        ui.new_line();
    }
    fn show_usage(&self, ui: &Ui) {
        ui.same_line();
        let fraction = self.openai_usage.prompt_tokens as f32 / self.max_context_tokens as f32;
        ProgressBar::new(fraction)
            .overlay_text(format!(
                "ChatGPT: {}/{} prompt: {} context ({:.2}%)",
                self.openai_usage.prompt_tokens,
                self.openai_usage.total_tokens,
                self.max_context_tokens,
                fraction
            ))
            .size([350.0, 0.0])
            .build(ui);
    }
}
impl TranslationView for DeepLTranslation {
    fn ui(&self, ui: &Ui) {
        let _wrap_token = ui.push_text_wrap_pos_with_pos(0.0);
        let draw_list = ui.get_window_draw_list();
        lang_marker(ui, &draw_list, "en");
        ui.same_line();
        stroke_text(
            ui,
            &draw_list,
            &self.deepl_text,
            ui.cursor_screen_pos(),
            1.0,
        );
        ui.new_line();
    }
    fn show_usage(&self, ui: &Ui) {
        ui.same_line();
        let fraction =
            self.deepl_usage.character_count as f32 / self.deepl_usage.character_limit as f32;
        ProgressBar::new(fraction)
            .overlay_text(format!(
                "DeepL API usage: {}/{} ({:.2}%)",
                self.deepl_usage.character_count,
                self.deepl_usage.character_limit,
                fraction * 100.0
            ))
            .size([350.0, 0.0])
            .build(ui);
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
