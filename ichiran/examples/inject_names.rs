//! Prototype: inject character names into the live ichiran DB and
//! verify that segmentation picks them up.
//!
//! Runs `romanize` before, calls `add_custom_entry` for a couple of
//! names, runs `romanize` again, then deletes the injected rows via
//! `clear_custom_entries`. Seqs are allocated internally from
//! `CUSTOM_SEQ_BASE`; callers don't choose them.
//!
//! Usage:
//!     cargo run --release --example inject_names -p ichiran
//!
//! NOTE: this WRITES to the running ichiran Postgres database. It
//! cleans up on exit, but if the process is killed mid-run you can
//! manually clean up with:
//!     DELETE FROM entry WHERE seq >= 1000000000;

use std::path::PathBuf;

use ichiran::prelude::*;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let path = PathBuf::from("data/ichiran-cli").with_extension(std::env::consts::EXE_EXTENSION);
    let ichiran = Ichiran::new(path, default_pool_size());

    // Make sure the sentinel range is empty before we start, so a
    // previous crashed run doesn't bias the "before" result.
    ichiran.clear_custom_entries().await.unwrap();

    let cases = [
        ("\u{4eac}\u{4ecb}\u{306f}\u{6765}\u{305f}", &[
            CustomEntry {
                kanji: Some("\u{4eac}\u{4ecb}".to_string()),
                kana: "\u{304d}\u{3087}\u{3046}\u{3059}\u{3051}".to_string(),
                pos: "n-pr".to_string(),
                gloss: "Kyousuke (given name)".to_string(),
            },
        ][..]),
    ];

    for (text, entries) in cases {
        println!("=== {} ===", text);

        let splits: Vec<(Split, String)> = basic_split(text)
            .into_iter()
            .map(|(k, s)| (k, s.to_string()))
            .collect();

        let before = ichiran.romanize(&splits, 1).await.unwrap();
        println!("BEFORE: {}", summarize(&before));

        for entry in entries {
            let seq = ichiran.add_custom_entry(entry).await.unwrap();
            println!(
                "injected (seq={}): {:?} [{}] -> {}",
                seq,
                entry.kanji.as_deref().unwrap_or(""),
                entry.kana,
                entry.gloss,
            );
        }

        let after = ichiran.romanize(&splits, 1).await.unwrap();
        println!("AFTER:  {}", summarize(&after));

        ichiran.clear_custom_entries().await.unwrap();
        println!("cleaned up");
    }
}

fn summarize(root: &Root) -> String {
    let mut out = String::new();
    for seg in root.segments() {
        match seg {
            Segment::Skipped(s) => {
                out.push_str(&format!("[skip {:?}] ", s));
            }
            Segment::Clauses(clauses) => {
                if let Some(c) = clauses.first() {
                    let parts: Vec<String> = c
                        .romanized()
                        .iter()
                        .map(|r| format!("{}({})", r.term().text(), r.romaji()))
                        .collect();
                    out.push_str(&parts.join(" + "));
                    out.push(' ');
                }
            }
        }
    }
    out
}
