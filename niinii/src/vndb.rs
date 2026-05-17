//! VNDB integration runtime.
//!
//! Each user action (search, set-active, clear) spawns an HTTP fetch task
//! that updates `Arc<ArcSwap<VndbState>>` directly when it returns. Writes
//! are serialized through a small `update_lock` so concurrent fetches don't
//! lose updates; UI reads stay wait-free via `state.load()`.
//!
//! Stale fetches (e.g. user typed in the search box twice and the older
//! request finishes second) are filtered with a content check inside the
//! update closure: search compares the response's query against
//! `state.search_query`; active-VN fetches compare `summary.id` against
//! `state.active.summary.id`. The state itself is the source of truth for
//! "what does the user currently want", so no separate generation counter
//! is needed.
//!
//! When a VN is loaded, a pre-rendered system-prompt fragment is published
//! via `on_prompt_change`, and the character list is pushed into ichiran's
//! dictionary so the segmenter recognises proper nouns. Custom entries are
//! cleared on every transition (and once at startup, in case a prior run
//! crashed mid-injection).

use std::{
    fmt::Write as _,
    sync::{Arc, Mutex},
};

use arc_swap::ArcSwap;
use ichiran::prelude::*;
use tokio::task::JoinHandle;
use tracing::Instrument;
pub use vndb::{Character, NameRef, SearchParams, Sex, SortKey, VnSummary};

pub const VNDB_PROMPT_PREAMBLE: &str = "Use the information in the following character summaries. When the speaker needs to be inferred, try your best using the character information provided. If you are unsure of the speaker's gender, use neutral pronouns such as 'singular -they'.";

#[derive(Clone, Debug)]
pub struct ActiveVn {
    pub summary: VnSummary,
    pub characters: Vec<Character>,
    /// Pre-rendered system-prompt fragment, ready to attach to a translation.
    pub prompt: String,
}

#[derive(Clone, Debug, Default)]
pub struct VndbState {
    pub search_query: String,
    pub search_results: Vec<VnSummary>,
    pub searching: bool,
    pub loading_active: bool,
    pub active: Option<ActiveVn>,
    pub last_error: Option<Arc<str>>,
}

impl VndbState {
    /// Borrow of the active VN's pre-rendered system-prompt fragment, if any.
    /// Returns `None` when no VN is active or the fragment is empty.
    pub fn prompt(&self) -> Option<&str> {
        self.active
            .as_ref()
            .map(|a| a.prompt.as_str())
            .filter(|s| !s.is_empty())
    }
}

#[derive(Clone)]
pub struct VndbHandle {
    inner: Arc<Inner>,
}

struct Inner {
    client: vndb::Client,
    ichiran: Ichiran,
    state: ArcSwap<VndbState>,
    /// Serialise read-modify-write on `state`. UI reads still go through
    /// `state.load()` lock-free.
    update_lock: Mutex<()>,
    /// At most one inject task in flight; each new task starts with a
    /// clear, so aborting the previous is safe.
    inject: Mutex<Option<JoinHandle<()>>>,
    on_prompt_change: Box<dyn Fn(Option<Arc<str>>) + Send + Sync>,
    /// Last prompt fragment we delivered, for content-based dedup.
    last_prompt: Mutex<Option<Arc<str>>>,
}

impl Inner {
    fn update<R>(&self, f: impl FnOnce(&mut VndbState) -> R) -> R {
        let _g = self.update_lock.lock().unwrap();
        let mut next = (**self.state.load()).clone();
        let r = f(&mut next);
        self.state.store(Arc::new(next));
        r
    }

    /// Recompute the active prompt fragment from `state` and fire the
    /// callback if it differs from the last one delivered.
    fn refresh_prompt(&self) {
        let now: Option<Arc<str>> = self
            .state
            .load()
            .active
            .as_ref()
            .filter(|a| !a.prompt.is_empty())
            .map(|a| Arc::from(a.prompt.as_str()));
        let mut last = self.last_prompt.lock().unwrap();
        let changed = match (&now, &*last) {
            (Some(a), Some(b)) => a.as_ref() != b.as_ref(),
            (None, None) => false,
            _ => true,
        };
        if changed {
            *last = now.clone();
            (self.on_prompt_change)(now);
        }
    }

