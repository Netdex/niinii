# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

niinii is a Windows desktop application for glossing and translating Japanese text, primarily used for assisted reading of visual novels. It provides real-time text segmentation (via Ichiran), dictionary lookups (JMDict/KANJIDIC2), furigana display, and LLM-based translation. The UI is built with imgui-rs.

## Build Commands

```bash
# Build (requires vcpkg with freetype installed)
# vcpkg install freetype:x64-windows-static-md
cargo build --release

# Run
cargo run --release -p niinii

# Run tests
cargo test --workspace

# Run a single crate's tests
cargo test -p openai
cargo test -p ichiran
cargo test -p vndb

# Optional features
cargo build --features tracing-tracy    # Tracy profiler support
cargo build --features tracing-chrome   # Chrome tracing support
cargo build --features voicevox         # Text-to-speech (Windows only)
```

## Architecture

### Workspace Crates

- **`niinii/`** — Main application crate (binary). Contains the GUI, application logic, and glue between subsystems.
- **`openai/`** — Custom OpenAI API client library (Chat Completions, Realtime WebSocket, Responses API). Not published; built specifically for this project.
- **`ichiran/`** — Rust wrapper around `ichiran-cli`, a Common Lisp program for Japanese text segmentation. Manages a PostgreSQL subprocess and communicates via CLI invocations with S-expressions. Includes LRU caching for segments and kanji lookups.
- **`vndb/`** — Minimal client for the VNDB Kana API (`https://api.vndb.org/kana`). Implements only the endpoints niinii needs (VN search, VN-by-id, characters-by-vn) and the search filters it surfaces in the UI.
- **`third-party/`** — Vendored/forked dependencies: `imgui-dx11-renderer`, `vvcore` (VOICEVOX), `eventsource-stream`.

### Key Subsystems in `niinii/`

- **`app.rs`** — Central `App` struct. Owns the parser, translator, TTS engine, and coordinates async message passing (gloss results, translations) via tokio mpsc channels.
- **`renderer/`** — Rendering backends implementing the `Renderer` trait: `glow_viewports` (OpenGL, cross-platform) and `d3d11` (Direct3D 11, Windows-only). Manages imgui context, font loading, and the main event loop.
- **`translator/`** — Two backends behind the shared `Backend` trait (`translator/mod.rs`): `chat` (OpenAI Chat Completions) and `responses` (OpenAI Responses API). Each is a command/event/state store: the UI sends commands, a single writer task applies them and reduces events emitted by adapter tasks, and publishes immutable state snapshots via `ArcSwap`. UI reads are wait-free (`state.load_full()`) and never `async`. Per-request knobs are snapshotted into the shared `TranslateConfig` at submission time (selected by `Settings::translator_type`); backends never read `Settings` live. `system_addendum` is a separate text fragment appended to the system message / `instructions` at prompt-build time -- external producers (e.g. VNDB) push it via `Backend::set_system_addendum` so the translator owns the prompt without per-call plumbing. The `chat` backend keeps a local editable context buffer with token trimming; the `responses` backend keeps conversation state server-side, chaining turns via `previous_response_id` (`store: true`) and resending `instructions` each turn. Shared render types (`ExchangeId`, `Response`, `ExchangeView`, `UsageView`) live in `translator/mod.rs`. Latency is a first-class concern for the `responses` backend (streaming, `service_tier`, `reasoning.effort`, opt-in reasoning summary, a stable per-process `prompt_cache_key`).
- **`vndb.rs`** — Same command/event/state shape as the translator backends. `vndb::spawn(callback)` takes a `Fn(Option<Arc<str>>)` invoked from the writer task whenever the active VN's pre-rendered prompt fragment changes; this is wired in `VndbView::new` to call `Backend::set_system_addendum` on the active translator (type-erased `Arc<dyn Backend>`). Persists the active VN id in `Settings::vndb_active_id` and restores it on startup.
- **`view/`** — imgui UI components. Each top-level window (translator, settings, style editor, vndb) is a persistent struct that owns its own `open: bool` and any edit-buffer state. Convention: `show_menu_item(ui)` to render the menu entry that opens it, and `ui(...)` self-renders the window with `.opened(&mut self.open)` and early-returns when closed. `App` holds one instance of each and calls `ui(...)` unconditionally each frame. `VndbView` additionally owns the `VndbHandle` and the wiring from VNDB -> translator, so `App` doesn't see either.
- **`settings.rs`** — Application configuration. Serialized to/from `niinii.toml` using serde.
- **`parser.rs`** — Wraps the `ichiran` crate to produce a `SyntaxTree` from Japanese input text.

### Configuration

The app reads `niinii.toml` at startup. This contains API keys, model settings, translation prompts, renderer choice, and UI preferences. Settings are written back on exit.

### Dependencies and Patches

The workspace uses a forked `imgui-rs` (branch `glow-viewports-mdpi`) patched for viewport and DPI support. The fork is referenced via `[patch.crates-io]` in the root `Cargo.toml`. The `freetype` library is required via vcpkg for font rendering.

### Runtime Dependencies

Japanese language support requires `ichiran-cli` and a PostgreSQL instance with the Ichiran database. Paths are configured in `niinii.toml`. The `data/` directory contains these runtime dependencies for packaged builds.

## Code Style

- Only add a comment when it explains a requirement or constraint that is not self-evident from the code (e.g. an OS quirk, an API contract, a non-obvious invariant). Don't restate what the code does.
- Comments must stand alone against the current code. Don't describe history, previous implementations, alternatives that were tried, or why something changed -- that belongs in commit messages.
- Keep comments concise: a line or two, not paragraphs.

## Target Platform

Primary target is `x86_64-pc-windows-msvc`. Cross-platform support is possible via the Glow renderer but is not actively maintained.