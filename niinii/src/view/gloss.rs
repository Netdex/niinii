use std::{
    collections::{hash_map::DefaultHasher, HashMap},
    hash::{Hash, Hasher},
};

use dear_imgui_rs::*;
use ichiran::prelude::*;

use super::{
    index::IndexView,
    mixins::*,
    panel::{Panel, Sizing, TextSize},
    raw::RawView,
    term::TermView,
};
use crate::{
    gui::px,
    history::{Gloss, History},
    parser::SyntaxTree,
    settings::{RubyTextType, Settings},
};

/// Highlight color for terms that came from an injected custom entry
/// (`Term::is_custom()`). Distinct enough from the default
/// `TextSelectedBg` to be obvious at a glance.
const NAME_HIGHLIGHT: [f32; 4] = [0.85, 0.55, 0.20, 0.55];

/// Senses per gloss list in the hover tooltip.
const TOOLTIP_SENSES: usize = 3;

/// Returned from `show_input` so the caller can dispatch. The input row owns
/// both the Gloss and Translate buttons so the UI stays together.
pub enum GlossInputAction {
    Gloss(String),
    Translate(String),
}

const DETAIL_SIZING: Sizing = Sizing {
    default: Some(TextSize::new(33.0, 5.0)),
    min: TextSize::new(24.0, 4.0),
};

/// View state around the line being shown: the manual input box, the word
/// windows the user opened, and the Raw and Glossary windows.
pub struct GlossView {
    input_text: String,
    term_windows: HashMap<Romanized, WindowKey>,
    raw: Panel,
    glossary: Panel,
}

impl Default for GlossView {
    fn default() -> Self {
        Self::new()
    }
}

impl GlossView {
    pub fn new() -> Self {
        Self {
            input_text: String::new(),
            term_windows: HashMap::new(),
            raw: Panel::new("Raw", DETAIL_SIZING),
            glossary: Panel::new("Glossary", DETAIL_SIZING),
        }
    }

    pub fn set_input(&mut self, text: &str) {
        text.clone_into(&mut self.input_text);
    }

    pub fn show_menu(&mut self, ui: &Ui) {
        self.raw.menu_item(ui);
        self.glossary.menu_item(ui);
    }

    /// Render the manual-input row: textarea, Gloss button, Translate button.
    /// Leaves the cursor on the same line so the caller can append adjacent
    /// controls (e.g. a usage bar). Returns the action the user triggered.
    pub fn show_input(&mut self, ui: &Ui, history: &History) -> Option<GlossInputAction> {
        let mut action = None;
        {
            let _disabled = ui.begin_disabled_with_cond(history.is_processing());
            let entered = ui
                .input_text_multiline("##", &mut self.input_text, [0.0, px(ui, 50.0)])
                .flags(
                    InputTextMultilineFlags::ENTER_RETURNS_TRUE
                        | InputTextMultilineFlags::WORD_WRAP,
                )
                .build();
            let clicked = ui.button_with_size("Gloss", [px(ui, 120.0), 0.0]);
            if entered || clicked {
                action = Some(GlossInputAction::Gloss(self.input_text.clone()));
            }
        }
        ui.same_line();

        let enable_tl = history.ast().is_some_and(|ast| !ast.empty());
        {
            let _disabled = ui.begin_disabled_with_cond(!enable_tl);
            if ui.button_with_size("Translate", [px(ui, 120.0), 0.0]) {
                if let Some(ast) = history.ast() {
                    action = Some(GlossInputAction::Translate(ast.original_text.clone()));
                }
            }
        }
        if !enable_tl {
            ui.set_item_tooltip("Text does not require translation");
        }
        action
    }

    /// The line being shown, glossed, and the windows opened from it.
    pub fn ui(&mut self, ui: &Ui, settings: &Settings, history: &mut History) {
        let earlier = history.showing_earlier_line();
        let Some(line) = history.current_mut() else {
            return;
        };
        {
            let mut draw = LineDraw {
                ui,
                settings,
                highlight: if earlier {
                    earlier_line_highlight(ui)
                } else {
                    ui.style_color(StyleColor::TextSelectedBg)
                },
                term_windows: &mut self.term_windows,
                flow: Flow::new(ui),
            };
            match &line.gloss {
                Gloss::Parsed(ast) => {
                    for (idx, segment) in ast.root.segments().iter().enumerate() {
                        draw.segment(ast, idx, segment, &mut line.selected_clause);
                    }
                }
                Gloss::Preview(splits) => {
                    for (kind, chunk) in splits {
                        match kind {
                            Split::Text => draw.preview(chunk),
                            Split::Skip => draw.skipped(chunk, Ink::Preview),
                        }
                    }
                }
            }
        }
        ui.new_line();

        let Some(ast) = line.ast() else {
            return;
        };
        self.raw
            .show(ui, WindowFlags::empty(), || RawView::new(&ast.root).ui(ui));
        self.glossary.show(ui, WindowFlags::empty(), || {
            IndexView::new(ast).ui(ui, settings)
        });
        self.term_windows
            .retain(|romanized, key| term_window(ui, settings, ast, romanized, key));
    }
}

/// Draws one line's text.
struct LineDraw<'a> {
    ui: &'a Ui,
    settings: &'a Settings,
    /// Box behind glossed words; gray when the line is not the latest.
    highlight: [f32; 4],
    term_windows: &'a mut HashMap<Romanized, WindowKey>,
    flow: Flow,
}

