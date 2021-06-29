use eframe::egui::{Label, Response, TextStyle, Ui, Widget};
use ichiran::types::{Romanized, Root, Segment, Term};

use crate::View;

pub struct Rikai<'a> {
    root: &'a Root,
}
impl<'a> Rikai<'a> {
    pub fn new(root: &'a Root) -> Self {
        Self { root }
    }
}

impl<'a> View for Rikai<'a> {
    fn ui(&mut self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            for segment in self.root.segments() {
                match segment {
                    Segment::Skipped(s) => {
                        ui.label(s);
                    }
                    Segment::Clauses(clauses) => {
                        if let Some(clause) = clauses.first() {
                            for romanized in clause.romanized() {
                                HoverTerm::new(romanized).ui(ui);
                            }
                        }
                    }
                }
            }
        });
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
impl<'a> Widget for HoverTerm<'a> {
    fn ui(self, ui: &mut Ui) -> Response {
        let term = self.romaji.term();
        match term {
            Term::Word(word) => {
                let meta = word.meta();
                ui.add(Label::new(meta.text()).heading())
            }
            Term::Alternative(alternative) => ui.heading("TODO"),
        }
    }
}