    fn spawn_inject(&self, chars: Vec<Character>) {
        let mut slot = self.inject.lock().unwrap();
        if let Some(prev) = slot.take() {
            prev.abort();
        }
        let ichiran = self.ichiran.clone();
        *slot = Some(tokio::spawn(
            run_inject(ichiran, chars).instrument(tracing::Span::current()),
        ));
    }
}

pub fn spawn<F>(on_prompt_change: F, ichiran: Ichiran) -> VndbHandle
where
    F: Fn(Option<Arc<str>>) + Send + Sync + 'static,
{
    let inner = Arc::new(Inner {
        client: vndb::Client::new(),
        ichiran,
        state: ArcSwap::from_pointee(VndbState::default()),
        update_lock: Mutex::new(()),
        inject: Mutex::new(None),
        on_prompt_change: Box::new(on_prompt_change),
        last_prompt: Mutex::new(None),
    });
    // No eager cleanup. Every real action (`set_active`,
    // `set_active_by_id`, `clear_active`) starts its own inject job
    // with `clear_custom_entries` -- letting that one owner do the
    // work avoids racing the ichiran-cli pool's lazy init with an
    // abort, which can drop pool state mid-startup and leave the
    // user's first gloss looking up names that haven't been inserted.
    VndbHandle { inner }
}

impl VndbHandle {
    pub fn state(&self) -> Arc<VndbState> {
        self.inner.state.load_full()
    }

    pub fn search(&self, params: SearchParams) {
        let query = params.query.clone();
        self.inner.update(|s| {
            s.searching = true;
            s.search_query = query.clone();
        });
        let inner = self.inner.clone();
        tokio::spawn(
            async move {
                let result = inner.client.search_vn(&params).await;
                inner.update(|s| match result {
                    // Drop the response if the user has typed something
                    // else since this fetch was issued.
                    Ok(results) if s.search_query == query => {
                        s.search_results = results;
                        s.searching = false;
                    }
                    Ok(_) => {}
                    Err(err) if s.search_query == query => {
                        tracing::error!(?err, "vndb search failed");
                        s.searching = false;
                        s.last_error = Some(Arc::from(err.to_string()));
                    }
                    Err(err) => tracing::debug!(?err, "vndb search (superseded)"),
                });
            }
            .instrument(tracing::Span::current()),
        );
    }

    pub fn set_active(&self, summary: VnSummary) {
        self.inner.update(|s| {
            s.loading_active = true;
            s.active = Some(ActiveVn {
                summary: summary.clone(),
                characters: Vec::new(),
                prompt: String::new(),
            });
        });
        let inner = self.inner.clone();
        tokio::spawn(
            async move { load_characters(inner, summary).await }
                .instrument(tracing::Span::current()),
        );
    }

    pub fn set_active_by_id(&self, id: String) {
        // Stake out the slot synchronously with a placeholder summary
        // so any racing set_active / clear_active sees us occupying
        // `state.active` and the id-based supersession check works.
        self.inner.update(|s| {
            s.loading_active = true;
            s.active = Some(ActiveVn {
                summary: placeholder_summary(&id),
                characters: Vec::new(),
                prompt: String::new(),
            });
        });
        let inner = self.inner.clone();
        tokio::spawn(
            async move {
                let summary = match inner.client.vn_by_id(&id).await {
                    Ok(Some(s)) => s,
                    Ok(None) => {
                        inner.update(|s| {
                            if active_id_is(s, &id) {
                                s.active = None;
                                s.loading_active = false;
                                s.last_error =
                                    Some(Arc::from(format!("vndb id {id} not found")));
                            }
                        });
                        return;
                    }
                    Err(err) => {
                        inner.update(|s| {
                            if active_id_is(s, &id) {
                                s.active = None;
                                s.loading_active = false;
                                s.last_error = Some(Arc::from(err.to_string()));
                            }
                        });
                        return;
                    }
                };
                // Bail before phase 2 if we've been superseded.
                if !active_id_is(&inner.state.load(), &id) {
                    return;
                }
                load_characters(inner, summary).await;
            }
            .instrument(tracing::Span::current()),
        );
    }

    pub fn clear_active(&self) {
        self.inner.update(|s| {
            s.active = None;
            s.loading_active = false;
        });
        self.inner.refresh_prompt();
        self.inner.spawn_inject(Vec::new());
    }
}

