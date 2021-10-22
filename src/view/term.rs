use std::collections::HashMap;

use ichiran::{kanji::Kanji, romanize::*, JmDictData};
use imgui::*;

use super::kanji::KanjiView;
use super::mixins::*;
use super::settings::SettingsView;
use crate::common::Env;

pub struct TermView<'a> {
    jmdict_data: &'a JmDictData,
    kanji_info: &'a HashMap<char, Kanji>,
    romaji: &'a Romanized,
    wrap_x: f32,
}
impl<'a> TermView<'a> {
    pub fn new(
        jmdict_data: &'a JmDictData,
        kanji_info: &'a HashMap<char, Kanji>,
        romaji: &'a Romanized,
        wrap_w: f32,
    ) -> Self {
        Self {
            jmdict_data,
            kanji_info,
            romaji,
            wrap_x: wrap_w,
        }
    }

    fn add_pos(&self, _env: &mut Env, ui: &Ui, pos: &str) {
        ui.text_colored([0., 1., 1., 1.], pos);
        if ui.is_item_hovered() {
            if let Some(kwpos) = self.jmdict_data.kwpos_by_kw.get(pos) {
                ui.tooltip_text(kwpos.descr.as_str());
            }
        }
    }

    fn add_glosses(&self, env: &mut Env, ui: &Ui, glosses: &[Gloss]) {
        for (i, gloss) in glosses.iter().enumerate() {
            // index
            ui.text(format!("{}.", i + 1));
            ui.same_line();
            ui.group(|| {
                // part-of-speech
                ui.text("[");
                ui.same_line_with_spacing(0.0, 0.0);
                let pos_split = gloss.pos_split();
                for (i, pos) in pos_split.iter().enumerate() {
                    self.add_pos(env, ui, pos);
                    ui.same_line_with_spacing(0.0, 0.0);
                    if i != pos_split.len() - 1 {
                        ui.text(",");
                        ui.same_line_with_spacing(0.0, 0.0);
                    }
                }
                ui.text("]");
                ui.same_line();
                // gloss
                ui.text(&format!("{}", gloss.gloss()));
                // info
                if let Some(info) = gloss.info() {
                    ui.text(&format!("({})", info));
                }
            });
        }
    }

    fn kanji_tooltip(&self, env: &mut Env, ui: &Ui, kanji: &Kanji) {
        ui.tooltip(|| KanjiView::new(kanji, 25.0).ui(env, ui));
    }

    fn add_word(
        &self,
        env: &mut Env,
        ui: &Ui,
        settings: &SettingsView,
        word: &Word,
        romaji: &str,
        show_kanji: bool,
    ) {
        let meta = word.meta();

        if show_kanji {
            {
                for chr in meta.text().chars() {
                    let kanji = self.kanji_info.get(&chr);
                    {
                        let _style_token = ui.push_style_var(StyleVar::ItemSpacing([0.0, 4.0]));
                        let text = format!("{}", chr);
                        ui.same_line();
                        draw_kanji_text(
                            ui,
                            env,
                            &text,
                            kanji != None,
                            false,
                            UnderlineMode::None,
                            RubyTextMode::None,
                        );
                    }

                    if let Some(kanji) = kanji {
                        if ui.is_item_hovered() {
                            self.kanji_tooltip(env, ui, &kanji);
                        }
                    }
                }
            }
            if meta.kana() != meta.text() {
                ui.text(format!("{}\u{ff0f}{}", meta.kana(), romaji));
            } else {
                ui.text(romaji);
            }
        }

        if let Word::Compound(compound) = word {
            ui.text(format!("Compound {}", compound.compound().join(" + ")));
        }

        match word {
            Word::Plain(plain) => {
                if let Some(suffix) = plain.suffix() {
                    ui.bullet();
                    ui.text(suffix);
                }
                // there should be no glosses if there are conjugations
                self.add_glosses(env, ui, plain.gloss());
                for conj in plain.conj() {
                    self.add_conj(env, ui, conj);
                }
                if let Some(counter) = plain.counter() {
                    ui.bullet();
                    ui.text(counter.value());
                    if counter.ordinal() {
                        ui.same_line();
                        ui.text_colored([0., 1., 1., 1.], "ordinal");
                    }
                }
            }
            Word::Compound(compound) => {
                for component in compound.components() {
                    TreeNode::new(&format!("{}", component.text()))
                        .default_open(true)
                        .build(ui, || {
                            self.add_term(env, ui, settings, component, romaji, false);
                        });
                }
            }
        }
    }

    fn add_term(
        &self,
        env: &mut Env,
        ui: &Ui,
        settings: &SettingsView,
        term: &Term,
        romaji: &str,
        show_kanji: bool,
    ) {
        match term {
            Term::Word(word) => self.add_word(env, ui, settings, word, romaji, show_kanji),
            Term::Alternative(alt) => {
                for (idx, word) in alt.alts().iter().enumerate() {
                    if idx != 0 {
                        ui.separator();
                    }
                    self.add_word(env, ui, settings, word, romaji, true);
                }
            }
        }
    }

    fn add_conj(&self, env: &mut Env, ui: &Ui, conj: &Conjugation) {
        for vias in conj.flatten() {
            let base = *vias.first().unwrap();

            if CollapsingHeader::new(&format!("{}", base.reading().unwrap_or("Conjugation")))
                .default_open(true)
                .build(ui)
            {
                {
                    let _wrap_token = ui.push_text_wrap_pos_with_pos(-1.0);
                    for via in vias {
                        if via != base {
                            ui.same_line();
                            ui.text("->");
                            ui.same_line();
                        }
                        for (idx, prop) in via.prop().iter().enumerate() {
                            if idx != 0 {
                                ui.same_line();
                                ui.text("/");
                                ui.same_line();
                            }
                            ui.text("[");
                            ui.same_line_with_spacing(0.0, 0.0);
                            self.add_pos(env, ui, prop.pos());
                            ui.same_line_with_spacing(0.0, 0.0);
                            ui.text("]");
                            ui.same_line();
                            ui.text(prop.kind());
                            if prop.neg() {
                                ui.same_line();
                                ui.text_colored([1., 0., 0., 1.], "neg");
                                if ui.is_item_hovered() {
                                    ui.tooltip_text("negative");
                                }
                            }
                            if prop.fml() {
                                ui.same_line();
                                ui.text_colored([1., 0., 1., 1.], "fml");
                                if ui.is_item_hovered() {
                                    ui.tooltip_text("formal");
                                }
                            }
                        }
                    }
                    _wrap_token.pop(ui);
                }
                self.add_glosses(env, ui, base.gloss());
            }
        }
    }

    pub fn ui(&mut self, env: &mut Env, ui: &Ui, settings: &SettingsView) {
        let _wrap_token = ui.push_text_wrap_pos_with_pos(ui.current_font_size() * self.wrap_x);
        self.add_term(
            env,
            ui,
            settings,
            self.romaji.term(),
            self.romaji.romaji(),
            true,
        );
    }
}
