# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

niinii is a Windows desktop application for glossing and translating Japanese text, primarily used for assisted reading of visual novels. It provides real-time text segmentation (via Ichiran), dictionary lookups (JMDict/KANJIDIC2), furigana display, and LLM-based translation. The UI is built with Dear ImGui via dear-imgui-rs, rendered with wgpu (DX12).

## Build Commands

```bash
# Build (requires vcpkg with freetype installed)
# vcpkg install freetype:x64-windows-static-md
cargo build --release

# Run
cargo run --release -p niinii

# Run tests
cargo test --workspace

# UI behavior tests (headless, Dear ImGui Test Engine) run with the niinii tests
cargo test -p niinii

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
- **`third-party/`** — Vendored/forked dependencies: `vvcore` (VOICEVOX), `eventsource-stream`, and `dear-imgui-reflect` / `dear-imgui-reflect-derive` 0.23.0 (via `[patch.crates-io]`) with local additions: struct attributes `#[imgui(columns = N, open)]` (fields in an N-column table, tree node open at first); field attributes `help = "..."` (help marker), `enabled_if = expr` (drawn disabled unless the condition on `self` holds) and `enable_with = expr` (value an `Option<T>` takes when enabled); `Option<T>` fields get `T`'s editor with `T`'s attributes via `imgui_option_with`; and `OptionSettings::inline` (checkbox and editor on one line, editor previewing the enable value while off).
- **`tools/prompt-tune/`** — Python (uv) tool that tunes the chat system prompt with GEPA on the VNTL dataset against a local model, with Claude Code headless as the reflection model. See its README.

### Key Subsystems in `niinii/`