/// Stage two of `set_active` / `set_active_by_id`: fetch characters
/// for `summary` and apply, but only if the user hasn't superseded us.
async fn load_characters(inner: Arc<Inner>, summary: VnSummary) {
    let id = summary.id.clone();
    match inner.client.characters(&id).await {
        Ok(characters) => {
            let prompt = build_prompt(&summary, &characters);
            let chars_for_inject = characters.clone();
            let landed = inner.update(|s| {
                if !active_id_is(s, &id) {
                    return false;
                }
                s.active = Some(ActiveVn {
                    summary,
                    characters,
                    prompt,
                });
                s.loading_active = false;
                true
            });
            if landed {
                inner.refresh_prompt();
                inner.spawn_inject(chars_for_inject);
            }
        }
        Err(err) => {
            tracing::error!(?err, "vndb characters failed");
            let landed = inner.update(|s| {
                if !active_id_is(s, &id) {
                    return false;
                }
                s.active = None;
                s.loading_active = false;
                s.last_error = Some(Arc::from(err.to_string()));
                true
            });
            if landed {
                inner.refresh_prompt();
                inner.spawn_inject(Vec::new());
            }
        }
    }
}

fn active_id_is(state: &VndbState, id: &str) -> bool {
    state.active.as_ref().map(|a| a.summary.id.as_str()) == Some(id)
}

/// Stub `VnSummary` we plant into `state.active` while a `set_active_by_id`
/// is in flight, before the real summary has arrived. Only the `id` matters
/// for the supersession check; the title is filler the UI may briefly show.
fn placeholder_summary(id: &str) -> VnSummary {
    VnSummary {
        id: id.to_string(),
        title: "...".to_string(),
        alttitle: None,
        released: None,
        length: None,
        rating: None,
        votecount: None,
        description: None,
        developers: Vec::new(),
        platforms: Vec::new(),
    }
}

/// Replace the set of injected character entries in the ichiran DB
/// with `characters` (empty `characters` just clears). Safe to abort
/// mid-flight: the supersedor's clear sweeps any partial rows.
async fn run_inject(ichiran: Ichiran, characters: Vec<Character>) {
    if let Err(err) = ichiran.clear_custom_entries().await {
        tracing::warn!(?err, "vndb: clear_custom_entries failed");
        return;
    }

    // Collect drafts, then group by (kanji, kana) so two characters
    // sharing a surname or given name produce a single dictionary
    // entry with both full names listed in the gloss (mirrors how
    // JMdict joins proper-noun glosses with `; `).
    let mut drafts: Vec<DraftEntry> = Vec::new();
    for ch in &characters {
        drafts.extend(build_entries(ch, &ichiran).await);
    }
    let entries = group_drafts(drafts);

    let mut injected = 0;
    for entry in &entries {
        if let Err(err) = ichiran.add_custom_entry(entry).await {
            tracing::warn!(?err, kanji = ?entry.kanji, kana = %entry.kana, "vndb: add_custom_entry failed");
            continue;
        }
        injected += 1;
    }
    tracing::info!(
        injected,
        characters = characters.len(),
        "vndb: name injection complete",
    );
}

/// One name token paired with the full character name it came from.
/// Multiple drafts with the same `(kanji, kana)` are merged into a
/// single [`CustomEntry`] by [`group_drafts`].
struct DraftEntry {
    kanji: Option<String>,
    kana: String,
    full_name: String,
}

fn group_drafts(drafts: Vec<DraftEntry>) -> Vec<CustomEntry> {
    let mut groups: Vec<(Option<String>, String, Vec<String>)> = Vec::new();
    for d in drafts {
        match groups
            .iter_mut()
            .find(|(k, n, _)| k == &d.kanji && n == &d.kana)
        {
            Some((_, _, names)) => {
                if !names.iter().any(|n| n == &d.full_name) {
                    names.push(d.full_name);
                }
            }
            None => groups.push((d.kanji, d.kana, vec![d.full_name])),
        }
    }
    groups
        .into_iter()
        .map(|(kanji, kana, names)| CustomEntry {
            kanji,
            kana,
            pos: "n-pr".to_string(),
            gloss: names.join("; "),
        })
        .collect()
}

