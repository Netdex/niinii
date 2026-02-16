# AGENTS.md

This document provides guidance for AI agents and contributors working on this repository.

## Project Overview

- Language: Rust (edition 2021)
- Workspace with multiple crates: `niinii` (main app), `ichiran` (dictionary integration), `openai` (API client)
- Primary target: Windows (DX11, Win32 hooks), with best-effort cross-platform support via winit/glutin
- UI: Dear ImGui (`imgui-rs`) with docking + multi-viewport
- Async runtime: `tokio`

This is a desktop GUI application focused on Japanese language tooling (JMdict/KANJIDIC, Ichiran integration, optional VoiceVox TTS, OpenAI integration).

## Core Principles

- Keep modules small and cohesive.
- Prefer explicit types over overly generic abstractions.
- Avoid premature optimization; measure before optimizing.
- Favor readability and maintainability over cleverness.
- Keep public APIs minimal and well-documented.

## Workspace Structure

- `niinii/` – main GUI application and platform integration
- `ichiran/` – wrapper around Ichiran CLI / data processing
- `openai/` – OpenAI API client and streaming logic
- `third-party/` – vendored dependencies (excluded from workspace)

When adding new functionality:

- Put UI-only code in `niinii`.
- Put reusable, non-UI logic in a dedicated module or crate.
- Avoid leaking GUI types (e.g. `imgui::Ui`) into core/domain logic.

Within `niinii`:

- Keep rendering code separate from state management.
- Isolate platform-specific code behind `cfg(windows)` when necessary.
- Keep `build.rs` limited to resource embedding and build-time configuration.

Example layout:

```
src/
  main.rs
  lib.rs
  core/
  parser/
tests/
```

## Error Handling

- Use `thiserror` for crate-level error enums (already used in workspace).
- Return `Result<T, E>` from library-style modules (`ichiran`, `openai`).
- Convert to user-visible errors at UI boundaries.
- Avoid `unwrap()` and `expect()` outside tests and clearly justified invariants.
- For async tasks, propagate errors via structured types rather than logging-only failures.

## Logging and Diagnostics

- Use `tracing` for structured logs.
- Respect feature flags: `tracing-tracy`, `tracing-chrome`.
- Do not use `println!` for diagnostics in application code.
- Ensure long-running async tasks emit useful spans.

If adding new subsystems, instrument them with `tracing::instrument` where helpful.

## Async and Concurrency

- Runtime: `tokio` (multi-threaded).
- Prefer async/await over manual thread spawning.
- Use channels (`tokio::sync`) for cross-task communication.
- Avoid blocking calls on async threads; use `spawn_blocking` if necessary.
- Be careful not to block the render loop.

GUI responsiveness is critical: never perform heavy I/O or parsing directly inside frame rendering.

## UI Guidelines (ImGui)

- Keep layout code deterministic and frame-local.
- Store persistent UI state in structs, not global statics.
- Avoid large allocations during each frame.
- When adding new windows/panels, keep them modular and self-contained.
- Respect docking + multi-viewport behavior.

Do not mix networking, file I/O, or heavy parsing directly into UI drawing functions.

## VoiceVox and Native Integrations

- All FFI or native bindings must be isolated and documented.
- Document invariants and lifetime assumptions when wrapping C APIs.
- Guard optional integrations behind feature flags (e.g. `voicevox`, `hook`).
- Keep Windows-specific hooks clearly separated with `cfg(windows)`.

## Formatting and Linting

- Run `cargo fmt` before committing.
- Run `cargo clippy --all-targets --all-features -D warnings` and fix warnings.
- Keep CI (if added) strict on formatting and lints.

## Data and Dictionaries

- Large datasets (JMdict, KANJIDIC, etc.) must not be repeatedly parsed.
- Prefer lazy initialization (`once_cell`, `lazy_static`) where appropriate.
- Avoid copying large strings unnecessarily.
- Use streaming or incremental parsing for large external data.

## Performance

- Avoid per-frame heap allocations in hot UI paths.
- Minimize cloning of large language model responses or dictionary entries.
- Use iterators and slices where possible.
- Measure before optimizing; avoid premature micro-optimizations.

## Documentation

- Document all public APIs in `ichiran` and `openai`.
- For complex subsystems (streaming, hooks, TTS), add module-level comments explaining architecture.
- Keep README user-focused; put architectural notes in dedicated docs.

## Build and Platform Notes

- Windows requires vcpkg for FreeType (via `imgui-sys`).
- DX11 renderer is used on Windows; avoid breaking that path.
- Keep `build.rs` changes minimal and well-justified.

Always verify:

- `cargo fmt`
- `cargo clippy --all-targets --all-features -D warnings`
- `cargo test`

## When Modifying Code

- Do not refactor unrelated subsystems in the same change.
- Preserve existing UI behavior unless explicitly redesigning.
- If changing public APIs in `ichiran` or `openai`, update call sites together.
- Keep feature-flag behavior intact.
- Prefer incremental improvements over large rewrites.

## Safety and Security

- Avoid unsafe code unless absolutely necessary.
- If `unsafe` is used, document invariants and justify its necessity.
- Validate all external input.

---

If any of these guidelines conflict with explicit user instructions, follow the user instructions and update this file if the new pattern becomes standard for the project.
