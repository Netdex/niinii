//! Latency benchmark: Realtime (WebSocket) vs Responses vs Chat Completions.
//!
//! Measures time-to-first-token (the primary metric), plus time-to-first-byte
//! and total stream duration, for each backend against the same prompt. All run
//! with minimum-latency settings: streaming, reasoning off, low verbosity (where
//! applicable), a bounded output cap, and (for responses) a stable prompt cache
//! key. Mirrors `ichiran/examples/bench_parse.rs`.
//!
//! The Realtime backend reuses one persistent WebSocket across iterations (the
//! connect cost is reported once, separately), so its per-turn TTFT excludes the
//! handshake -- which is the point of a realtime session. It needs a
//! realtime-capable model, which is usually NOT the configured `openai_model`,
//! so it uses [`BENCH_REALTIME_MODEL`] (env, default `gpt-realtime`).
//!
//! Config comes from the workspace `niinii.toml` (`openai_api_endpoint`,
//! `openai_model`, optional `openai_api_key`), the same keys the integration
//! tests read. Prints a SKIP notice and exits if absent, so it is always safe
//! to invoke.
//!
//! Usage:
//!     cargo run --release -p openai --example bench_latency -- [N] [chat|responses|realtime|both|all]
//!     BENCH_REALTIME_MODEL=gpt-realtime-2 cargo run ... -- 10 all

use std::time::{Duration, Instant};

use openai::{
    chat,
    realtime::{ClientEvent, Item, Realtime, RealtimeReasoning, ServerEvent, SessionConfig},
    responses::{self, Input, InputItem, Reasoning, StreamEvent, TextConfig},
    Client, ConnectionPolicy, ModelId, ReasoningEffort, Role, Verbosity,
};
use tokio_stream::StreamExt;

const CONFIG_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../niinii.toml");

/// Bounded output keeps total runtime predictable; it does not affect TTFT.
const MAX_TOKENS: u32 = 256;

/// Realtime-capable model for the realtime backend, overridable via the
/// `BENCH_REALTIME_MODEL` env var. The configured `openai_model` is typically a
/// chat/responses model that the Realtime API rejects.
fn realtime_model() -> ModelId {
    ModelId(
        std::env::var("BENCH_REALTIME_MODEL").unwrap_or_else(|_| "gpt-realtime".to_string()),
    )
}

/// Optional realtime reasoning effort via `BENCH_REALTIME_EFFORT` (minimal/low/
/// medium/high/xhigh). Unset = omit the field (server default). Only meaningful
/// for reasoning-capable realtime models (e.g. `gpt-realtime-2`).
fn realtime_effort() -> Option<ReasoningEffort> {
    match std::env::var("BENCH_REALTIME_EFFORT").ok()?.to_lowercase().as_str() {
        "minimal" => Some(ReasoningEffort::Minimal),
        "low" => Some(ReasoningEffort::Low),
        "medium" => Some(ReasoningEffort::Medium),
        "high" => Some(ReasoningEffort::High),
        "xhigh" => Some(ReasoningEffort::Xhigh),
        _ => None,
    }
}

struct Cfg {
    endpoint: String,
    model: String,
    api_key: String,
}

/// Read the shared top-level `niinii.toml` keys, or `None` if missing.
fn load_config() -> Option<Cfg> {
    let text = std::fs::read_to_string(CONFIG_PATH).ok()?;
    let v: toml::Value = toml::from_str(&text).ok()?;
    let endpoint = v.get("openai_api_endpoint")?.as_str()?.to_string();
    let model = v.get("openai_model")?.as_str()?.to_string();
    let api_key = v
        .get("openai_api_key")
        .and_then(|k| k.as_str())
        .unwrap_or("no-key")
        .to_string();
    Some(Cfg {
        endpoint,
        model,
        api_key,
    })
}

