use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashSet;

use ichiran::{types::*, JmDictData};
use imgui::*;

use crate::support::{Env, TextStyle};
use crate::view::RawView;

use super::SettingsView;

fn highlight_text(ui: &Ui, text: &str) {
    let sz = ui.calc_text_size(text);
    let x = ui.cursor_screen_pos()[0];
    let y = ui.cursor_screen_pos()[1];
    let draw_list = ui.get_window_draw_list();
    draw_list
        .add_rect([x, y], [x + sz[0], y + sz[1]], [1., 0., 0., 0.2])
        .filled(true)
        .build();
    ui.text(text);
}

fn wrap_line(ui: &Ui, expected_width: f32) -> bool {
    let visible_x = ui.window_pos()[0] + ui.window_content_region_max()[0];
    let last_x = ui.item_rect_max()[0];
    let style = ui.clone_style();
    let next_x = last_x + style.item_spacing[0] + expected_width;
    if next_x < visible_x {
        ui.same_line();
        false
    } else {
        true
    }
}

#[derive(Default, Deserialize, Serialize)]
pub struct RikaiView {
    root: Root,
    jmdict_data: JmDictData,
    show_term_window: RefCell<HashSet<Romanized>>,
}
impl RikaiView {
    pub fn new(root: Root, jmdict_data: JmDictData) -> Self {
        Self {
            root,
            jmdict_data,
            show_term_window: RefCell::new(HashSet::new()),
        }
    }

    fn term_window(&self, env: &mut Env, ui: &Ui, romanized: &Romanized) -> bool {
        let mut opened = true;
        Window::new(&im_str!("{}", romanized.term().text()))
            .size([300.0, 500.0], Condition::Appearing)
            .save_settings(false)
            .focus_on_appearing(true)
            .opened(&mut opened)
            .build(ui, || {
                TermView::new(&self.jmdict_data, romanized, 0.0).ui(env, ui);
            });
        opened
    }

    fn term_tooltip(&self, env: &mut Env, ui: &Ui, romanized: &Romanized) {
        ui.tooltip(|| TermView::new(&self.jmdict_data, romanized, 30.0).ui(env, ui));
    }

    fn add_skipped(&self, env: &mut Env, ui: &Ui, skipped: &str) {
        let _kanji_font_token = ui.push_font(env.get_font(TextStyle::Kanji));
        wrap_line(ui, ui.calc_text_size(skipped)[0]);
        ui.text(skipped);
    }

    fn add_romanized(&self, env: &mut Env, ui: &Ui, romanized: &Romanized) {
        let term = romanized.term();

        {
            let _kanji_font_token = ui.push_font(env.get_font(TextStyle::Kanji));
            wrap_line(ui, ui.calc_text_size(term.text())[0]);
            // draw red box behind term
            highlight_text(ui, term.text());
        }

        if ui.is_item_hovered() {
            ui.set_mouse_cursor(Some(MouseCursor::Hand));
            self.term_tooltip(env, ui, romanized);
        }

        let mut show_term_window = self.show_term_window.borrow_mut();
        if ui.is_item_clicked() {
            show_term_window.insert(romanized.clone());
        }
    }

    fn add_segment(&self, env: &mut Env, ui: &Ui, segment: &Segment) {
        match segment {
            Segment::Skipped(skipped) => {
                self.add_skipped(env, ui, skipped);
            }
            Segment::Clauses(clauses) => {
                if let Some(clause) = clauses.first() {
                    for romanized in clause.romanized() {
                        self.add_romanized(env, ui, romanized);
                    }
                }
            }
        }
    }

    fn add_root(&self, env: &mut Env, ui: &Ui, root: &Root) {
        for segment in root.segments() {
            self.add_segment(env, ui, segment);
        }
    }

    pub fn ui(&mut self, env: &mut Env, ui: &Ui, settings: &SettingsView) {
        self.add_root(env, ui, &self.root);
        if settings.show_raw {
            Window::new(im_str!("Raw"))
                .size([300., 110.], Condition::FirstUseEver)
                .build(ui, || {
                    RawView::new(&self.root).ui(env, ui);
                });
        }

        // show all term windows, close if requested (this is actually witchcraft)
        self.show_term_window
            .borrow_mut()
            .retain(|romanized| self.term_window(env, ui, romanized));
    }
}

