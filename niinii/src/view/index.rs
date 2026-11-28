use std::collections::HashSet;

use dear_imgui_rs::*;
use ichiran::prelude::*;

use crate::{parser::SyntaxTree, settings::Settings};

use super::term::TermView;

pub struct IndexView<'a> {
    ast: &'a SyntaxTree,
    seen_terms: HashSet<String>,
}
impl<'a> IndexView<'a> {
    pub fn new(ast: &'a SyntaxTree) -> Self {
        Self {
            ast,
            seen_terms: HashSet::new(),
        }
    }
    pub fn ui(&mut self, ui: &Ui, settings: &Settings) {
        let _wrap_token = ui.push_text_wrap_pos(0.0);
        self.add_root(ui, settings, &self.ast.root);
    }
    fn add_root(&mut self, ui: &Ui, settings: &Settings, root: &Root) {
        for segment in root.segments() {
            let _id_token = ui.push_id(segment as *const _);
            self.add_segment(ui, settings, segment);
        }
    }
    fn add_segment(&mut self, ui: &Ui, settings: &Settings, segment: &Segment) {
        match segment {
            Segment::Skipped(_) => {}
            Segment::Clauses(clauses) => {
                if let Some(clause) = clauses.first() {
                    self.add_clause(ui, settings, clause);
                }
            }
        }
    }
    fn add_clause(&mut self, ui: &Ui, settings: &Settings, clause: &Clause) {
        for romanized in clause.romanized() {
            let _id_token = ui.push_id(romanized as *const _);
            if !romanized.term().text().chars().any(|c| is_kanji(&c)) {
                continue;
            }
            if let Word::Plain(word) = romanized.term().best() {
                if word.counter().is_some() {
                    continue;
                }
            }
            if self.seen_terms.contains(romanized.term().text()) {
                continue;
            }
            self.seen_terms.insert(romanized.term().text().to_string());
            TermView::new(&self.ast.jmdict_data, &self.ast.kanji_info, romanized, 0.0)
                .ui(ui, settings);
            ui.separator()
        }
    }
}
