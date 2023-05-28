use std::collections::HashSet;

use ichiran::{charset::is_kanji, romanize::*};
use imgui::*;

use crate::{parser::SyntaxTree, renderer::context::Context, settings::Settings};

use super::{id, term::TermView};

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
    pub fn ui(&mut self, ctx: &mut Context, ui: &Ui, settings: &Settings) {
        let _wrap_token = ui.push_text_wrap_pos();
        self.add_root(ctx, ui, settings, &self.ast.root);
    }
    fn add_root(&mut self, ctx: &mut Context, ui: &Ui, settings: &Settings, root: &Root) {
        for segment in root.segments() {
            let _id_token = ui.push_id_ptr(&id(segment));
            self.add_segment(ctx, ui, settings, segment);
        }
    }
    fn add_segment(&mut self, ctx: &mut Context, ui: &Ui, settings: &Settings, segment: &Segment) {
        match segment {
            Segment::Skipped(_) => {}
            Segment::Clauses(clauses) => {
                if let Some(clause) = clauses.first() {
                    self.add_clause(ctx, ui, settings, clause);
                }
            }
        }
    }
    fn add_clause(&mut self, ctx: &mut Context, ui: &Ui, settings: &Settings, clause: &Clause) {
        for (_idx, romanized) in clause.romanized().iter().enumerate() {
            let _id_token = ui.push_id_ptr(&id(romanized));
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
                .ui(ctx, ui, settings);
            ui.separator()
        }
    }
}