/// One request's timing breakdown. `None` fields mean the event never occurred
/// (e.g. no reasoning deltas on a non-thinking model).
#[derive(Default)]
struct Sample {
    /// Time to the first SSE event of any kind (network + queueing).
    ttfb: Option<Duration>,
    /// Time to the first non-empty output-text delta -- the headline metric.
    ttft: Option<Duration>,
    /// Time to the first reasoning delta, if the model streams one.
    ttfr: Option<Duration>,
    /// Time to stream end.
    total: Duration,
    /// Output-text characters received.
    chars: usize,
}

async fn bench_chat(
    client: &Client,
    model: ModelId,
    system: &str,
    user_line: &str,
) -> Result<Sample, openai::Error> {
    let req = chat::Request::builder()
        .model(model)
        .messages(vec![
            chat::Message {
                role: chat::Role::System,
                content: Some(system.into()),
                ..Default::default()
            },
            chat::Message {
                role: chat::Role::User,
                content: Some(user_line.into()),
                ..Default::default()
            },
        ])
        .reasoning_effort(ReasoningEffort::None)
        .verbosity(Verbosity::Low)
        .max_completion_tokens(MAX_TOKENS)
        .build();

    let t0 = Instant::now();
    let mut stream = client.stream(req).await?;
    let mut s = Sample::default();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if s.ttfb.is_none() {
            s.ttfb = Some(t0.elapsed());
        }
        for choice in &chunk.choices {
            if let Some(rc) = &choice.delta.reasoning_content {
                if !rc.is_empty() && s.ttfr.is_none() {
                    s.ttfr = Some(t0.elapsed());
                }
            }
            if let Some(c) = &choice.delta.content {
                if !c.is_empty() {
                    if s.ttft.is_none() {
                        s.ttft = Some(t0.elapsed());
                    }
                    s.chars += c.chars().count();
                }
            }
        }
    }
    s.total = t0.elapsed();
    Ok(s)
}

async fn bench_responses(
    client: &Client,
    model: ModelId,
    system: &str,
    user_line: &str,
) -> Result<Sample, openai::Error> {
    let req = responses::Request::builder()
        .model(model)
        .input(Input::Items(vec![InputItem {
            role: Role::User,
            content: user_line.into(),
        }]))
        .instructions(system.to_string())
        .reasoning(Reasoning {
            effort: Some(ReasoningEffort::None),
            // Summaries cost extra latency; omit them for the min-latency path.
            summary: None,
        })
        .text(TextConfig {
            verbosity: Some(Verbosity::Low),
        })
        .max_output_tokens(MAX_TOKENS)
        // Independent requests: do not chain or persist state.
        .store(false)
        // Stable key raises the prefix-cache hit rate, lowering TTFT.
        .prompt_cache_key("bench-latency".to_string())
        .build();

    let t0 = Instant::now();
    let mut stream = client.stream_responses(req).await?;
    let mut s = Sample::default();
    while let Some(event) = stream.next().await {
        let event = event?;
        if s.ttfb.is_none() {
            s.ttfb = Some(t0.elapsed());
        }
        match event {
            StreamEvent::OutputTextDelta { delta } if !delta.is_empty() => {
                if s.ttft.is_none() {
                    s.ttft = Some(t0.elapsed());
                }
                s.chars += delta.chars().count();
            }
            StreamEvent::ReasoningSummaryTextDelta { delta }
                if !delta.is_empty() && s.ttfr.is_none() =>
            {
                s.ttfr = Some(t0.elapsed());
            }
            StreamEvent::Error { code, message } => {
                eprintln!("  responses stream error: {message} (code {code:?})");
            }
            _ => {}
        }
    }
    s.total = t0.elapsed();
    Ok(s)
}

/// Open a realtime session and configure it text-only with the instructions.
/// Reasoning is left off (min latency / `gpt-realtime` is non-reasoning).
async fn realtime_connect(
    client: &Client,
    model: &ModelId,
    system: &str,
) -> Result<Realtime, openai::Error> {
    let mut conn = client.connect_realtime(model).await?;
    let mut session = SessionConfig::text(system);
    session.max_output_tokens = Some(MAX_TOKENS);
    session.reasoning = realtime_effort().map(|effort| RealtimeReasoning {
        effort: Some(effort),
    });
    conn.send(&ClientEvent::SessionUpdate { session }).await?;
    Ok(conn)
}

