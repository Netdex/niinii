use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use ichiran::prelude::*;
use imgui::*;

use super::index::IndexView;
use super::mixins::*;
use super::translator::TranslationView;
use crate::parser::SyntaxTree;
use crate::renderer::context::Context;
use crate::settings::{RubyTextType, Settings};
use crate::translator::Translation;
use crate::view::{raw::RawView, term::TermView};

enum View {
    Text(String),
    Interpret {
        ast: SyntaxTree,
        translation: Option<Translation>,
        translation_pending: bool,
    },
}

pub struct GlossView {
    view: Option<View>,
    show_term_window: RefCell<HashSet<Romanized>>,
    selected_clause: RefCell<HashMap<Segment, i32>>,
    show_raw: bool,
    show_glossary: bool,
}
impl GlossView {
    pub fn new() -> Self {
        Self {
            view: None,
            show_term_window: RefCell::new(HashSet::new()),
            selected_clause: RefCell::new(HashMap::new()),
            show_raw: false,
            show_glossary: false,
        }
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.view = Some(View::Text(text.into()));
    }

    pub fn set_ast(&mut self, ast: SyntaxTree) {
        self.view = Some(View::Interpret {
            ast,
            translation: None,
            translation_pending: false,
        });
    }
    pub fn ast(&self) -> Option<&SyntaxTree> {
        if let Some(View::Interpret { ast, .. }) = &self.view {
            Some(ast)
        } else {
            None
        }
    }

    pub fn set_translation_pending(&mut self, pending: bool) {
        if let Some(View::Interpret {
            translation_pending,
            ..
        }) = &mut self.view
        {
            *translation_pending = pending;
        }
    }
    pub fn set_translation(&mut self, tl: Option<Translation>) {
        if let Some(View::Interpret { translation, .. }) = &mut self.view {
            *translation = tl;
        }
    }
    pub fn translation(&self) -> Option<&Translation> {
        if let Some(View::Interpret { translation, .. }) = &self.view {
            translation.as_ref()
        } else {
            None
        }
    }

    fn term_window(
        &self,
        ctx: &mut Context,
        ui: &Ui,
        settings: &Settings,
        romanized: &Romanized,
    ) -> bool {
        let mut opened = true;
        ui.window(&romanized.term().text().to_string())
            .size_constraints([300.0, 100.0], [1000.0, 1000.0])
            .save_settings(false)
            .focus_on_appearing(true)
            .opened(&mut opened)
            .build(|| {
                if let Some(View::Interpret { ast: gloss, .. }) = &self.view {
                    TermView::new(&gloss.jmdict_data, &gloss.kanji_info, romanized, 0.0)
                        .ui(ctx, ui, settings);
                }
            });
        opened
    }

    fn term_tooltip(&self, ctx: &mut Context, ui: &Ui, settings: &Settings, romanized: &Romanized) {
        ui.tooltip(|| {
            if let Some(View::Interpret { ast: gloss, .. }) = &self.view {
                TermView::new(&gloss.jmdict_data, &gloss.kanji_info, romanized, 30.0)
                    .ui(ctx, ui, settings)
            }
        });
    }

    fn add_skipped(
        &self,
        ctx: &mut Context,
        ui: &Ui,
        settings: &Settings,
        skipped: &str,
        preview: bool,
    ) {
        draw_kanji_text(
            ui,
            ctx,
            skipped,
            false,
            !preview,
            preview,
            UnderlineMode::None,
            if settings.ruby_text_type == RubyTextType::None {
                RubyTextMode::None
            } else {
                RubyTextMode::Pad
            },
        );
    }

    fn add_romanized(
        &self,
        ctx: &mut Context,
        ui: &Ui,
        settings: &Settings,
        romanized: &Romanized,
        ruby_text: RubyTextType,
        underline: UnderlineMode,
    ) -> bool {
        let term = romanized.term();

        let fg_text = match ruby_text {
            RubyTextType::None => RubyTextMode::None,
            RubyTextType::Furigana if term.text() != term.kana() => RubyTextMode::Text(term.kana()),
            RubyTextType::Romaji => RubyTextMode::Text(romanized.romaji()),
            _ => RubyTextMode::Pad,
        };
        let ul_hover = draw_kanji_text(
            ui,
            ctx,
            term.text(),
            true,
            settings.stroke_text,
            false,
            underline,
            fg_text,
        );

        if ui.is_item_hovered() {
            ui.set_mouse_cursor(Some(MouseCursor::Hand));
            self.term_tooltip(ctx, ui, settings, romanized);
        }

        let mut show_term_window = self.show_term_window.borrow_mut();
        if ui.is_item_clicked() {
            show_term_window.insert(romanized.clone());
        }

        ul_hover
    }

