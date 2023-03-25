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

pub struct TranslatorView<'a>(pub &'a Translator, pub &'a mut Settings);
impl<'a> TranslatorView<'a> {
    pub fn ui(&mut self, ui: &Ui) {
        let TranslatorView(translator, settings) = self;
        translator.show_translator(ui, settings);
    }
}
#[enum_dispatch(Translator)]
trait ViewTranslator {
    fn show_translator(&self, ui: &Ui, settings: &mut Settings);
}
impl ViewTranslator for ChatGptTranslator {
    fn show_translator(&self, ui: &Ui, settings: &mut Settings) {
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
impl ViewTranslator for DeepLTranslator {
    fn show_translator(&self, _ui: &Ui, _settings: &mut Settings) {}
}

pub struct TranslationView<'a>(pub &'a Translation);
impl<'a> TranslationView<'a> {
    pub fn ui(&self, ui: &Ui) {
        self.0.view(ui);
    }
}
pub struct TranslationUsageView<'a>(pub &'a Translation);
impl<'a> TranslationUsageView<'a> {
    pub fn ui(&self, ui: &Ui) {
        self.0.show_usage(ui);
    }
}
#[enum_dispatch(Translation)]
trait ViewTranslation {
    fn view(&self, ui: &Ui);
    fn show_usage(&self, ui: &Ui);
}
impl ViewTranslation for ChatGptTranslation {
    fn view(&self, ui: &Ui) {
        let _wrap_token = ui.push_text_wrap_pos_with_pos(0.0);
        let draw_list = ui.get_window_draw_list();
        match self {
            ChatGptTranslation::Translated { content_text, .. } => {
                lang_marker(ui, &draw_list, "en");
                ui.same_line();
                stroke_text(ui, &draw_list, content_text, ui.cursor_screen_pos(), 1.0);
            }
            ChatGptTranslation::Filtered(result) => {
                lang_marker(ui, &draw_list, "n/a");
                ui.same_line();
                let s = result
                    .categories
                    .iter()
                    .filter_map(|(k, &v)| if v { Some(k.as_ref()) } else { None })
                    .collect::<Vec<_>>()
                    .join(", ");
                stroke_text(
                    ui,
                    &draw_list,
                    &format!("filtered for {}", s),
                    ui.cursor_screen_pos(),
                    1.0,
                );
            }
        }
        ui.new_line();
    }
    fn show_usage(&self, ui: &Ui) {
        match self {
            ChatGptTranslation::Translated {
                openai_usage,
                max_context_tokens,
                ..
            } => {
                ui.same_line();
                let fraction = openai_usage.prompt_tokens as f32 / *max_context_tokens as f32;
                ProgressBar::new(fraction)
                    .overlay_text(format!(
                        "ChatGPT: {}/{} prompt: {} context ({:.2}%)",
                        openai_usage.prompt_tokens,
                        openai_usage.total_tokens,
                        max_context_tokens,
                        fraction
                    ))
                    .size([350.0, 0.0])
                    .build(ui);
            }
            ChatGptTranslation::Filtered(_) => {}
        }
    }
}
impl ViewTranslation for DeepLTranslation {
    fn view(&self, ui: &Ui) {
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