/// Run one turn on an already-open realtime session: send the user line, request
/// a response, and time the streamed text. The persistent connection is reused
/// across turns, so this excludes the WebSocket handshake.
async fn bench_realtime_turn(conn: &mut Realtime, user_line: &str) -> Result<Sample, openai::Error> {
    let t0 = Instant::now();
    // One flush: item.create + response.create reach the server together.
    conn.send_all(&[
        ClientEvent::ConversationItemCreate {
            item: Item::user_text(user_line),
        },
        ClientEvent::ResponseCreate { response: None },
    ])
    .await?;

    let mut s = Sample::default();
    while let Some(event) = conn.next_event().await {
        let event = event?;
        if s.ttfb.is_none() {
            s.ttfb = Some(t0.elapsed());
        }
        match event {
            ServerEvent::OutputTextDelta { delta, .. } if !delta.is_empty() => {
                if s.ttft.is_none() {
                    s.ttft = Some(t0.elapsed());
                }
                s.chars += delta.chars().count();
            }
            ServerEvent::ResponseDone { .. } => {
                s.total = t0.elapsed();
                return Ok(s);
            }
            ServerEvent::Error { error } => return Err(openai::Error::Realtime(error.message)),
            _ => {}
        }
    }
    s.total = t0.elapsed();
    Ok(s)
}

/// Render a duration as whole milliseconds. Uniform units keep columns aligned
/// and comparable; the default `{:.0?}` collapses anything >=1s to "1s".
fn ms(d: Duration) -> String {
    format!("{:.0}ms", d.as_secs_f64() * 1000.0)
}

fn fmt_opt(d: Option<Duration>) -> String {
    match d {
        Some(d) => ms(d),
        None => "-".to_string(),
    }
}

fn print_iter(name: &str, i: usize, s: &Sample) {
    eprintln!(
        "{name:>9} iter {i}: ttfb={} ttft={} ttfr={} total={} ({} chars)",
        fmt_opt(s.ttfb),
        fmt_opt(s.ttft),
        fmt_opt(s.ttfr),
        ms(s.total),
        s.chars,
    );
}

/// (min, mean, max) over a non-empty slice of durations.
fn stats(durs: &[Duration]) -> Option<(Duration, Duration, Duration)> {
    if durs.is_empty() {
        return None;
    }
    let min = *durs.iter().min().unwrap();
    let max = *durs.iter().max().unwrap();
    let sum: Duration = durs.iter().sum();
    Some((min, sum / durs.len() as u32, max))
}

fn mean_ttft(samples: &[Sample]) -> Option<Duration> {
    let ttfts: Vec<Duration> = samples.iter().filter_map(|s| s.ttft).collect();
    stats(&ttfts).map(|(_, mean, _)| mean)
}

