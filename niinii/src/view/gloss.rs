use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
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
use crate::translator::{ExchangeId, TranslationSpan};
use crate::view::{raw::RawView, term::TermView};

const CLIPBOARD_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Term-block widths for one segment, in flow order. Skips contribute one
/// item (the punctuation glyph); clauses contribute one per romanized
/// term using the same ruby logic the actual draw will apply.
fn segment_term_widths(
    ui: &Ui,
    ctx: &Context,
    settings: &Settings,
    segment: &Segment,
    selected: &HashMap<Segment, i32>,
) -> Vec<f32> {
    match segment {
        Segment::Skipped(s) => vec![measure_kanji_w(ui, ctx, s, &RubyTextMode::None)],
        Segment::Clauses(clauses) => {
            let sel = selected.get(segment).copied().unwrap_or(0);
            clauses
                .get(sel as usize)
                .map(|c| {
                    c.romanized()
                        .iter()
                        .map(|rz| {
                            measure_kanji_w(
                                ui,
                                ctx,
                                rz.term().text(),
                                &GlossView::ruby_for_term(settings, rz),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default()
        }
    }
}

/// One translation rendering group: a contiguous range of basic-split
/// segment indices sharing a single span's translation. Pending groups
/// (the model hasn't emitted their span yet) hold a single index and a
/// single ellipsis "word" so they consume a tiny but visible slot during
/// streaming. `words` is built once at group construction and reused by
/// both the planner and the drawer.
struct SegmentGroup {
    start: usize,
    end: usize,
    words: Vec<String>,
}

impl SegmentGroup {
    fn indices(&self) -> std::ops::RangeInclusive<usize> {
        self.start..=self.end
    }
}

/// Partition basic-split segment indices `0..num_segments` into rendering
/// groups using the model's translation spans. Spans covering multiple
/// segments produce a single group covering all of them; segments not yet
/// covered by any span become pending single-segment groups rendered as
/// an ellipsis.
fn group_segments(ui: &Ui, spans: &[TranslationSpan], num_segments: usize) -> Vec<SegmentGroup> {
    // Sort spans by start once and walk in lockstep with `idx`. The model
    // emits non-overlapping spans in roughly-but-not-strictly streaming order,
    // so we can't rely on input ordering.
    let mut sorted: Vec<&TranslationSpan> = spans.iter().collect();
    sorted.sort_by_key(|s| s.start);

    let mut out = Vec::new();
    let mut idx = 0;
    let mut sp = 0;
    while idx < num_segments {
        // Skip past any spans that start before the cursor (already covered or
        // overlapping a prior span).
        while sp < sorted.len() && sorted[sp].start < idx {
            sp += 1;
        }
        if sp < sorted.len() && sorted[sp].start == idx {
            let span = sorted[sp];
            let end = span.end.min(num_segments - 1).max(idx);
            out.push(SegmentGroup {
                start: idx,
                end,
                words: span.text.split_whitespace().map(String::from).collect(),
            });
            idx = end + 1;
            sp += 1;
        } else {
            out.push(SegmentGroup {
                start: idx,
                end: idx,
                words: vec![ellipses(ui).to_owned()],
            });
            idx += 1;
        }
    }
    out
}

/// Merge per-segment rows into one sequence covering the group. Entries on
/// the same row collapse to one extent spanning min-left to max-right.
/// `epsilon` is in the same units as `r.y` -- callers pass a small constant
/// for simulator rows (integer indices) or a pixel tolerance for drawn rows.
fn merged_group_rows(
    group: &SegmentGroup,
    rows: &[Vec<RowExtent>],
    epsilon: f32,
) -> Vec<RowExtent> {
    let mut all: Vec<RowExtent> = group
        .indices()
        .flat_map(|i| rows[i].iter().copied())
        .collect();
    all.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal));

    let mut merged: Vec<RowExtent> = Vec::new();
    for r in all {
        match merged.last_mut() {
            Some(m) if (m.y - r.y).abs() < epsilon => {
                m.left = m.left.min(r.left);
                m.right = m.right.max(r.right);
                m.y = m.y.min(r.y);
            }
            _ => merged.push(r),
        }
    }
    merged
}

fn plan_segment_translations(
    ui: &Ui,
    sim_rows: &[Vec<RowExtent>],
    spans: &[TranslationSpan],
) -> (Vec<SegmentGroup>, Vec<f32>) {
    let groups = group_segments(ui, spans, sim_rows.len());
    let plan_groups: Vec<(Vec<RowExtent>, &[String])> = groups
        .iter()
        .map(|g| (merged_group_rows(g, sim_rows, 0.5), g.words.as_slice()))
        .collect();
    let group_h = plan_translation_reservations(ui, &plan_groups, ui.text_line_height());

    let mut reservations = vec![0.0f32; sim_rows.len()];
    for (gi, group) in groups.iter().enumerate() {
        for idx in group.indices() {
            reservations[idx] = group_h[gi];
        }
    }
    (groups, reservations)
}

fn draw_segment_translations(
    ui: &Ui,
    ctx: &Context,
    ruby_present: bool,
    stroke: bool,
    fore: StyleColor,
    groups: &[SegmentGroup],
    actual_rows: &[Vec<RowExtent>],
) {
    let pixel_epsilon = ui.text_line_height() * 0.5;
    for group in groups {
        let merged = merged_group_rows(group, actual_rows, pixel_epsilon);
        let widths: Vec<f32> = merged.iter().map(|r| r.right - r.left).collect();
        let word_refs: Vec<&str> = group.words.iter().map(String::as_str).collect();
        let lines = distribute_lines(ui, &word_refs, &widths);
        draw_translation_lines_colored(ui, ctx, ruby_present, stroke, fore, &merged, &lines);
    }
}

enum View {
    Text(Arc<Vec<(Split, String)>>),
    Interpret { ast: SyntaxTree },
}

/// Currently-glossed input. `splits` are needed by the App for translator
/// dispatch and by the parsed view to keep translation indices in basic-split
/// space. `exchange_id` ties displayed translations to this exact gloss.
struct CurrentRequest {
    splits: Arc<Vec<(Split, String)>>,
    exchange_id: Option<ExchangeId>,
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
    pending: Option<JoinHandle<Result<SyntaxTree, parser::Error>>>,
    match_regex: CachedRegex,

    input_text: String,
    last_clipboard: String,
    last_clipboard_poll: Instant,

    events: VecDeque<GlossEvent>,

    view: Option<View>,
    current: Option<CurrentRequest>,
    show_term_window: RefCell<HashSet<Romanized>>,
    selected_clause: RefCell<HashMap<Segment, i32>>,
    show_raw: bool,
    show_glossary: bool,
}

impl GlossView {
    pub async fn new(settings: &Settings) -> Self {
        Self {
            parser: Parser::new(settings).await,
            pending: None,
            match_regex: CachedRegex::default(),
            input_text: String::new(),
            last_clipboard: String::new(),
            last_clipboard_poll: Instant::now(),
            events: VecDeque::new(),
            view: None,
            current: None,
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
        self.pending.is_some()
    }

    pub fn input_text(&self) -> &str {
        &self.input_text
    }

    pub fn bind_translation(&mut self, id: ExchangeId) {
        if let Some(c) = self.current.as_mut() {
            c.exchange_id = Some(id);
        }
    }

    pub fn current_exchange_id(&self) -> Option<ExchangeId> {
        self.current.as_ref().and_then(|c| c.exchange_id)
    }

    /// Basic-split source strings, in order. Includes both Text and Skip
    /// segments so punctuation, quotes, Latin names, etc. are available to
    /// the translator. Translation output indices use this same order.
    pub fn translation_segments(&self) -> Option<Vec<String>> {
        self.current
            .as_ref()
            .map(|c| c.splits.iter().map(|(_, t)| t.clone()).collect())
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
        let text = text.trim().to_owned();
        if text.is_empty() {
            return Ok(None);
        }

        if let Some(prev) = self.pending.take() {
            prev.abort();
        }
        let splits: Arc<Vec<(Split, String)>> = Arc::new(
            basic_split(&text)
                .into_iter()
                .map(|(s, t)| (s, t.to_owned()))
                .collect(),
        );

        let parser = self.parser.clone();
        let variants = if settings.more_variants { 5 } else { 1 };
        let spawn_text = text.clone();
        let spawn_splits = Arc::clone(&splits);
        self.current = Some(CurrentRequest {
            splits: Arc::clone(&splits),
            exchange_id: None,
        });
        self.view = Some(View::Text(splits));
        self.pending = Some(tokio::spawn(
            async move { parser.parse(&spawn_text, &spawn_splits, variants).await }
                .instrument(tracing::debug_span!("parse")),
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

        if let Some(handle) = self.pending.as_mut() {
            if let Some(poll) = handle.now_or_never() {
                self.pending = None;
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

    /// Map the user's ruby-text setting to a `RubyTextMode` for a specific
    /// term. Used by both `measure_kanji_w` (layout simulation) and the
    /// term draw, so the two stay in sync.
    fn ruby_for_term<'a>(settings: &Settings, rz: &'a Romanized) -> RubyTextMode<'a> {
        let term = rz.term();
        match settings.ruby_text_type {
            RubyTextType::None => RubyTextMode::None,
            RubyTextType::Furigana if term.text() != term.kana() => RubyTextMode::Text(term.kana()),
            RubyTextType::Romaji => RubyTextMode::Text(rz.romaji()),
            _ => RubyTextMode::Pad,
        }
    }

    /// Draw a non-romanized text block (skipped punctuation, or preview-mode
    /// glyph). `preview` switches to the disabled/outlined preview style.
    fn add_block(
        &self,
        ctx: &mut Context,
        ui: &Ui,
        settings: &Settings,
        text: &str,
        preview: bool,
        bottom: BottomTextMode,
    ) -> RowExtent {
        let drawn = draw_kanji_text(
            ui,
            ctx,
            text,
            if settings.ruby_text_type == RubyTextType::None {
                RubyTextMode::None
            } else {
                RubyTextMode::Pad
            },
            bottom,
            KanjiStyle {
                highlight: false,
                stroke: !preview && settings.stroke_text,
                preview,
                underline: UnderlineMode::None,
            },
        );
        RowExtent {
            y: drawn.row_y,
            left: drawn.x_left,
            right: drawn.x_right,
        }
    }

    fn add_romanized(
        &self,
        ctx: &mut Context,
        ui: &Ui,
        settings: &Settings,
        romanized: &Romanized,
        underline: UnderlineMode,
        bottom: BottomTextMode,
    ) -> KanjiDrawn {
        let drawn = draw_kanji_text(
            ui,
            ctx,
            romanized.term().text(),
            Self::ruby_for_term(settings, romanized),
            bottom,
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

        drawn
    }

    /// Render a single Clauses segment with its (optional) translation.
    ///
    /// Draw a clause's kanji terms with the given bottom-pad reservation.
    /// `reservation_h` is precomputed by `plan_clauses` so all terms get
    /// the same Pad height -- this makes every row of the clause exactly
    /// tall enough for the translation lines that will be drawn under it.
    /// Returns the row aggregates (pixel y, x_left, x_right) for the
    /// translation phase.
    fn add_clause(
        &self,
        ctx: &mut Context,
        ui: &Ui,
        settings: &Settings,
        segment: &Segment,
        clauses: &[Clause],
        reservation_h: f32,
    ) -> Vec<RowExtent> {
        let mut selected_clause = self.selected_clause.borrow_mut();
        let mut clause_idx = selected_clause.get(segment).cloned().unwrap_or(0);
        let Some(clause) = clauses.get(clause_idx as usize) else {
            return Vec::new();
        };
        let romanized = clause.romanized();
        if romanized.is_empty() {
            return Vec::new();
        }

        let bottom = if reservation_h > 0.0 {
            BottomTextMode::Pad(reservation_h)
        } else {
            BottomTextMode::None
        };
        let last_term = romanized.len() - 1;
        let mut rows: Vec<RowExtent> = Vec::new();
        for (idx, rz) in romanized.iter().enumerate() {
            let underline = if clauses.len() > 1 {
                if idx == last_term {
                    UnderlineMode::Normal
                } else {
                    UnderlineMode::Pad
                }
            } else {
                UnderlineMode::None
            };
            let drawn = self.add_romanized(ctx, ui, settings, rz, underline, bottom);
            match rows.last_mut() {
                Some(r) if (r.y - drawn.row_y).abs() < 0.5 => {
                    r.right = r.right.max(drawn.x_right);
                }
                _ => rows.push(RowExtent {
                    y: drawn.row_y,
                    left: drawn.x_left,
                    right: drawn.x_right,
                }),
            }
            if drawn.underline_hover {
                let scroll = ui.io().mouse_wheel as i32;
                clause_idx = (clause_idx - scroll).clamp(0, clauses.len() as i32 - 1);
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
                    let _wrap_token = ui.push_text_wrap_pos_with_pos(ui.current_font_size() * 20.0);
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
        rows
    }

    /// Driver shared by parsed (`add_root`) and preview (`add_splits`) views.
    /// Two phases: simulate flow over precomputed `widths_per_segment` to
    /// partition the segments into translation groups, then draw each segment
    /// (capturing pixel rows) and place per-group translations across the
    /// merged actual rows of their constituent segments.
    fn add_segmented<T>(
        &self,
        ctx: &mut Context,
        ui: &Ui,
        settings: &Settings,
        items: &[T],
        widths_per_segment: Vec<Vec<f32>>,
        translations: Option<&[TranslationSpan]>,
        fore: StyleColor,
        stroke: bool,
        mut draw: impl FnMut(&mut Context, &T, BottomTextMode) -> Vec<RowExtent>,
    ) {
        let sim_rows = simulate_global_flow(ui, &widths_per_segment);
        let num_segments = items.len().min(sim_rows.len());

        let Some(spans) = translations else {
            for item in items {
                draw(ctx, item, BottomTextMode::None);
            }
            return;
        };

        let (groups, reservation) = plan_segment_translations(ui, &sim_rows[..num_segments], spans);

        let mut actual_rows: Vec<Vec<RowExtent>> = vec![Vec::new(); items.len()];
        for (i, item) in items.iter().enumerate() {
            let h = reservation.get(i).copied().unwrap_or(0.0);
            let bottom = if h > 0.0 {
                BottomTextMode::Pad(h)
            } else {
                BottomTextMode::None
            };
            actual_rows[i] = draw(ctx, item, bottom);
        }

        draw_segment_translations(
            ui,
            ctx,
            settings.ruby_text_type != RubyTextType::None,
            stroke,
            fore,
            &groups,
            &actual_rows,
        );
    }

    fn add_root(
        &self,
        ctx: &mut Context,
        ui: &Ui,
        settings: &Settings,
        root: &Root,
        num_basic_segments: usize,
        translations: Option<&[TranslationSpan]>,
    ) {
        let segments = root.segments();
        debug_assert_eq!(segments.len(), num_basic_segments);

        let widths: Vec<Vec<f32>> = {
            let selected = self.selected_clause.borrow();
            segments
                .iter()
                .map(|seg| segment_term_widths(ui, ctx, settings, seg, &selected))
                .collect()
        };

        self.add_segmented(
            ctx,
            ui,
            settings,
            segments,
            widths,
            translations,
            StyleColor::Text,
            settings.stroke_text,
            |ctx, seg, bottom| match seg {
                Segment::Skipped(s) => {
                    vec![self.add_block(ctx, ui, settings, s, false, bottom)]
                }
                Segment::Clauses(clauses) => {
                    let h = match bottom {
                        BottomTextMode::Pad(h) => h,
                        BottomTextMode::None => 0.0,
                    };
                    self.add_clause(ctx, ui, settings, seg, clauses, h)
                }
            },
        );
    }

    /// Render the pre-Ichiran preview directly from `basic_split` segments.
    /// Uses the same basic-split index space and grouped layout planner as
    /// the parsed view, so streaming translations don't use a different
    /// layout while Ichiran is still running.
    fn add_splits(
        &self,
        ctx: &mut Context,
        ui: &Ui,
        settings: &Settings,
        splits: &[(Split, String)],
        translations: Option<&[TranslationSpan]>,
    ) {
        let ruby = if settings.ruby_text_type == RubyTextType::None {
            RubyTextMode::None
        } else {
            RubyTextMode::Pad
        };
        let widths: Vec<Vec<f32>> = splits
            .iter()
            .map(|(_, text)| vec![measure_kanji_w(ui, ctx, text, &ruby)])
            .collect();

        self.add_segmented(
            ctx,
            ui,
            settings,
            splits,
            widths,
            translations,
            StyleColor::TextDisabled,
            false,
            |ctx, (_, text), bottom| vec![self.add_block(ctx, ui, settings, text, true, bottom)],
        );
    }

    pub fn ui(
        &mut self,
        ctx: &mut Context,
        ui: &Ui,
        settings: &Settings,
        translations: Option<&[TranslationSpan]>,
    ) {
        ui.text(""); // anchor for line wrapping
        match &self.view {
            Some(View::Interpret { ast }) => {
                let num_basic_segments = self
                    .current
                    .as_ref()
                    .map(|current| current.splits.len())
                    .unwrap_or(0);
                self.add_root(
                    ctx,
                    ui,
                    settings,
                    &ast.root,
                    num_basic_segments,
                    translations,
                );

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
            Some(View::Text(splits)) => self.add_splits(ctx, ui, settings, splits, translations),
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
