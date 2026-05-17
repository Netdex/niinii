mod charset;
mod coerce;
mod error;
mod pgdaemon;
mod protocol;
mod server;
pub mod split;

use std::{
    collections::HashMap,
    io::{self, ErrorKind},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use enclose::enclose;
use futures::{stream, TryStreamExt};
use itertools::Itertools;
use lru::LruCache;
use nonzero_ext::nonzero;
use par_stream::ParStreamExt;
use tokio::sync::OnceCell;
use tracing::{Instrument, Level};

use crate::server::IchiranPool;

/// Suggested default pool size: cap at 8 to avoid spinning up more
/// resident `ichiran-cli` workers than there's parallel benefit for
/// (a single parse fans out at most ~20 calls but the longest call
/// caps the critical path).
pub fn default_pool_size() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(8)
}

pub mod prelude {
    pub use crate::charset::*;
    pub use crate::error::*;
    pub use crate::pgdaemon::*;
    pub use crate::protocol::*;
    pub use crate::split::{basic_split, Split};
    pub use crate::*;
}
use prelude::*;

#[derive(Debug)]
pub struct ConnParams {
    pub database: String,
    pub user: String,
    pub password: String,
    pub hostname: String,
    pub port: u16,
}

#[derive(Clone)]
pub struct Ichiran {
    shared: Arc<Shared>,
}

struct Shared {
    path: PathBuf,
    pool_size: usize,
    state: Mutex<State>,
    pool: OnceCell<IchiranPool>,
}
impl Shared {
    /// Evaluate the expression with ichiran and return the raw output.
    async fn evaluate(&self, expr: impl Into<String>) -> Result<String, IchiranError> {
        let pool = self
            .pool
            .get_or_try_init(|| IchiranPool::spawn(&self.path, self.pool_size))
            .await?;
        pool.evaluate(expr.into()).await
    }
    fn working_dir(&self) -> Result<&Path, io::Error> {
        Path::new(&self.path).parent().ok_or_else(|| {
            io::Error::new(
                ErrorKind::NotFound,
                "Could not find working directory of ichiran-cli",
            )
        })
    }
    async fn jmdict_path(&self) -> Result<PathBuf, IchiranError> {
        let working_dir = self.working_dir()?;
        let jmdict_path = self
            .evaluate(r#"(format t "~d" ichiran/dict::*jmdict-data*)"#)
            .await?;
        Ok(working_dir.join(jmdict_path.trim()))
    }
}

/// Escape a Rust string into a Common Lisp `"..."` literal. Lisp string
/// syntax only requires escaping `"` and `\`; everything else (including
/// newlines and non-ASCII) is literal.
fn lisp_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A custom dictionary entry to inject via [`Ichiran::add_custom_entry`].
///
/// Pure-kana names (e.g. アリス) should set `kanji = None`; otherwise both
/// fields are required. `pos` is a JMdict part-of-speech code -- use
/// `"n-pr"` for proper nouns (the right choice for character names).
#[derive(Debug, Clone)]
pub struct CustomEntry {
    pub kanji: Option<String>,
    pub kana: String,
    pub pos: String,
    pub gloss: String,
}

impl CustomEntry {
    fn to_xml(&self) -> String {
        let mut out = String::new();
        out.push_str("<entry><ent_seq></ent_seq>");
        if let Some(kanji) = &self.kanji {
            out.push_str("<k_ele><keb>");
            xml_escape_into(kanji, &mut out);
            out.push_str("</keb></k_ele>");
        }
        out.push_str("<r_ele><reb>");
        xml_escape_into(&self.kana, &mut out);
        out.push_str("</reb></r_ele><sense><pos>");
        xml_escape_into(&self.pos, &mut out);
        out.push_str("</pos><gloss xml:lang=\"eng\">");
        xml_escape_into(&self.gloss, &mut out);
        out.push_str("</gloss></sense></entry>");
        out
    }
}

fn xml_escape_into(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
}

/// Seq sentinel for entries inserted via [`Ichiran::add_custom_entry`].
/// JMdict's real seqs sit in the low millions, so this is well above
/// any upstream row but well below i32 max (`entry.seq` is a 32-bit
/// integer in the schema). Exposed so callers can detect "is this term
/// one we injected?" -- e.g. for highlighting -- but the seq itself is
/// allocated internally and never accepted from the outside, so no
/// caller can collide with a real JMdict entry by passing the wrong
/// number.
pub const CUSTOM_SEQ_BASE: u32 = 1_000_000_000;

struct State {
    kanji_cache: LruCache<char, Kanji>,
    segment_cache: LruCache<String, Segment>,
    jmdict: Option<JmDictData>,
    /// Next seq to hand out from [`CUSTOM_SEQ_BASE`]. Reset by
    /// [`Ichiran::clear_custom_entries`].
    next_custom_seq: u32,
}

impl Ichiran {
    pub fn new(path: impl Into<PathBuf>, pool_size: usize) -> Self {
        assert!(pool_size >= 1, "pool size must be >= 1");
        Self {
            shared: Arc::new(Shared {
                path: path.into(),
                pool_size,
                state: Mutex::new(State {
                    kanji_cache: LruCache::new(nonzero!(512usize)),
                    segment_cache: LruCache::new(nonzero!(512usize)),
                    jmdict: None,
                    next_custom_seq: CUSTOM_SEQ_BASE,
                }),
                pool: OnceCell::new(),
            }),
        }
    }

