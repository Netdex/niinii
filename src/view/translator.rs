use enum_dispatch::enum_dispatch;
use imgui::*;

use crate::{
    settings::Settings,
    translator::{
        chatgpt::{ChatGptTranslation, ChatGptTranslator},
        deepl::{DeepLTranslation, DeepLTranslator},
        Translation, Translator,
    },
};

use super::mixins::{stroke_text, stroke_token_with_color};

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
        let mut context = self.context.lock().unwrap();
        if ui.button("Clear context") {
            context.clear();
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
                [ui.content_region_avail()[0], 200.0],
            )
            .build();
            for message in context.iter() {
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
        match self {
            ChatGptTranslation::Translated { context, .. } => {
                let context = context.lock().unwrap();

                let draw_list = ui.get_window_draw_list();
                marker(
                    ui,
                    &draw_list,
                    "ChatGPT",
                    &ui.style_color(StyleColor::TextSelectedBg),
                );
                ui.same_line();

                if let Some(message) = context.back() {
                    stroke_text(ui, &draw_list, &message.content, 1.0);
                }
            }
            ChatGptTranslation::Filtered { moderation } => {
                let draw_list = ui.get_window_draw_list();
                for k in moderation
                    .categories
                    .iter()
                    .filter_map(|(k, &v)| if v { Some(k) } else { None })
                {
                    marker(
                        ui,
                        &draw_list,
                        k.as_ref(),
                        &ui.style_color(StyleColor::PlotLinesHovered),
                    );
                    if ui.is_item_hovered() {
                        ui.tooltip_text(format!("{:.1}%", moderation.category_scores[k] * 100.0))
                    }
                }
                ui.same_line();
                drop(draw_list);
            }
        }
    }
    fn show_usage(&self, _ui: &Ui) {
        // TODO: the streaming API doesn't have a usage field...
        // match self {
        //     ChatGptTranslation::Translated {
        //         openai_usage,
        //         max_context_tokens,
        //         ..
        //     } => {
        //         ui.same_line();
        //         let fraction = openai_usage.prompt_tokens as f32 / *max_context_tokens as f32;
        //         ProgressBar::new(fraction)
        //             .overlay_text(format!(
        //                 "ChatGPT: {}/{} prompt: {} context ({:.2}%)",
        //                 openai_usage.prompt_tokens,
        //                 openai_usage.total_tokens,
        //                 max_context_tokens,
        //                 fraction
        //             ))
        //             .size([350.0, 0.0])
        //             .build(ui);
        //     }
        //     _ => {}
        // }
    }
}
impl ViewTranslation for DeepLTranslation {
    fn view(&self, ui: &Ui) {
        let _wrap_token = ui.push_text_wrap_pos_with_pos(0.0);
        let draw_list = ui.get_window_draw_list();
        marker(
            ui,
            &draw_list,
            "DeepL",
            &ui.style_color(StyleColor::TextSelectedBg),
        );
        ui.same_line();
        stroke_text(ui, &draw_list, &self.deepl_text, 1.0);
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

fn marker(ui: &Ui, draw_list: &DrawListMut, text: impl AsRef<str>, color: &[f32; 4]) {
    let lang = text.as_ref();
    let text = format!("[{}]", lang);
    let p = ui.cursor_screen_pos();
    let sz = ui.calc_text_size(&text);
    draw_list
        .add_rect(p, [p[0] + sz[0], p[1] + sz[1]], *color)
        .filled(true)
        .build();
    stroke_token_with_color(
        ui,
        draw_list,
        &text,
        p,
        1.0,
        StyleColor::Text,
        StyleColor::TitleBg,
    );
    ui.dummy(sz);
}
