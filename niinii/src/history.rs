//! The lines of text the user has seen, and the parse that glosses each one.

use std::collections::{HashMap, VecDeque};

use futures::FutureExt;
use ichiran::prelude::*;
use tokio::task::JoinHandle;
use tracing::Instrument;

use crate::parser::{self, Parser, SyntaxTree};
use crate::settings::Settings;
use crate::support::{
    platform::{clipboard_sequence_number, clipboard_text},
    regex::CachedRegex,
};
use crate::translator::ExchangeId;

/// Lines kept for stepping back through recent text.
const HISTORY_LEN: usize = 100;

pub enum Gloss {
    /// While the parse is in flight: the text chunked by `basic_split` so
    /// Text/Skip blocks can be styled distinctly.
    Preview(Vec<(Split, String)>),
    Parsed(SyntaxTree),
}

/// One line of text with its gloss and translation.
pub struct Line {
    id: u64,
    /// The text as glossed and translated (after the match/replace regex).
    pub text: String,
    pub gloss: Gloss,
    /// Alternative interpretation chosen per segment, keyed by segment index.
    pub selected_clause: HashMap<usize, usize>,
    pub translation: Option<ExchangeId>,
}

impl Line {
    pub fn ast(&self) -> Option<&SyntaxTree> {
        match &self.gloss {
            Gloss::Parsed(ast) => Some(ast),
            Gloss::Preview(_) => None,
        }
    }
}

/// Emitted from [`History::poll`] so orchestration (parse, translate, speak)
/// happens in one place.
pub enum HistoryEvent {
    ClipboardReceived(String),
    Failed(parser::Error),
}

pub struct History {
    parser: Parser,
    match_regex: CachedRegex,
    lines: VecDeque<Line>,
    /// Index into `lines` of the line being shown.
    cursor: usize,
    next_line_id: u64,
    pending: Option<PendingParse>,
    last_clipboard: String,
    last_clipboard_seq: u32,
    events: VecDeque<HistoryEvent>,
}

/// The parse of the newest line. Kanji info loads in the background and is
/// applied once the AST has landed.
struct PendingParse {
    line: u64,
    ast: Option<JoinHandle<Result<SyntaxTree, parser::Error>>>,
    kanji: JoinHandle<Result<HashMap<char, Kanji>, parser::Error>>,
}

impl Drop for PendingParse {
    fn drop(&mut self) {
        if let Some(ast) = &self.ast {
            ast.abort();
        }
        self.kanji.abort();
    }
}

impl History {
    pub async fn new(settings: &Settings) -> Self {
        Self {
            parser: Parser::new(settings).await,
            match_regex: CachedRegex::default(),
            lines: VecDeque::new(),
            cursor: 0,
            next_line_id: 0,
            pending: None,
            last_clipboard: String::new(),
            last_clipboard_seq: 0,
            events: VecDeque::new(),
        }
    }

    pub fn parser(&self) -> &Parser {
        &self.parser
    }

    /// The line being shown.
    pub fn current(&self) -> Option<&Line> {
        self.lines.get(self.cursor)
    }

    pub fn current_mut(&mut self) -> Option<&mut Line> {
        self.lines.get_mut(self.cursor)
    }

    pub fn ast(&self) -> Option<&SyntaxTree> {
        self.current().and_then(Line::ast)
    }

    /// Translation of the line being shown.
    pub fn translation(&self) -> Option<ExchangeId> {
        self.current().and_then(|line| line.translation)
    }

    pub fn set_translation(&mut self, id: ExchangeId) {
        if let Some(line) = self.current_mut() {
            line.translation = Some(id);
        }
    }

    /// 1-based position of the line being shown, and the number of lines.
    pub fn position(&self) -> (usize, usize) {
        (self.cursor + 1, self.lines.len())
    }

    /// Whether the line being shown is not the latest one.
    pub fn showing_earlier_line(&self) -> bool {
        self.cursor + 1 < self.lines.len()
    }

