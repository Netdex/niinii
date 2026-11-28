# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

niinii is a Windows desktop application for glossing and translating Japanese text, primarily used for assisted reading of visual novels: segmentation via Ichiran, JMDict/KANJIDIC2 lookups, furigana and LLM translation. The UI is Dear ImGui (dear-imgui-rs) on winit and wgpu.

## Build Commands

```bash
# Requires FreeType from vcpkg: vcpkg install freetype:x64-windows-static-md
cargo build --release
cargo run --release -p niinii

cargo test --workspace
cargo test -p niinii        # includes the headless UI tests (niinii/tests/ui.rs)

cargo build --features tracing-tracy    # Tracy profiler support
cargo build --features tracing-chrome   # Chrome tracing support
cargo build --features voicevox         # Text-to-speech (Windows only)
```

Runtime: Japanese support needs `ichiran-cli` and a PostgreSQL instance with the Ichiran database, configured in `niinii.toml` (read at startup, written back on exit). `data/` holds these for packaged builds; `docs/ichiran.md` covers rebuilding and packing them.

## Constraints

- **Dependencies.** `dear-imgui-rs`, `dear-imgui-winit` and `dear-imgui-wgpu` are released together and must be updated together. Vendored crates in `third-party/` carry local changes; `third-party/README.md` lists why each is there, what changed and how to update it. Keep it current when vendoring or patching.
- **Renderer.** Transparent windows need DX12's DirectComposition swapchain or, on OpenGL, the alpha pixel format from the patched `wgpu-hal`. VMware guests have no DX12, hence the OpenGL fallback.
- **Native windows.** Every ImGui window is its own native window. ImGui's `NoInputs` viewport flag only passes input to windows of the same thread, so click-through uses native hit-testing instead.
- **Sizes.** ImGui works in physical pixels. Fixed lengths are written for 100% scale and converted with `gui::px`; window sizes are in text units (`TextSize`).
- **Settings file.** `Settings` sections are `#[serde(flatten)]`ed so `niinii.toml` stays a flat list of keys. Plain values must come before the tables (`chat`, `responses`, `realtime`, `theme`): TOML cannot hold values after a table.
- **Translator state.** Backends are command/event/state stores publishing snapshots through `ArcSwap`; UI reads never block or `await`. Per-request settings are snapshotted into `TranslateConfig` at submission; backends never read `Settings` live.
- **Prompt caching.** The `chat` backend puts the same `LINE_PREFIX` on every unpinned user line, history and live, so a line's message is byte-identical when it later becomes history. Keep it that way, or the request prefix stops being cacheable.
- **UI tests.** Dear ImGui Test Engine is process-global, so all scenarios run in one `#[test]`. Test contexts must not use `imgui.ini`, or window sizes leak between runs.

## Code Style

- Only add a comment when it explains a requirement or constraint that is not self-evident from the code (e.g. an OS quirk, an API contract, a non-obvious invariant). Don't restate what the code does.
- Comments must stand alone against the current code. Don't describe history, previous implementations, alternatives that were tried, or why something changed -- that belongs in commit messages.
- Keep comments concise: a line or two, not paragraphs.

## Target Platform

`x86_64-pc-windows-msvc` only. The GUI relies on Win32 behavior (DirectComposition, window hooks, clipboard sequence numbers, scroll lock).