impl LineDraw<'_> {
    /// Ruby text above a word, or space for it so lines stay aligned.
    fn ruby<'r>(&self, romanized: Option<&'r Romanized>) -> RubyTextMode<'r> {
        match (self.settings.interface.ruby_text_type, romanized) {
            (RubyTextType::None, _) => RubyTextMode::None,
            (RubyTextType::Furigana, Some(rz)) if rz.term().text() != rz.term().kana() => {
                RubyTextMode::Text(rz.term().kana())
            }
            (RubyTextType::Romaji, Some(rz)) => RubyTextMode::Text(rz.romaji()),
            _ => RubyTextMode::Pad,
        }
    }

    /// Text the parser does not gloss, such as punctuation.
    fn skipped(&mut self, text: &str, ink: Ink) {
        let ruby = self.ruby(None);
        draw_kanji_text(
            self.ui,
            &mut self.flow,
            text,
            ruby,
            KanjiStyle {
                highlight: None,
                ink,
                underline: UnderlineMode::None,
            },
        );
    }

    /// Text that will be glossed once the parse finishes, boxed like the
    /// words it will become.
    fn preview(&mut self, text: &str) {
        let ruby = self.ruby(None);
        draw_kanji_text(
            self.ui,
            &mut self.flow,
            text,
            ruby,
            KanjiStyle {
                highlight: Some(self.highlight),
                ink: Ink::Preview,
                underline: UnderlineMode::None,
            },
        );
    }

    /// A glossed word: hover for its entry, click to keep it open. Returns
    /// whether its underline is hovered.
    fn word(&mut self, ast: &SyntaxTree, romanized: &Romanized, underline: UnderlineMode) -> bool {
        let ui = self.ui;
        let term = romanized.term();
        let ruby = self.ruby(Some(romanized));
        let ul_hover = draw_kanji_text(
            ui,
            &mut self.flow,
            term.text(),
            ruby,
            KanjiStyle {
                highlight: Some(if term.is_custom() {
                    NAME_HIGHLIGHT
                } else {
                    self.highlight
                }),
                ink: Ink::text(self.settings.interface.stroke_text),
                underline,
            },
        );

        if ui.is_item_hovered() {
            ui.set_mouse_cursor(Some(MouseCursor::Hand));
        }
        if ui.is_item_hovered_with_flags(ItemHoveredFlags::FOR_TOOLTIP) {
            ui.tooltip(|| {
                // The term window (click) shows the full entry.
                TermView::new(&ast.jmdict_data, &ast.kanji_info, romanized, 30.0)
                    .max_senses(TOOLTIP_SENSES)
                    .ui(ui, self.settings)
            });
        }
        if ui.is_item_clicked() && !self.term_windows.contains_key(romanized) {
            if let Some(key) = term_window_key(romanized) {
                self.term_windows.insert(romanized.clone(), key);
            }
        }
        ul_hover
    }

    fn segment(
        &mut self,
        ast: &SyntaxTree,
        segment_idx: usize,
        segment: &Segment,
        selected_clause: &mut HashMap<usize, usize>,
    ) {
        let ui = self.ui;
        match segment {
            Segment::Skipped(skipped) => {
                self.skipped(skipped, Ink::text(self.settings.interface.stroke_text));
            }
            Segment::Clauses(clauses) => {
                let clause_idx = selected_clause.get(&segment_idx).copied().unwrap_or(0);
                let Some(clause) = clauses.get(clause_idx) else {
                    return;
                };
                // if clause.score() > 0 {
                let romanized = clause.romanized();
                for (idx, rz) in romanized.iter().enumerate() {
                    let underline_mode = if clauses.len() > 1 {
                        if idx == romanized.len() - 1 {
                            UnderlineMode::Normal
                        } else {
                            UnderlineMode::Pad
                        }
                    } else {
                        UnderlineMode::None
                    };
                    if !self.word(ast, rz, underline_mode) {
                        continue;
                    }
                    // Scrolling over the underline cycles through the
                    // alternative interpretations.
                    let scroll = ui.io().mouse_wheel() as isize;
                    let next = clause_idx
                        .saturating_add_signed(-scroll)
                        .min(clauses.len() - 1);
                    if next != clause_idx {
                        selected_clause.insert(segment_idx, next);
                    }
                    ui.tooltip(|| {
                        ui.text(format!(
                            "Alternate #{}/{} score={} (scroll to cycle)",
                            next + 1,
                            clauses.len(),
                            clause.score()
                        ));
                        ui.separator();
                        let _wrap_token = ui.push_text_wrap_pos(ui.current_font_size() * 20.0);
                        let romaji = clause
                            .romanized()
                            .iter()
                            .map(|x| x.romaji())
                            .collect::<Vec<&str>>()
                            .join(" ");
                        ui.text_wrapped(romaji);
                    });
                }
                // } else {
                //     self.skipped(&clause.text(), Ink::text(self.settings.interface.stroke_text));
                // }
            }
        }
    }
}

/// Identity of a word's window. Titled with the word's text, which other
/// interpretations of the same text share, so the identity comes from the
/// interpretation itself.
fn term_window_key(romanized: &Romanized) -> Option<WindowKey> {
    let mut hasher = DefaultHasher::new();
    romanized.hash(&mut hasher);
    WindowKey::new(
        format!("term-{:016x}", hasher.finish()),
        romanized.term().text(),
    )
    .ok()
}

/// A word's full entry in its own window. Returns false once closed.
fn term_window(
    ui: &Ui,
    settings: &Settings,
    ast: &SyntaxTree,
    romanized: &Romanized,
    key: &WindowKey,
) -> bool {
    let mut opened = true;
    ui.window(key)
        .size_constraints(TextSize::new(33.0, 4.5).px(ui), [f32::MAX, f32::MAX])
        .flags(WindowFlags::NO_SAVED_SETTINGS)
        .opened(&mut opened)
        .build(|| {
            TermView::new(&ast.jmdict_data, &ast.kanji_info, romanized, 0.0).ui(ui, settings);
        });
    opened
}