    pub fn back(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn forward(&mut self) {
        self.cursor = (self.cursor + 1).min(self.lines.len().saturating_sub(1));
    }

    pub fn latest(&mut self) {
        self.cursor = self.lines.len().saturating_sub(1);
    }

    pub fn show(&mut self, idx: usize) {
        if idx < self.lines.len() {
            self.cursor = idx;
        }
    }

    /// All lines, oldest first.
    pub fn lines(&self) -> impl Iterator<Item = &Line> {
        self.lines.iter()
    }

    /// Identifies the newest line; changes whenever a line is added.
    pub fn latest_line_id(&self) -> Option<u64> {
        self.lines.back().map(|line| line.id)
    }

    /// Whether a parse is blocking input. Kanji info loading does not.
    pub fn is_processing(&self) -> bool {
        self.pending.as_ref().is_some_and(|p| p.ast.is_some())
    }

    /// Adds `text`, after the configured match/replace regex, as the newest
    /// line and starts parsing it, replacing any parse still in flight. The
    /// line shows a preview until the parse lands. Returns the processed
    /// text, or `None` if nothing was left to parse.
    pub fn request(
        &mut self,
        text: &str,
        settings: &Settings,
    ) -> Result<Option<String>, parser::Error> {
        let regex = self.match_regex.get(&settings.text.regex_match)?;
        let text = regex
            .replace_all(text, &settings.text.regex_replace)
            .into_owned();
        // VN/clipboard text often arrives with hard line breaks from dialogue
        // wrapping; ichiran segments per-line so embedded newlines wreck the
        // parse. Strip them along with leading/trailing whitespace.
        let text = text.replace(['\n', '\r'], "").trim().to_owned();
        if text.is_empty() {
            return Ok(None);
        }

        // A line replaced before its parse finished was never shown glossed.
        if let Some(prev) = self.pending.take() {
            if self.lines.back().is_some_and(|line| line.id == prev.line) {
                self.lines.pop_back();
            }
        }

        let variants = if settings.interface.more_variants {
            5
        } else {
            1
        };
        let splits: Vec<(Split, String)> = basic_split(&text)
            .into_iter()
            .map(|(kind, s)| (kind, s.to_string()))
            .collect();
        let id = self.next_line_id;
        self.next_line_id += 1;
        self.lines.push_back(Line {
            id,
            text: text.clone(),
            gloss: Gloss::Preview(splits.clone()),
            selected_clause: HashMap::new(),
            translation: None,
        });
        if self.lines.len() > HISTORY_LEN {
            self.lines.pop_front();
        }
        self.latest();

        let parser = self.parser.clone();
        let ast_text = text.clone();
        let ast = tokio::spawn(
            async move { parser.parse_ast(&ast_text, &splits, variants).await }
                .instrument(tracing::debug_span!("parse_ast")),
        );
        let parser = self.parser.clone();
        let kanji_text = text.clone();
        let kanji = tokio::spawn(
            async move { parser.parse_kanji(&kanji_text).await }
                .instrument(tracing::debug_span!("parse_kanji")),
        );
        self.pending = Some(PendingParse {
            line: id,
            ast: Some(ast),
            kanji,
        });
        Ok(Some(text))
    }

    /// Watches the clipboard and lands finished parses. Returns the next
    /// event for the caller to act on.
    pub fn poll(&mut self, settings: &Settings) -> Option<HistoryEvent> {
        self.poll_clipboard(settings);
        self.poll_parse();
        self.events.pop_front()
    }

    fn poll_clipboard(&mut self, settings: &Settings) {
        if !settings.watch_clipboard {
            return;
        }
        // An unavailable sequence number (0) always counts as changed.
        let seq = clipboard_sequence_number();
        if seq != 0 && seq == self.last_clipboard_seq {
            return;
        }
        self.last_clipboard_seq = seq;
        if let Some(text) = clipboard_text() {
            if text != self.last_clipboard {
                self.last_clipboard.clone_from(&text);
                self.events.push_back(HistoryEvent::ClipboardReceived(text));
            }
        }
    }

    fn poll_parse(&mut self) {
        let Some(pending) = self.pending.as_mut() else {
            return;
        };
        let Some(idx) = self.lines.iter().position(|line| line.id == pending.line) else {
            self.pending = None;
            return;
        };
        if let Some(handle) = pending.ast.as_mut() {
            let Some(result) = handle.now_or_never() else {
                return;
            };
            pending.ast = None;
            match result {
                Ok(Ok(ast)) => self.lines[idx].gloss = Gloss::Parsed(ast),
                Ok(Err(err)) => {
                    self.events.push_back(HistoryEvent::Failed(err));
                    self.pending = None;
                    return;
                }
                // Aborted by a follow-up request; the replacement is already in flight.
                Err(_) => return,
            }
        }
        let Some(result) = (&mut pending.kanji).now_or_never() else {
            return;
        };
        self.pending = None;
        match result {
            Ok(Ok(kanji_info)) => {
                if let Gloss::Parsed(ast) = &mut self.lines[idx].gloss {
                    ast.kanji_info = kanji_info;
                }
            }
            Ok(Err(err)) => self.events.push_back(HistoryEvent::Failed(err)),
            Err(_) => {}
        }
    }
}