    #[tracing::instrument(level = Level::DEBUG, skip_all, err)]
    pub async fn romanize(
        &self,
        splits: &[(Split, String)],
        limit: u32,
    ) -> Result<Root, IchiranError> {
        assert!(limit > 0);

        let shared = self.shared.clone();

        // determine minimal candidate queries from splits
        let split_queries: Vec<_> = splits
            .iter()
            .filter_map(|split| match split {
                (Split::Text, text) => Some(text),
                _ => None,
            })
            .sorted_unstable()
            .unique()
            .cloned()
            .collect();

        let mut segment_table: HashMap<String, Segment> = {
            let segment_cache = &mut shared.state.lock().unwrap().segment_cache;
            // for entries which are in segment cache, use cached value
            split_queries
                .iter()
                .filter_map(|text| {
                    segment_cache
                        .get(text)
                        .map(|segment| (text.clone(), segment.clone()))
                })
                .collect::<HashMap<String, Segment>>()
        };

        let split_queries: Vec<_> = split_queries
            .into_iter()
            .filter(|query| !segment_table.contains_key(query))
            .collect();

        // for entries which are not in the cache, query ichiran
        let span = tracing::Span::current();
        let query_table: HashMap<String, Segment> = stream::iter(split_queries)
            .par_then_unordered(
                None,
                enclose! { (span, shared) move |split: String| {
                    enclose! { (span, shared) async move {
                        let output = shared
                            .evaluate(format!(
                                r#"(princ (jsown:to-json (ichiran:romanize* {} :limit {})))"#,
                                lisp_string(&split),
                                limit
                            ))
                            .await?;
                        let root: Root = serde_json::from_str(&output)?;
                        assert_eq!(
                            root.segments().len(),
                            1,
                            "unexpected number of segments",
                        );
                        let segment = root.segments()[0].clone();
                        Ok::<_, IchiranError>((split.clone(), segment))
                    }.instrument(span)
                }}},
            )
            .try_collect()
            .await?;

        // put queried entries into segment cache
        let segment_cache = &mut shared.state.lock().unwrap().segment_cache;
        for (k, v) in &query_table {
            segment_cache.push(k.clone(), v.clone());
        }
        segment_table.extend(query_table);

        let segments: Vec<_> = splits
            .iter()
            .map(|split| match split {
                (Split::Text, text) => segment_table.get(text).cloned().unwrap(),
                (Split::Skip, skip) => Segment::Skipped(skip.clone()),
            })
            .collect();

        Ok(Root(segments))
    }

    #[tracing::instrument(level = Level::DEBUG, skip_all, err)]
    pub async fn kanji(&self, chars: &[char]) -> Result<HashMap<char, Kanji>, IchiranError> {
        let (mut kanji_info, query_chars): (HashMap<char, Kanji>, Vec<char>) = {
            let kanji_cache = &mut self.shared.state.lock().unwrap().kanji_cache;
            let kanji_info = chars
                .iter()
                .filter_map(|c| kanji_cache.get(c).map(|kanji| (*c, kanji.clone())))
                .collect();
            let query_chars = chars
                .iter()
                .filter(|c| !kanji_cache.contains(c))
                .copied()
                .collect();
            (kanji_info, query_chars)
        };

        if query_chars.is_empty() {
            return Ok(kanji_info);
        }

        // Fan out per char so the pool can dispatch them across workers in
        // parallel. Previously these were batched into one (list ...) form
        // which serialized through a single worker.
        let shared = self.shared.clone();
        let span = tracing::Span::current();
        let results: Vec<(char, Option<Kanji>)> = stream::iter(query_chars)
            .par_then_unordered(
                None,
                enclose! { (span, shared) move |c: char| {
                    enclose! { (span, shared) async move {
                        let expr = format!(
                            r#"(princ (jsown:to-json (ichiran/kanji:kanji-info-json #\{})))"#,
                            c
                        );
                        let output = shared.evaluate(expr).await?;
                        let kanji = if output.trim() == "[]" {
                            None
                        } else {
                            Some(serde_json::from_str::<Kanji>(&output)?)
                        };
                        Ok::<_, IchiranError>((c, kanji))
                    }.instrument(span)}
                }},
            )
            .try_collect()
            .await?;

        let kanji_cache = &mut self.shared.state.lock().unwrap().kanji_cache;
        for (chr, maybe_kanji) in results {
            if let Some(kanji) = maybe_kanji {
                kanji_info.insert(chr, kanji.clone());
                kanji_cache.put(chr, kanji);
            }
        }
        Ok(kanji_info)
    }

