# third-party

Dependencies kept in the repository, either because niinii changes them or
because they ship files that crates.io doesn't carry. The crates.io crates are
substituted through `[patch.crates-io]` in the workspace `Cargo.toml`.

| Directory | Version | Why it is here |
|---|---|---|
| `dear-imgui-reflect`, `dear-imgui-reflect-derive` | 0.23.0 | Editor features the settings windows need |
| `dear-imgui-winit` | 0.23.0 | Fix for collapsed windows forgetting their size |
| `wgpu-hal` | 30.0.2 | Transparent windows on OpenGL |
| `eventsource-stream` | 0.2.3 | Server-sent events for streaming OpenAI responses |
| `vvcore` | 0.0.2 | VOICEVOX CORE bindings with the native libraries they link |
| `vcpkg` | submodule | FreeType for builds and CI |

## Local changes

### dear-imgui-reflect, dear-imgui-reflect-derive

Additions to the derive and the `Option<T>` editor:

- Struct attributes `#[imgui(grid, columns = N, open)]`: `grid` lays fields
  out as table rows with the label in its own column and the widget filling
  the next; `columns` puts N fields on a row; `open` starts the tree node
  expanded.
- Field attributes `help = "..."` (help marker), `enabled_if = expr`
  (disabled unless the condition on `self` holds) and `enable_with = expr`
  (the value an `Option<T>` takes when ticked).
- `Option<T>` fields get `T`'s editor with `T`'s attributes, through
  `imgui_option_with`.
- `OptionSettings::inline`: the checkbox and the editor on one line, the
  editor previewing the enable value while off.

### dear-imgui-winit

A resize the backend requested itself is no longer reported back to Dear ImGui
as a platform resize (`callbacks/window.rs`, `events.rs`, marked "niinii
patch"). Reporting it made ImGui overwrite a collapsed window's full size with
the collapsed height. Dear ImGui's own Win32 backend ignores these echoes too.

### wgpu-hal

The WGL pixel format (`src/gles/wgl.rs`) requests an alpha channel and
`PFD_SUPPORT_COMPOSITION`, so DWM can blend OpenGL windows with what is behind
them. Upstream asks for 8 color bits and no alpha.

### eventsource-stream

Examples and tests removed; the `http` dev-dependency bumped to 1.

### vvcore

`build.rs` links the VOICEVOX CORE libraries in `lib/` by a path relative to
this repository, and lint warnings are allowed.

## Updating

- `dear-imgui-winit` must stay on the same version as `dear-imgui-rs` and
  `dear-imgui-wgpu`, which are released together. `dear-imgui-reflect` follows
  the same version.
- To update a patched crate, diff the vendored copy against the published
  version it came from (in `~/.cargo/registry/src`) to recover the local
  changes, copy in the new version, and re-apply them.
- Drop a vendored crate once upstream covers its change, along with its
  `[patch.crates-io]` entry.
