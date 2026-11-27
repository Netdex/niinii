use dear_imgui_rs::*;
use ichiran::prelude::*;

use crate::gui::FontSize;

pub struct KanjiView<'a> {
    kanji: &'a Kanji,
    wrap_w: f32,
}
impl<'a> KanjiView<'a> {
    pub fn new(kanji: &'a Kanji, wrap_w: f32) -> Self {
        KanjiView { kanji, wrap_w }
    }

    fn add_kanji(&mut self, ui: &Ui, kanji: &Kanji) {
        FontSize::KANJI.with(ui, || ui.text(kanji.text()));
        ui.same_line();
        ui.group(|| {
            if let Some(freq) = kanji.freq() {
                ui.text(format!("#{}/2501 most common", freq));
            } else {
                ui.text("Uncommon");
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

    pub fn ui(&mut self, ui: &Ui) {
        let _wrap_token = ui.push_text_wrap_pos(ui.current_font_size() * self.wrap_w);
        self.add_kanji(ui, self.kanji);
    }
}