/// Build [`DraftEntry`]s for one character. Multi-token names get one
/// draft per token *and* one for the full concatenated form, since
/// surnames and given names commonly appear on their own in dialogue
/// (with or without さん/くん/様 honorifics, which ichiran handles as
/// suffixes once the name itself is known).
///
/// Per-token pairing assumes `original` and `name` use the same word
/// order. VNDB doesn't strictly enforce that -- when a character has
/// Western-order romaji ("Kyousuke Saionji") against Eastern-order
/// kanji (西園寺 京介), the per-token readings will be swapped. The
/// kanji-text match still wires up segmentation correctly; only the
/// romaji rendering of the swapped tokens is wrong.
async fn build_entries(ch: &Character, ichiran: &Ichiran) -> Vec<DraftEntry> {
    let original = match ch.original.as_deref().map(str::trim) {
        Some(s) if !s.is_empty() => s,
        _ => return Vec::new(),
    };
    let original_tokens: Vec<&str> = original.split_whitespace().collect();
    let name_tokens: Vec<&str> = ch.name.split_whitespace().collect();

    let mut out = Vec::new();
    let push_entry = async |kanji_or_kana: &str, romaji: &str, out: &mut Vec<DraftEntry>| {
        if let Some(e) = make_draft(kanji_or_kana, romaji, &ch.name, ichiran).await {
            out.push(e);
        }
    };

    // Per-token: catches surname-only / given-name-only mentions, the
    // common case in dialogue.
    if original_tokens.len() > 1 && original_tokens.len() == name_tokens.len() {
        for (k, r) in original_tokens.iter().zip(name_tokens.iter()) {
            push_entry(k, r, &mut out).await;
        }
    }

    // Full concatenated form: catches the whole name appearing as one
    // chunk. (For single-token names this is the only entry emitted.)
    let full_kanji: String = original_tokens.concat();
    let full_romaji: String = name_tokens.concat();
    push_entry(&full_kanji, &full_romaji, &mut out).await;

    // Japanese aliases. VNDB returns aliases as flat strings with no
    // paired reading, so we can only inject the kana-only ones --
    // anything containing kanji has no derivable reading. Latin-script
    // aliases (English nicknames, translations) aren't useful for the
    // segmenter and are dropped.
    for alias in &ch.aliases {
        let alias = alias.trim();
        if alias.is_empty() {
            continue;
        }
        let chars: Vec<char> = alias.chars().collect();
        let has_kanji = chars.iter().any(is_kanji);
        let has_kana = chars.iter().any(|c| is_hiragana(c) || is_katakana(c));
        if has_kanji || !has_kana {
            continue;
        }
        if let Some(e) = make_draft(alias, "", &ch.name, ichiran).await {
            out.push(e);
        }
    }

    out
}

/// Build one [`DraftEntry`] for the given kanji-or-kana token and its
/// matching romaji. Returns `None` if the token is empty or the kana
/// reading can't be derived.
async fn make_draft(
    token: &str,
    romaji: &str,
    full_name: &str,
    ichiran: &Ichiran,
) -> Option<DraftEntry> {
    let token = token.trim();
    if token.is_empty() {
        return None;
    }
    let has_kanji = token.chars().any(|c| is_kanji(&c));

    let (kanji, kana) = if !has_kanji {
        // Pure-kana token -- emit no <k_ele>, matching dict-custom's
        // `as-xml-simple` shape.
        (None, token.to_string())
    } else {
        let romaji = romaji.trim();
        if romaji.is_empty() {
            return None;
        }
        let kana = match ichiran.romaji_to_kana(romaji).await {
            Ok(k) if !k.trim().is_empty() => k.trim().to_string(),
            Ok(_) => return None,
            Err(err) => {
                tracing::debug!(?err, %romaji, "romaji_to_kana failed");
                return None;
            }
        };
        (Some(token.to_string()), kana)
    };

    Some(DraftEntry {
        kanji,
        kana,
        full_name: full_name.to_string(),
    })
}