fn print_aggregate(name: &str, samples: &[Sample]) {
    let ttfts: Vec<Duration> = samples.iter().filter_map(|s| s.ttft).collect();
    let totals: Vec<Duration> = samples.iter().map(|s| s.total).collect();
    eprintln!("--- {name} (n={}) ---", samples.len());
    if let Some((mn, me, mx)) = stats(&ttfts) {
        eprintln!("  TTFT  min={} mean={} max={}", ms(mn), ms(me), ms(mx));
    }
    if let Some((mn, me, mx)) = stats(&totals) {
        eprintln!("  total min={} mean={} max={}", ms(mn), ms(me), ms(mx));
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let mut args = std::env::args().skip(1);
    let n: usize = args.next().as_deref().unwrap_or("5").parse().unwrap();
    let which = args.next().unwrap_or_else(|| "all".to_string());
    let (run_chat, run_responses, mut run_realtime) = match which.as_str() {
        "chat" => (true, false, false),
        "responses" => (false, true, false),
        "realtime" => (false, false, true),
        "both" => (true, true, false),
        "all" => (true, true, true),
        other => {
            eprintln!("unknown backend {other:?}; use chat|responses|realtime|both|all");
            return;
        }
    };

    let cfg = match load_config() {
        Some(cfg) => cfg,
        None => {
            eprintln!(
                "SKIP bench_latency: niinii.toml missing openai_api_endpoint / openai_model"
            );
            return;
        }
    };
    let model = ModelId(cfg.model);
    // Reasoning models can be slow to first byte; give them headroom over the
    // 10s default.
    let client = Client::new(
        cfg.api_key,
        cfg.endpoint,
        ConnectionPolicy {
            timeout: Duration::from_secs(60),
            connect_timeout: Duration::from_secs(5),
        },
    );

    let system = "You are a translator. Translate the Japanese line into natural \
                  English. Output only the translation, nothing else.";

    // Distinct VN-style lines per iter to reduce server-side prompt caching
    // skewing the comparison (cf. bench_parse).
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

    let rt_model = realtime_model();
    if run_realtime {
        let effort = realtime_effort()
            .map(|e| <&'static str>::from(e).to_string())
            .unwrap_or_else(|| "default".to_string());
        eprintln!(
            "realtime backend uses model {} (reasoning={}); chat/responses use {}",
            rt_model.as_ref(),
            effort,
            model.as_ref(),
        );
    }

    let mut chat_samples: Vec<Sample> = Vec::new();
    let mut resp_samples: Vec<Sample> = Vec::new();
    let mut rt_samples: Vec<Sample> = Vec::new();
    // Persistent realtime session, opened lazily on the first realtime turn.
    let mut rt_conn: Option<Realtime> = None;
    for i in 0..n {
        let line = corpus[i % corpus.len()];
        if run_chat {
            match bench_chat(&client, model.clone(), system, line).await {
                Ok(s) => {
                    print_iter("chat", i, &s);
                    chat_samples.push(s);
                }
                Err(e) => eprintln!("chat iter {i}: error: {e}"),
            }
        }
        if run_responses {
            match bench_responses(&client, model.clone(), system, line).await {
                Ok(s) => {
                    print_iter("responses", i, &s);
                    resp_samples.push(s);
                }
                Err(e) => eprintln!("responses iter {i}: error: {e}"),
            }
        }
        if run_realtime {
            if rt_conn.is_none() {
                let tc = Instant::now();
                match realtime_connect(&client, &rt_model, system).await {
                    Ok(c) => {
                        eprintln!(" realtime: connected in {}", ms(tc.elapsed()));
                        rt_conn = Some(c);
                    }
                    Err(e) => {
                        eprintln!("realtime connect error: {e}");
                        run_realtime = false;
                    }
                }
            }
            if let Some(conn) = rt_conn.as_mut() {
                match bench_realtime_turn(conn, line).await {
                    Ok(s) => {
                        print_iter("realtime", i, &s);
                        rt_samples.push(s);
                    }
                    Err(e) => {
                        eprintln!("realtime iter {i}: error: {e}");
                        // Drop the session so the next turn reconnects.
                        rt_conn = None;
                    }
                }
            }
        }
    }
    if let Some(conn) = rt_conn {
        let _ = conn.close().await;
    }

    eprintln!();
    let runs: [(&str, &Vec<Sample>); 3] = [
        ("chat", &chat_samples),
        ("responses", &resp_samples),
        ("realtime", &rt_samples),
    ];
    for (name, samples) in runs {
        if !samples.is_empty() {
            print_aggregate(name, samples);
        }
    }
    // Mean-TTFT comparison across whichever backends produced samples.
    let mut means: Vec<(&str, Duration)> = runs
        .iter()
        .filter_map(|(name, s)| mean_ttft(s).map(|m| (*name, m)))
        .collect();
    if means.len() >= 2 {
        means.sort_by_key(|(_, m)| *m);
        let parts: Vec<String> = means.iter().map(|(n, m)| format!("{n}={}", ms(*m))).collect();
        eprintln!("\nTTFT mean: {}  (fastest: {})", parts.join(" "), means[0].0);
    }
}
