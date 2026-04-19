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

# Optional features
cargo build --features tracing-tracy    # Tracy profiler support
cargo build --features tracing-chrome   # Chrome tracing support
cargo build --features voicevox         # Text-to-speech (Windows only)
cargo build --features hook             # DLL injection/hooking (Windows only)

# 32-bit build (for hooking into 32-bit applications)
cargo +stable-i686-pc-windows-msvc build --target i686-pc-windows-msvc --release
```

## Architecture

### Workspace Crates

- **`niinii/`** — Main application crate (binary + cdylib). Contains the GUI, application logic, and glue between subsystems.
- **`openai/`** — Custom OpenAI API client library (Chat Completions, Realtime WebSocket, Responses API). Not published; built specifically for this project.
- **`ichiran/`** — Rust wrapper around `ichiran-cli`, a Common Lisp program for Japanese text segmentation. Manages a PostgreSQL subprocess and communicates via CLI invocations with S-expressions. Includes LRU caching for segments and kanji lookups.
- **`third-party/`** — Vendored/forked dependencies: `imgui-dx11-renderer`, `vvcore` (VOICEVOX), `eventsource-stream`.

### Key Subsystems in `niinii/`

- **`app.rs`** — Central `App` struct. Owns the parser, translator, TTS engine, and coordinates async message passing (gloss results, translations) via tokio mpsc channels.
- **`renderer/`** — Rendering backends implementing the `Renderer` trait: `glow_viewports` (OpenGL, cross-platform) and `d3d11` (Direct3D 11, Windows-only). Manages imgui context, font loading, and the main event loop.
- **`translator/`** — Currently one backend: `chat` (OpenAI Chat Completions). Organized as a command/event/state store: the UI sends `ChatCommand`s, a single writer task applies commands and reduces `ChatEvent`s emitted by adapter tasks, and publishes immutable `ChatState` snapshots via `ArcSwap`. UI reads are wait-free (`state.load_full()`) and never `async`. Per-request knobs are snapshotted into `TranslateConfig` at submission time; the backend never reads `Settings` live.
- **`view/`** — imgui UI components. Each top-level window (translator, settings, inject, style editor) is a persistent struct that owns its own `open: bool` and any edit-buffer state. Convention: `show_menu_item(ui)` to render the menu entry that opens it, and `ui(...)` self-renders the window with `.opened(&mut self.open)` and early-returns when closed. `App` holds one instance of each and calls `ui(...)` unconditionally each frame.
- **`settings.rs`** — Application configuration. Serialized to/from `niinii.toml` using serde.
- **`parser.rs`** — Wraps the `ichiran` crate to produce a `SyntaxTree` from Japanese input text.
- **`hook.rs`** — DLL injection support via `hudhook` for rendering the overlay inside another process (feature-gated).

### Configuration

The app reads `niinii.toml` at startup. This contains API keys, model settings, translation prompts, renderer choice, and UI preferences. Settings are written back on exit.

### Dependencies and Patches

The workspace uses a forked `imgui-rs` (branch `glow-viewports-mdpi`) patched for viewport and DPI support. The fork is referenced via `[patch.crates-io]` in the root `Cargo.toml`. The `freetype` library is required via vcpkg for font rendering.

### Runtime Dependencies

Japanese language support requires `ichiran-cli` and a PostgreSQL instance with the Ichiran database. Paths are configured in `niinii.toml`. The `data/` directory contains these runtime dependencies for packaged builds.

## Target Platform

Primary target is `x86_64-pc-windows-msvc`. Cross-platform support is possible via the Glow renderer but is not actively maintained.