fn build_prompt(summary: &VnSummary, characters: &[Character]) -> String {
    let mut out = String::new();
    out.push_str(VNDB_PROMPT_PREAMBLE);
    out.push_str("\n\nVisual novel: ");
    out.push_str(&summary.title);
    if let Some(alt) = &summary.alttitle {
        let _ = write!(out, " ({})", alt);
    }
    out.push_str("\n\nCharacters:\n");
    for ch in characters {
        out.push_str("- ");
        if let Some(orig) = &ch.original {
            out.push_str(orig);
            let _ = write!(out, " ({})", ch.name);
        } else {
            out.push_str(&ch.name);
        }
        if let Some(sex) = ch.sex {
            let _ = write!(out, " [{}]", sex.label());
        }
        if let Some(role) = ch.role_for(&summary.id) {
            let _ = write!(out, " [{}]", role);
        }
        let aliases: Vec<&str> = ch
            .aliases
            .iter()
            .map(|a| a.trim())
            .filter(|a| !a.is_empty())
            .collect();
        if !aliases.is_empty() {
            let _ = write!(out, " (aka: {})", aliases.join("; "));
        }
        if let Some(desc) = &ch.description {
            let cleaned = clean_description(desc);
            if !cleaned.is_empty() {
                out.push_str(": ");
                // Flatten -- one character, one line.
                for (i, segment) in cleaned.split('\n').filter(|s| !s.trim().is_empty()).enumerate()
                {
                    if i > 0 {
                        out.push_str(" / ");
                    }
                    out.push_str(segment.trim());
                }
            }
        }
        out.push('\n');
    }
    out
}

/// Strip VNDB BBCode markup from a description and drop the contents of any
/// [spoiler]...[/spoiler] regions. We don't want spoilers leaking into the
/// translator, and the markup is dead weight either way.
pub fn clean_description(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // [spoiler] ... [/spoiler] -- drop entirely (case-insensitive).
        if let Some(end) = match_tag_close(bytes, i, b"spoiler") {
            if let Some(close_start) = find_close_tag(s, end, "spoiler") {
                if let Some(after) = match_tag_close(bytes, close_start, b"/spoiler") {
                    i = after;
                    continue;
                }
            }
            // No matching close tag -- drop the rest of the string.
            break;
        }
        // [url=...]text[/url] -- keep just the visible text.
        if let Some(after_open) = match_url_open(bytes, i) {
            if let Some(close_start) = find_close_tag(s, after_open, "url") {
                out.push_str(&s[after_open..close_start]);
                if let Some(after_close) = match_tag_close(bytes, close_start, b"/url") {
                    i = after_close;
                    continue;
                }
            }
        }
        // Any other [tag] or [/tag] -- strip just the bracketed token.
        if bytes[i] == b'[' {
            if let Some(end) = s[i..].find(']') {
                i += end + 1;
                continue;
            }
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    // Collapse runs of blank lines.
    let mut result = String::with_capacity(out.len());
    let mut blanks = 0;
    for line in out.lines() {
        if line.trim().is_empty() {
            blanks += 1;
            if blanks <= 1 {
                result.push('\n');
            }
        } else {
            blanks = 0;
            result.push_str(line);
            result.push('\n');
        }
    }
    result.trim().to_string()
}

/// If the bytes starting at `i` form `[<tag>]` (case-insensitive), return
/// the index just past the closing `]`.
fn match_tag_close(bytes: &[u8], i: usize, tag: &[u8]) -> Option<usize> {
    if bytes.get(i)? != &b'[' {
        return None;
    }
    let inner = bytes.get(i + 1..i + 1 + tag.len())?;
    if !inner.eq_ignore_ascii_case(tag) {
        return None;
    }
    let after = i + 1 + tag.len();
    if bytes.get(after)? == &b']' {
        Some(after + 1)
    } else {
        None
    }
}

/// `[url=...]` -- return the index just past the closing `]`.
fn match_url_open(bytes: &[u8], i: usize) -> Option<usize> {
    if bytes.get(i)? != &b'[' {
        return None;
    }
    let inner = bytes.get(i + 1..i + 5)?;
    if !inner.eq_ignore_ascii_case(b"url=") {
        return None;
    }
    let rest = &bytes[i + 5..];
    let end = rest.iter().position(|&b| b == b']')?;
    Some(i + 5 + end + 1)
}

/// Find the byte offset of `[/<tag>]` (case-insensitive) starting at `from`.
fn find_close_tag(s: &str, from: usize, tag: &str) -> Option<usize> {
    let needle = format!("[/{}", tag);
    let lower = s[from..].to_ascii_lowercase();
    let pos = lower.find(&needle.to_ascii_lowercase())?;
    let abs = from + pos;
    let after = abs + needle.len();
    if s.as_bytes().get(after)? == &b']' {
        Some(abs)
    } else {
        None
    }
}
