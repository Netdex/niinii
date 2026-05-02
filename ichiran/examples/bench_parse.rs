//! Steady-state parse-pathway benchmark.
//!
//! Mirrors what `niinii::Parser::parse` does: for each iteration runs
//! `romanize` and `kanji_from_str` in parallel via `tokio::try_join!`.
//! The pool is lazily initialized on the first call, so the first
//! iteration includes worker startup; subsequent iterations are warm.
//!
//! Usage:
//!     cargo run --release --example bench_parse -- [N] [TEXT]

use std::{path::PathBuf, time::Instant};

use ichiran::prelude::*;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let mut args = std::env::args().skip(1);
    let n: usize = args.next().as_deref().unwrap_or("5").parse().unwrap();

    // Distinct sentences per iter to defeat `segment_cache`. Picked to roughly
    // mirror visual-novel-line lengths.
    let corpus: &[&str] = &[
        "これは長い日本語の文章のテストです。",
        "今日は晴れていて気持ちがいいですね。",
        "彼女は静かに窓の外を眺めていた。",
        "夕食は何にしましょうかと母が尋ねた。",
        "雨が降り始めたので傘を持って出かけた。",
        "新しい本を買ったので読むのが楽しみだ。",
        "電車の中で偶然友達に会った。",
        "公園で子供たちが楽しそうに遊んでいる。",
        "明日の会議は十時から始まります。",
        "猫が日向でゆっくりと眠っている。",
    ];

    let path = PathBuf::from("data/ichiran-cli").with_extension(std::env::consts::EXE_EXTENSION);
    let ichiran = Ichiran::new(path, default_pool_size());

    let total_start = Instant::now();
    for i in 0..n {
        let text = corpus[i % corpus.len()];
        let splits: Vec<(Split, String)> = basic_split(text)
            .into_iter()
            .map(|(k, s)| (k, s.to_string()))
            .collect();
        let t = Instant::now();
        let (_root, _kanji) = tokio::try_join!(
            ichiran.romanize(&splits, 1),
            ichiran.kanji_from_str(text),
        )
        .unwrap();
        eprintln!("iter {}: {:.0?}", i, t.elapsed());
    }
    let elapsed = total_start.elapsed();
    eprintln!(
        "total: {:.0?}  mean/iter: {:.0?}",
        elapsed,
        elapsed / n as u32
    );
}
