use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

use futures::FutureExt;
use ichiran::prelude::*;
use imgui::*;
use tokio::task::JoinHandle;
use tracing::Instrument;

use super::index::IndexView;
use super::mixins::*;
use crate::parser::{self, Parser, SyntaxTree};
use crate::renderer::context::{Context, ContextFlags};
use crate::settings::{RubyTextType, Settings};
use crate::support::regex::CachedRegex;
use crate::view::{raw::RawView, term::TermView};

const CLIPBOARD_POLL_INTERVAL: Duration = Duration::from_millis(33);

enum View {
    /// Preview shown while a parse is in flight: the text chunked by
    /// `basic_split` so Text/Skip blocks can be styled distinctly.
    Text(Vec<(Split, String)>),
    Interpret {
        ast: SyntaxTree,
    },
}

/// Emitted from `GlossView::poll`. `ClipboardReceived` surfaces new clipboard
/// text to the caller so orchestration (parse + translate + clear) happens in
/// one place -- `GlossView` does not self-gloss on clipboard changes.
pub enum GlossEvent {
    ClipboardReceived(String),
    Failed(parser::Error),
}

/// Returned from `show_input` so the caller can dispatch. The input row owns
/// both the Gloss and Translate buttons so the UI stays together.
pub enum GlossInputAction {
    Gloss(String),
    Translate(String),
}

pub struct GlossView {
    parser: Parser,
    pending_ast: Option<JoinHandle<Result<SyntaxTree, parser::Error>>>,
    pending_kanji: Option<JoinHandle<Result<HashMap<char, Kanji>, parser::Error>>>,
    match_regex: CachedRegex,

    input_text: String,
    last_clipboard: String,
    last_clipboard_poll: Instant,

    events: VecDeque<GlossEvent>,

    view: Option<View>,
    show_term_window: RefCell<HashSet<Romanized>>,
    selected_clause: RefCell<HashMap<Segment, i32>>,
    show_raw: bool,
    show_glossary: bool,
}

impl GlossView {
    pub async fn new(settings: &Settings) -> Self {
        Self {
            parser: Parser::new(settings).await,
            pending_ast: None,
            pending_kanji: None,
            match_regex: CachedRegex::default(),
            input_text: String::new(),
            last_clipboard: String::new(),
            last_clipboard_poll: Instant::now(),
            events: VecDeque::new(),
            view: None,
            show_term_window: RefCell::new(HashSet::new()),
            selected_clause: RefCell::new(HashMap::new()),
            show_raw: false,
            show_glossary: false,
        }
    }

    pub fn ast(&self) -> Option<&SyntaxTree> {
        if let Some(View::Interpret { ast, .. }) = &self.view {
            Some(ast)
        } else {
            None
        }
    }

    pub fn is_processing(&self) -> bool {
        // Only block the input on the AST; kanji info loads in the background.
        self.pending_ast.is_some()
    }

    pub fn input_text(&self) -> &str {
        &self.input_text
    }

    /// Preprocess `text` through the configured match/replace regex and spawn
    /// a parse. Aborts any prior in-flight parse. The preview text is shown
    /// immediately; `poll` will transition to the parsed AST on completion.
    /// Returns the post-regex text on success, or `None` if the regex yielded
    /// empty text and nothing was spawned.
    pub fn request(
        &mut self,
        text: &str,
        settings: &Settings,
    ) -> Result<Option<String>, parser::Error> {
        let regex = self.match_regex.get(&settings.regex_match)?;
        let text = regex
            .replace_all(text, &settings.regex_replace)
            .into_owned();
        // VN/clipboard text often arrives with hard line breaks from dialogue
        // wrapping; ichiran segments per-line so embedded newlines wreck the
        // parse. Strip them along with leading/trailing whitespace.
        let text = text
            .replace(|c: char| c == '\n' || c == '\r', "")
            .trim()
            .to_owned();
        if text.is_empty() {
            return Ok(None);
        }

        if let Some(prev) = self.pending_ast.take() {
            prev.abort();
        }
        if let Some(prev) = self.pending_kanji.take() {
            prev.abort();
        }

        let variants = if settings.more_variants { 5 } else { 1 };
        let splits: Vec<(Split, String)> = basic_split(&text)
            .into_iter()
            .map(|(kind, s)| (kind, s.to_string()))
            .collect();
        self.view = Some(View::Text(splits.clone()));

        let parser_ast = self.parser.clone();
        let ast_text = text.clone();
        self.pending_ast = Some(tokio::spawn(
            async move { parser_ast.parse_ast(&ast_text, &splits, variants).await }
                .instrument(tracing::debug_span!("parse_ast")),
        ));

        let parser_kanji = self.parser.clone();
        let kanji_text = text.clone();
        self.pending_kanji = Some(tokio::spawn(
            async move { parser_kanji.parse_kanji(&kanji_text).await }
                .instrument(tracing::debug_span!("parse_kanji")),
        ));

        Ok(Some(text))
    }