    fn add_segment(&self, ctx: &mut Context, ui: &Ui, settings: &Settings, segment: &Segment) {
        match segment {
            Segment::Skipped(skipped) => {
                self.add_skipped(ctx, ui, settings, skipped, false);
            }
            Segment::Clauses(clauses) => {
                let mut selected_clause = self.selected_clause.borrow_mut();
                let mut clause_idx = selected_clause.get(segment).cloned().unwrap_or(0);

                let clause = clauses.get(clause_idx as usize);
                if let Some(clause) = clause {
                    if clause.score() > 0 {
                        let romanized = clause.romanized();
                        for (idx, rz) in romanized.iter().enumerate() {
                            let ul_hover = self.add_romanized(
                                ctx,
                                ui,
                                settings,
                                rz,
                                settings.ruby_text_type,
                                if idx == romanized.len() - 1 {
                                    UnderlineMode::Normal
                                } else {
                                    UnderlineMode::Pad
                                },
                            );
                            if ul_hover {
                                let scroll = ui.io().mouse_wheel as i32;
                                clause_idx -= scroll;
                                clause_idx = clause_idx.clamp(0, clauses.len() as i32 - 1);
                                if scroll != 0 {
                                    selected_clause.insert(segment.clone(), clause_idx);
                                }
                                ui.tooltip(|| {
                                    ui.text(format!(
                                        "Alternate #{}/{} score={} (scroll to cycle)",
                                        clause_idx + 1,
                                        clauses.len(),
                                        clause.score()
                                    ));
                                    ui.separator();
                                    let _wrap_token = ui
                                        .push_text_wrap_pos_with_pos(ui.current_font_size() * 20.0);
                                    let romaji = clause
                                        .romanized()
                                        .iter()
                                        .map(|x| x.romaji())
                                        .collect::<Vec<&str>>()
                                        .join(" ");
                                    ui.text_wrapped(romaji);
                                });
                            }
                        }
                    } else {
                        self.add_skipped(ctx, ui, settings, &clause.text(), false);
                    }
                }
            }
        }
    }

    fn add_root(&self, ctx: &mut Context, ui: &Ui, settings: &Settings, root: &Root) {
        for segment in root.segments() {
            self.add_segment(ctx, ui, settings, segment);
        }
    }

    pub fn ui(&mut self, ctx: &mut Context, ui: &Ui, settings: &Settings) {
        ui.text(""); // hack to align line position
        match &self.view {
            Some(View::Interpret {
                ast: gloss,
                translation,
                translation_pending,
            }) => {
                self.add_root(ctx, ui, settings, &gloss.root);

                if self.show_raw {
                    ui.window("Raw")
                        .size([300., 110.], Condition::FirstUseEver)
                        .opened(&mut self.show_raw)
                        .build(|| {
                            RawView::new(&gloss.root).ui(ctx, ui);
                        });
                }
                if self.show_glossary {
                    ui.window("Glossary")
                        .size([300., 110.], Condition::FirstUseEver)
                        .opened(&mut self.show_glossary)
                        .build(|| {
                            IndexView::new(gloss).ui(ctx, ui, settings);
                        });
                }
                ui.new_line();
                if let Some(translation) = translation {
                    TranslationView(translation).ui(ui);
                } else if *translation_pending {
                    ui.text_disabled("(waiting for translation...)")
                }
            }
            Some(View::Text(text)) => {
                self.add_skipped(ctx, ui, settings, text, true);
            }
            _ => {}
        }

        // show all term windows, close if requested (this is actually witchcraft)
        self.show_term_window
            .borrow_mut()
            .retain(|romanized| self.term_window(ctx, ui, settings, romanized));
    }

    pub fn show_menu(&mut self, _ctx: &mut Context, ui: &Ui) {
        if ui.menu_item_config("Raw").selected(self.show_raw).build() {
            self.show_raw = true;
        }
        if ui
            .menu_item_config("Glossary")
            .selected(self.show_glossary)
            .build()
        {
            self.show_glossary = true;
        }
    }
}