    pub async fn kanji_from_str(
        &self,
        text: impl AsRef<str>,
    ) -> Result<HashMap<char, Kanji>, IchiranError> {
        let text = text.as_ref();
        let mut uniq: Vec<char> = text.chars().filter(is_kanji).collect();
        uniq.sort_unstable();
        uniq.dedup();
        self.kanji(&uniq).await
    }

    pub async fn jmdict_data(&self) -> Result<JmDictData, IchiranError> {
        {
            let state = self.shared.state.lock().unwrap();
            if let Some(jmdict) = &state.jmdict {
                return Ok(jmdict.clone());
            }
        }

        let jmdict_path = &self.shared.jmdict_path().await?;
        let jmdict = JmDictData::new(jmdict_path).await;

        if let Ok(jmdict) = &jmdict {
            let mut state = self.shared.state.lock().unwrap();
            state.jmdict.replace(jmdict.clone());
        }
        jmdict
    }

    /// Inject a custom dictionary entry into the running ichiran DB.
    ///
    /// Allocates a fresh seq from the [`CUSTOM_SEQ_BASE`] range; the
    /// caller never sees the seq, which makes it impossible to collide
    /// with real JMdict rows by passing the wrong number. Calls
    /// `ichiran/dict::load-entry` with `:if-exists :overwrite` and
    /// `:conjugate-p nil`.
    ///
    /// Returns the allocated seq, mostly so callers can correlate
    /// terms in romanize output with their injected source. Most
    /// callers can ignore it.
    ///
    /// Clears the segment cache so subsequent romanize calls see the
    /// new entry. The kanji cache is left alone (per-char DB lookups
    /// stay correct).
    #[tracing::instrument(level = Level::DEBUG, skip_all, err)]
    pub async fn add_custom_entry(&self, entry: &CustomEntry) -> Result<u32, IchiranError> {
        let seq = {
            let mut state = self.shared.state.lock().unwrap();
            let s = state.next_custom_seq;
            state.next_custom_seq = s.checked_add(1).expect("custom seq counter overflow");
            s
        };
        let xml = entry.to_xml();
        let expr = format!(
            r#"(postmodern:with-connection ichiran/conn:*connection* (ichiran/dict::load-entry {} :seq {} :if-exists :overwrite))"#,
            lisp_string(&xml),
            seq,
        );
        self.shared.evaluate(expr).await?;
        self.invalidate_segment_cache();
        Ok(seq)
    }

    /// Delete every entry [`add_custom_entry`] could have inserted.
    ///
    /// Resets the seq allocator and runs a single
    /// `DELETE FROM entry WHERE seq >= CUSTOM_SEQ_BASE`. FK cascades
    /// on `entry.seq` clean up `kanji_text`, `kana_text`, `sense`,
    /// `gloss`, `sense_prop`, `restricted_readings`, and the
    /// conjugation tables.
    #[tracing::instrument(level = Level::DEBUG, skip_all, err)]
    pub async fn clear_custom_entries(&self) -> Result<(), IchiranError> {
        let expr = format!(
            r#"(postmodern:with-connection ichiran/conn:*connection* (postmodern:query (:delete-from 'entry :where (:>= 'seq {}))))"#,
            CUSTOM_SEQ_BASE,
        );
        self.shared.evaluate(expr).await?;
        {
            let mut state = self.shared.state.lock().unwrap();
            state.next_custom_seq = CUSTOM_SEQ_BASE;
        }
        self.invalidate_segment_cache();
        Ok(())
    }

    /// Drop all cached segment results. Call after mutating the DB.
    pub fn invalidate_segment_cache(&self) {
        self.shared.state.lock().unwrap().segment_cache.clear();
    }

    /// Convert a romaji string to hiragana via ichiran's `romaji-kana`.
    /// Pure roman-to-kana with no DB dependency. Returns the canonical
    /// kana form.
    #[tracing::instrument(level = Level::DEBUG, skip(self), err)]
    pub async fn romaji_to_kana(&self, romaji: &str) -> Result<String, IchiranError> {
        let expr = format!(
            r#"(princ (ichiran:romaji-kana {}))"#,
            lisp_string(romaji),
        );
        let out = self.shared.evaluate(expr).await?;
        Ok(out.trim().to_string())
    }

    pub async fn conn_params(&self) -> Result<ConnParams, IchiranError> {
        let conn_params = self
            .shared
            .evaluate(r#"(format t "~{~a~^,~}" ichiran/conn::*connection*)"#)
            .await?;
        let parse_error = || IchiranError::Server(format!("parse error:\n{conn_params}"));

        let conn_params = conn_params
            .trim()
            .split(',')
            .collect_tuple()
            .ok_or_else(parse_error)?;

        let (database, user, password, hostname, _, port) = conn_params;
        let port = port.parse::<u16>().map_err(|_| parse_error())?;

        Ok(ConnParams {
            database: database.to_string(),
            user: user.to_string(),
            password: password.to_string(),
            hostname: hostname.to_string(),
            port,
        })
    }
}
#[cfg(test)]
mod tests {
    pub(crate) mod fixture;
}