    /// Drive clipboard watching and pending-parse completion. Returns an event
    /// when a parse finishes so the caller can wire up auto-translate etc.
    pub fn poll(&mut self, ui: &Ui, ctx: &mut Context, settings: &Settings) -> Option<GlossEvent> {
        if settings.watch_clipboard && self.last_clipboard_poll.elapsed() >= CLIPBOARD_POLL_INTERVAL
        {
            self.last_clipboard_poll = Instant::now();
            if let Some(clipboard) = ui.clipboard_text() {
                if clipboard != self.last_clipboard {
                    self.input_text.clone_from(&clipboard);
                    self.last_clipboard.clone_from(&clipboard);
                    // Ignore clipboard contents if they are unreasonably large
                    if clipboard.len() < 500 {
                        self.events
                            .push_back(GlossEvent::ClipboardReceived(clipboard));
                    }
                }
            }
        }

        if let Some(handle) = self.pending_ast.as_mut() {
            if let Some(poll) = handle.now_or_never() {
                self.pending_ast = None;
                match poll {
                    Ok(Ok(ast)) => {
                        if ctx.flags().contains(ContextFlags::SUPPORTS_ATLAS_UPDATE) {
                            ctx.add_unknown_glyphs_from_root(&ast.root);
                        }
                        self.view = Some(View::Interpret { ast });
                    }
                    Ok(Err(err)) => self.events.push_back(GlossEvent::Failed(err)),
                    // Aborted by a follow-up request; the replacement is already in flight.
                    Err(_) => {}
                }
            }
        }

        // Only drain kanji once the AST has landed -- the JoinHandle holds a
        // completed result for us, so there's no need for a separate buffer.
        if let Some(View::Interpret { ast, .. }) = &mut self.view {
            if let Some(handle) = self.pending_kanji.as_mut() {
                if let Some(poll) = handle.now_or_never() {
                    self.pending_kanji = None;
                    match poll {
                        Ok(Ok(kanji_info)) => ast.kanji_info = kanji_info,
                        Ok(Err(err)) => self.events.push_back(GlossEvent::Failed(err)),
                        Err(_) => {}
                    }
                }
            }
        }

        self.events.pop_front()
    }

    /// Render the manual-input row: textarea, Gloss button, Translate button.
    /// Leaves the cursor on the same line so the caller can append adjacent
    /// controls (e.g. a usage bar). Returns the action the user triggered.
    pub fn show_input(&mut self, ui: &Ui) -> Option<GlossInputAction> {
        let mut action = None;
        {
            let _disabled = ui.begin_disabled(self.is_processing());
            let entered = ui
                .input_text_multiline("##", &mut self.input_text, [0.0, 50.0])
                .enter_returns_true(true)
                .build();
            let clicked = ui.button_with_size("Gloss", [120.0, 0.0]);
            if entered || clicked {
                action = Some(GlossInputAction::Gloss(self.input_text.clone()));
            }
        }
        ui.same_line();

        let enable_tl = self.ast().is_some_and(|ast| !ast.empty());
        let disable_tl = ui.begin_disabled(!enable_tl);
        if ui.button_with_size("Translate", [120.0, 0.0]) {
            if let Some(gloss) = self.ast() {
                action = Some(GlossInputAction::Translate(gloss.original_text.clone()));
            }
        }
        drop(disable_tl);
        if !enable_tl && ui.is_item_hovered_with_flags(ItemHoveredFlags::ALLOW_WHEN_DISABLED) {
            ui.tooltip(|| ui.text("Text does not require translation"));
        }
        action
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
            if settings.ruby_text_type == RubyTextType::None {
                RubyTextMode::None
            } else {
                RubyTextMode::Pad
            },
            KanjiStyle {
                highlight: false,
                stroke: !preview && settings.stroke_text,
                preview,
                underline: UnderlineMode::None,
            },
        );
    }

    /// Render a text chunk that will be glossed once the parse finishes.
    /// Drawn with `highlight: true` so it visually hints at the eventual
    /// clause boxes.
    fn add_preview_text(&self, ctx: &mut Context, ui: &Ui, settings: &Settings, text: &str) {
        draw_kanji_text(
            ui,
            ctx,
            text,
            if settings.ruby_text_type == RubyTextType::None {
                RubyTextMode::None
            } else {
                RubyTextMode::Pad
            },
            KanjiStyle {
                highlight: true,
                stroke: false,
                preview: true,
                underline: UnderlineMode::None,
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
            fg_text,
            KanjiStyle {
                highlight: true,
                stroke: settings.stroke_text,
                preview: false,
                underline,
            },
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
                        let ul_hover = self.add_romanized(
                            ctx,
                            ui,
                            settings,
                            rz,
                            settings.ruby_text_type,
                            underline_mode,
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
                                let _wrap_token =
                                    ui.push_text_wrap_pos_with_pos(ui.current_font_size() * 20.0);
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
                    // } else {
                    //     self.add_skipped(ctx, ui, settings, &clause.text(), false);
                    // }
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
        ui.text(""); // anchor for line wrapping
        match &self.view {
            Some(View::Interpret { ast }) => {
                self.add_root(ctx, ui, settings, &ast.root);

                if self.show_raw {
                    ui.window("Raw")
                        .size([300., 110.], Condition::FirstUseEver)
                        .opened(&mut self.show_raw)
                        .build(|| {
                            RawView::new(&ast.root).ui(ctx, ui);
                        });
                }
                if self.show_glossary {
                    ui.window("Glossary")
                        .size([300., 110.], Condition::FirstUseEver)
                        .opened(&mut self.show_glossary)
                        .build(|| {
                            IndexView::new(ast).ui(ctx, ui, settings);
                        });
                }
            }
            Some(View::Text(splits)) => {
                for (kind, chunk) in splits {
                    match kind {
                        Split::Text => self.add_preview_text(ctx, ui, settings, chunk),
                        Split::Skip => self.add_skipped(ctx, ui, settings, chunk, true),
                    }
                }
            }
            _ => {}
        }
        ui.new_line();

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
