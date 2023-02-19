use ichiran::kanji::Kanji;
use imgui::*;

use crate::backend::context::{Context, TextStyle};

pub struct KanjiView<'a> {
    kanji: &'a Kanji,
    wrap_x: f32,
}
impl<'a> KanjiView<'a> {
    pub fn new(kanji: &'a Kanji, wrap_x: f32) -> Self {
        KanjiView { kanji, wrap_x }
    }

    fn add_kanji(&mut self, ctx: &mut Context, ui: &Ui, kanji: &Kanji) {
        {
            let _kanji_font_token = ui.push_font(ctx.get_font(TextStyle::Kanji));
            ui.text(kanji.text());
        }
        ui.same_line();
        ui.group(|| {
            if let Some(freq) = kanji.freq() {
                ui.text(format!("#{}/2501 most common", freq));
            } else {
                ui.text("Uncommon".to_string());
            }
            ui.same_line();
            ui.text(format!(
                "({} stroke{})",
                kanji.stroke_count(),
                if kanji.stroke_count() != 1 { "s" } else { "" }
            ));
            ui.text(kanji.grade_desc());
        });
        ui.bullet();
        ui.same_line();
        ui.text_wrapped(kanji.meanings().join(", "));

        if let Some(_t) = ui.begin_table_header(
            "readings",
            [
                TableColumnSetup::new("Type"),
                TableColumnSetup::new("Kana"),
                TableColumnSetup::new("Romaji"),
                TableColumnSetup::new("Okuri"),
                TableColumnSetup::new("Frequency"),
            ],
        ) {
            for reading in kanji.readings() {
                ui.table_next_column();
                ui.text(format!("{}", reading.rtype()));

                ui.table_next_column();
                let mut kana = reading.kana().to_owned();
                if reading.prefix() {
                    kana.push('-')
                }
                if reading.suffix() {
                    kana.insert(0, '-')
                }
                ui.text(kana);

                ui.table_next_column();
                let mut romaji = reading.romaji().to_owned();
                if reading.prefix() {
                    romaji.push('-')
                }
                if reading.suffix() {
                    romaji.insert(0, '-')
                }
                ui.text(romaji);

                ui.table_next_column();
                ui.text_wrapped(reading.okuri().join(","));

                ui.table_next_column();
                ui.text(format!("{:.2}%", reading.usage_percentage()));
            }
            ui.table_next_column();
            ui.text("Irr.");

            ui.table_next_column();
            ui.table_next_column();
            ui.table_next_column();
            ui.table_next_column();
            ui.text(format!("{:.2}%", kanji.irregular_percentage()));
        }
    }

    pub fn ui(&mut self, ctx: &mut Context, ui: &Ui) {
        let _wrap_token = ui.push_text_wrap_pos_with_pos(ui.current_font_size() * self.wrap_x);
        self.add_kanji(ctx, ui, self.kanji);
    }
}
