use ichiran::types::{Alternative, Romanized, Root, Segment, Term, Word};
use imgui::*;

use crate::support::{Env, View};

pub struct Rikai<'a> {
    root: &'a Root,
}
impl<'a> Rikai<'a> {
    pub fn new(root: &'a Root) -> Self {
        Self { root }
    }

    fn add_skipped(&self, env: &mut Env, ui: &Ui, skipped: &str) {
        let font = env.fonts.get("Sarasa Mono J 40pt").unwrap();
        let font_token = ui.push_font(*font);

        self.wrap_line(ui, ui.calc_text_size(skipped)[0]);
        ui.text(skipped);

        font_token.pop();
    }

    fn add_word(&self, env: &mut Env, ui: &Ui, word: &Word) {
        let font = env.fonts.get("Sarasa Mono J 40pt").unwrap();
        let font_token = ui.push_font(*font);

        let meta = word.meta();
        self.wrap_line(ui, ui.calc_text_size(meta.text())[0]);
        let draw_list = ui.get_window_draw_list();
        ui.text(meta.text());

        draw_list
            .add_rect(ui.item_rect_min(), ui.item_rect_max(), [1., 0., 0., 1.])
            .build();
        font_token.pop();
    }

    fn wrap_line(&self, ui: &Ui, expected_w: f32) {
        let visible_x = ui.window_pos()[0] + ui.window_content_region_max()[0];
        let last_x = ui.item_rect_max()[0];
        let style = ui.clone_style();
        let next_x = last_x + style.item_spacing[0] + expected_w;
        if next_x < visible_x {
            ui.same_line();
        }
    }
}
impl<'a> View for Rikai<'a> {
    fn ui(&mut self, env: &mut Env, ui: &Ui) {
        for segment in self.root.segments() {
            match segment {
                Segment::Skipped(skipped) => {
                    self.add_skipped(env, ui, skipped);
                }
                Segment::Clauses(clauses) => {
                    if let Some(clause) = clauses.first() {
                        for romanized in clause.romanized() {
                            let term = romanized.term();
                            match term {
                                Term::Word(word) => {
                                    self.add_word(env, ui, word);
                                }
                                Term::Alternative(alternative) => {
                                    if let Some(alt) = alternative.alts().first() {
                                        self.add_word(env, ui, alt);
                                    }
                                    for alternative in alternative.alts() {}
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

pub struct HoverTerm<'a> {
    romaji: &'a Romanized,
}
impl<'a> HoverTerm<'a> {
    pub fn new(romaji: &'a Romanized) -> Self {
        Self { romaji }
    }
}
impl<'a> View for HoverTerm<'a> {
    fn ui(&mut self, env: &mut Env, ui: &Ui) {
        let term = self.romaji.term();
    }
}