- **`app.rs`** — Central `App` struct. Owns the history, translator, TTS engine and the optional windows' `Panel`s, and orchestrates them: history events (clipboard text, parse failures) trigger glossing, translation and TTS in one place.
- **`gui/`** — Dear ImGui on winit + wgpu (DX12 with a DirectComposition swapchain, so windows can be transparent). Every ImGui window is its own native window (`ConfigViewportsNoAutoMerge`); the winit host window only anchors the main viewport and stays hidden. `gui::run(options, |ui, frame| ...)` owns the event loop; the app is a per-frame closure. Native window policy that ImGui can't express is declared per frame on `Frame` (`pass_input_through` for click-through to other applications, `keep_on_top`, `exit`) and reconciled onto the native windows after each frame (`gui/native.rs`). ImGui's own `NoInputs` viewport flag only passes input to windows of the same thread, so it is not used for click-through. There is one font (Sarasa Mono J, FreeType); ImGui 1.92+ rasterizes glyphs on demand at any size, so text styles are just sizes (`FontSize::BODY`, `FontSize::KANJI`). ImGui works in physical pixels (per-monitor DPI), so fixed lengths in views are written for 100% scale with body text and converted with `gui::px`/`gui::px2`; resizable windows set a minimum size and no maximum. The saved `Settings::text_scale` is declared on `Frame` and applied as `style.FontScaleMain`. Only the main niinii window has a taskbar entry (`ConfigViewportsNoTaskBarIcon`, cleared by its window class).
- **`translator/`** — Three backends behind the shared `Backend` trait (`translator/mod.rs`): `chat` (OpenAI Chat Completions), `responses` (OpenAI Responses API) and `realtime` (OpenAI Realtime API over a persistent WebSocket session). Each is a command/event/state store: the UI sends commands, a single writer task applies them and reduces events emitted by adapter tasks, and publishes immutable state snapshots via `ArcSwap`. UI reads are wait-free (`state.load_full()`) and never `async`. Per-request knobs are snapshotted into the shared `TranslateConfig` at submission time (selected by `Settings::translator_type`); backends never read `Settings` live. `system_addendum` is a separate text fragment appended to the system message / `instructions` at prompt-build time -- external producers (e.g. VNDB) push it via `Backend::set_system_addendum` so the translator owns the prompt without per-call plumbing. The `chat` backend keeps a local editable context buffer with token trimming, and sends it as the user/assistant turns that produced it, with the same `LINE_PREFIX` on every unpinned user line (history and live) so a line's message is byte-identical when it later becomes history (stable request prefix for the prompt cache); the `responses` backend keeps conversation state server-side, chaining turns via `previous_response_id` (`store: true`) and resending `instructions` each turn; the `realtime` backend keeps the conversation in the server-side session (with `chain` off it reconnects per line so context never accumulates) with a configurable truncation strategy. Shared render types (`ExchangeId`, `Response`, `ExchangeView`, `UsageView`) live in `translator/mod.rs`. An exchange's own problems (failure, cancellation, a reply cut short) are on its `Response` (`Response::problem`); `Backend::last_error` only holds errors not tied to an exchange. Latency is a first-class concern for the `responses` backend (streaming, `service_tier`, `reasoning.effort`, opt-in reasoning summary, a stable per-process `prompt_cache_key`).
- **`vndb.rs`** — Same command/event/state shape as the translator backends. `vndb::spawn(callback)` takes a `Fn(Option<Arc<str>>)` invoked from the writer task whenever the active VN's pre-rendered prompt fragment changes; this is wired in `VndbView::new` to call `Backend::set_system_addendum` on the active translator (type-erased `Arc<dyn Backend>`). Persists the active VN id in `Settings::vndb_active_id` and restores it on startup.
- **`view/`** — imgui UI components. An optional window is a `view::Panel` (title + open flag) owned by `App`; `panel.menu_item(ui)` opens it and the window's module draws it with `panel.show(ui, configure, contents)`. Window modules with no state of their own are plain functions (`settings::show`, `style_editor::show`, `help::show`); stateful ones are structs with a `show(&mut self, panel, ui, ...)` method (`TranscriptView`, `VndbView`, `TranslatorWindow`). `VndbView` owns the `VndbHandle` and the wiring from VNDB -> translator, so `App` doesn't see either. `GlossView` renders the line being shown from `History` and owns only view state (input box, open word windows). Keyboard shortcuts are `view::shortcuts::Action`s: each has its key and description, `App::run` executes it, `Action::pressed` checks keys through ImGui shortcut routing, and menu items and the Help window take their key labels from the same table. Problems are shown with `mixins::error_note`, after the content they are about. The translator windows share `translator/history.rs` (the server-side conversation as Role/Message rows) and `translator/tuning.rs` (the settings grid).
- **`settings.rs`** — Application configuration, serialized to/from `niinii.toml`. `Settings` is grouped into the sections the Settings window shows (`IchiranSettings`, `InterfaceSettings`, ...), `#[serde(flatten)]`ed so the file stays a flat list of keys; plain values must come before the tables (`chat`, `responses`, `realtime`, `theme`) because TOML cannot hold values after a table. The sections and the translator backend structs derive `ImGuiReflect` (`dear-imgui-reflect`), so their editors are generated: field labels come from `#[imgui(name = ...)]`, a trailing `*` marks restart-to-apply. Types with their own editor (`FilePath`/`FolderPath` with a native Browse dialog, `Secret`, `ContextLimit`) implement `ImGuiValue` in `view/settings.rs`. The `openai` crate's `reflect` feature derives editors for its request enums.
- **`parser.rs`** — Wraps the `ichiran` crate to produce a `SyntaxTree` from Japanese input text.
- **`history.rs`** — The lines the user has seen (`History`): each line's text, gloss (preview, then the parsed `SyntaxTree`), selected alternatives and translation; the cursor for stepping through them; the in-flight parse; and clipboard watching. `poll` returns `HistoryEvent`s for `App` to act on.

### Configuration

The app reads `niinii.toml` at startup. This contains API keys, model settings, translation prompts, and UI preferences (including the saved style as a dear-imgui-rs `Theme`, with sizes unscaled). Settings are written back on exit.

### Dependencies and Patches

The dear-imgui crates (`dear-imgui-rs`, `dear-imgui-winit`, `dear-imgui-wgpu`) are released together and must be updated together. The `freetype` library is required via vcpkg for font rendering.

### Runtime Dependencies

Japanese language support requires `ichiran-cli` and a PostgreSQL instance with the Ichiran database. Paths are configured in `niinii.toml`. The `data/` directory contains these runtime dependencies for packaged builds.

## Code Style

- Only add a comment when it explains a requirement or constraint that is not self-evident from the code (e.g. an OS quirk, an API contract, a non-obvious invariant). Don't restate what the code does.
- Comments must stand alone against the current code. Don't describe history, previous implementations, alternatives that were tried, or why something changed -- that belongs in commit messages.
- Keep comments concise: a line or two, not paragraphs.

## Target Platform

Primary target is `x86_64-pc-windows-msvc`. The `gui` module selects the DX12 backend and relies on Win32 behavior (clipboard sequence numbers, scroll lock); other platforms are not supported.