pub struct TermView<'a> {
    jmdict_data: &'a JmDictData,
    romaji: &'a Romanized,
    wrap_x: f32,
}
impl<'a> TermView<'a> {
    pub fn new(jmdict_data: &'a JmDictData, romaji: &'a Romanized, wrap_w: f32) -> Self {
        Self {
            jmdict_data,
            romaji,
            wrap_x: wrap_w,
        }
    }

    fn add_pos(&self, env: &mut Env, ui: &Ui, pos: &str) {
        ui.text_colored([0., 1., 1., 1.], pos);
        if ui.is_item_hovered() {
            if let Some(kwpos) = self.jmdict_data.kwpos_by_kw.get(pos) {
                ui.tooltip_text(kwpos.descr.as_str());
            }
        }
    }

    fn add_glosses(&self, env: &mut Env, ui: &Ui, glosses: &Vec<Gloss>) {
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
                ui.text(&im_str!("{}", gloss.gloss()));
                // info
                if let Some(info) = gloss.info() {
                    ui.text(&im_str!("({})", info));
                }
            });
        }
    }

    fn add_word(&self, env: &mut Env, ui: &Ui, word: &Word, romaji: &str, show_kanji: bool) {
        let meta = word.meta();

        if show_kanji {
            {
                let _kanji_font_token = ui.push_font(env.get_font(TextStyle::Kanji));
                ui.text(meta.text());
            }
            if meta.kana() != meta.text() {
                ui.text_colored([0.7, 0.7, 0.7, 1.0], "[?]");
                ui.same_line();
                ui.text(meta.kana());
            }
            if ui.is_item_hovered() {
                ui.tooltip_text(romaji);
            }
        }

        if let Word::Compound(compound) = word {
            ui.text(format!("Compound {}", compound.compound().join(" + ")));
        }

        match word {
            Word::Plain(plain) => {
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
                if let Some(suffix) = plain.suffix() {
                    ui.bullet();
                    ui.text(suffix);
                }
            }
            Word::Compound(compound) => {
                for component in compound.components() {
                    TreeNode::new(&im_str!("{}", component.text()))
                        .default_open(true)
                        .build(ui, || {
                            self.add_term(env, ui, component, romaji, false);
                        });
                }
            }
        }
    }

    fn add_term(&self, env: &mut Env, ui: &Ui, term: &Term, romaji: &str, show_kanji: bool) {
        match term {
            Term::Word(word) => self.add_word(env, ui, word, romaji, show_kanji),
            Term::Alternative(alt) => {
                for word in alt.alts() {
                    if word != alt.alts().first().unwrap() {
                        ui.separator();
                    }
                    self.add_word(env, ui, word, romaji, true);
                }
            }
        }
    }

    fn add_conj(&self, env: &mut Env, ui: &Ui, conj: &Conjugation) {
        let vias = conj.flatten();
        let base = *vias.first().unwrap();

        if CollapsingHeader::new(&im_str!("{}", base.reading().unwrap_or("Conjugation")))
            .default_open(true)
            .build(ui)
        {
            for via in vias {
                if via != base {
                    ui.same_line();
                    ui.text("->");
                    ui.same_line();
                }
                for prop in via.prop() {
                    if prop != via.prop().first().unwrap() {
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
            self.add_glosses(env, ui, base.gloss());
        }
    }

    fn ui(&mut self, env: &mut Env, ui: &Ui) {
        let _body_font_token = ui.push_font(env.get_font(TextStyle::Body));
        let _wrap_token = ui.push_text_wrap_pos_with_pos(ui.current_font_size() * self.wrap_x);
        // ui.text(self.romaji.romaji());
        // ui.separator();
        self.add_term(env, ui, self.romaji.term(), self.romaji.romaji(), true);
    }
}
