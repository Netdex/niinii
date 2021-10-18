use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use ichiran::{kanji::Kanji, romanize::*, JmDictData};
use imgui::*;

use super::mixins::*;
use super::settings::{DisplayRubyText, SettingsView};
use crate::common::Env;
use crate::view::{raw::RawView, term::TermView};

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct RikaiView {
    root: Root,
    kanji_info: HashMap<char, Kanji>,
    jmdict_data: JmDictData,
    show_term_window: RefCell<HashSet<Romanized>>,
    selected_clause: RefCell<HashMap<Segment, i32>>,
}
impl RikaiView {
    pub fn new(root: Root, kanji_info: HashMap<char, Kanji>, jmdict_data: JmDictData) -> Self {
        Self {
            root,
            kanji_info,
            jmdict_data,
            show_term_window: RefCell::new(HashSet::new()),
            selected_clause: RefCell::new(HashMap::new()),
        }
    }

    fn term_window(
        &self,
        env: &mut Env,
        ui: &Ui,
        settings: &SettingsView,
        romanized: &Romanized,
    ) -> bool {
        let mut opened = true;
        Window::new(&format!("{}", romanized.term().text()))
            .size_constraints([300.0, 100.0], [1000.0, 1000.0])
            .save_settings(false)
            .focus_on_appearing(true)
            .opened(&mut opened)
            .build(ui, || {
                TermView::new(&self.jmdict_data, &self.kanji_info, romanized, 0.0)
                    .ui(env, ui, settings);
            });
        opened
    }

    fn term_tooltip(&self, env: &mut Env, ui: &Ui, settings: &SettingsView, romanized: &Romanized) {
        ui.tooltip(|| {
            TermView::new(&self.jmdict_data, &self.kanji_info, romanized, 30.0)
                .ui(env, ui, settings)
        });
    }

    fn add_skipped(&self, env: &mut Env, ui: &Ui, settings: &SettingsView, skipped: &str) {
        draw_kanji_text(
            ui,
            env,
            skipped,
            false,
            UnderlineMode::None,
            if settings.display_ruby_text() == DisplayRubyText::None {
                RubyTextMode::None
            } else {
                RubyTextMode::Pad
            },
        );
    }

    fn add_romanized(
        &self,
        env: &mut Env,
        ui: &Ui,
        settings: &SettingsView,
        romanized: &Romanized,
        ruby_text: DisplayRubyText,
        underline: UnderlineMode,
    ) -> bool {
        let term = romanized.term();

        let fg_text = match ruby_text {
            DisplayRubyText::None => RubyTextMode::None,
            DisplayRubyText::Furigana => RubyTextMode::Text(term.kana()),
            DisplayRubyText::Romaji => RubyTextMode::Text(romanized.romaji()),
        };
        let ul_hover = draw_kanji_text(ui, env, term.text(), true, underline, fg_text);

        if ui.is_item_hovered() {
            ui.set_mouse_cursor(Some(MouseCursor::Hand));
            self.term_tooltip(env, ui, settings, romanized);
        }

        let mut show_term_window = self.show_term_window.borrow_mut();
        if ui.is_item_clicked() {
            show_term_window.insert(romanized.clone());
        }

        ul_hover
    }

    fn add_segment(&self, env: &mut Env, ui: &Ui, settings: &SettingsView, segment: &Segment) {
        match segment {
            Segment::Skipped(skipped) => {
                self.add_skipped(env, ui, settings, skipped);
            }
            Segment::Clauses(clauses) => {
                let mut selected_clause = self.selected_clause.borrow_mut();
                let mut clause_idx = selected_clause.get(segment).cloned().unwrap_or(0);

                let clause = clauses.get(clause_idx as usize);
                if let Some(clause) = clause {
                    let romanized = clause.romanized();
                    for (idx, rz) in romanized.iter().enumerate() {
                        let ul_hover = self.add_romanized(
                            env,
                            ui,
                            settings,
                            rz,
                            settings.display_ruby_text(),
                            match settings.show_variant_switcher {
                                true if idx == romanized.len() - 1 => UnderlineMode::Normal,
                                true => UnderlineMode::Pad,
                                false => UnderlineMode::None,
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
                                    "Variant #{}/{} score={} (scroll to cycle)",
                                    clause_idx + 1,
                                    clauses.len(),
                                    clause.score()
                                ));
                                ui.separator();
                                let _wrap_token =
                                    ui.push_text_wrap_pos_with_pos(ui.current_font_size() * 20.0);
                                let romaji = clause
                                    .romanized()
                                    .iter()
                                    .map(|x| x.romaji())
                                    .collect::<Vec<&str>>()
                                    .join(" ");
                                ui.text_wrapped(romaji);
                                _wrap_token.pop(ui);
                            });
                        }
                    }
                }
            }
        }
    }

    fn add_root(&self, env: &mut Env, ui: &Ui, settings: &SettingsView, root: &Root) {
        for segment in root.segments() {
            self.add_segment(env, ui, settings, segment);
        }
    }

    pub fn ui(&mut self, env: &mut Env, ui: &Ui, settings: &SettingsView, show_raw: &mut bool) {
        self.add_root(env, ui, settings, &self.root);
        if *show_raw {
            Window::new("Raw")
                .size([300., 110.], Condition::FirstUseEver)
                .opened(show_raw)
                .build(ui, || {
                    RawView::new(&self.root).ui(env, ui);
                });
        }

        // show all term windows, close if requested (this is actually witchcraft)
        self.show_term_window
            .borrow_mut()
            .retain(|romanized| self.term_window(env, ui, settings, romanized));
    }
}
