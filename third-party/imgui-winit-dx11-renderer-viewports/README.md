# imgui-winit-dx11-renderer-viewports

Combined [winit](https://github.com/rust-windowing/winit) platform backend and
Direct3D 11 renderer for [imgui-rs](https://github.com/imgui-rs/imgui-rs), with
support for Dear ImGui's multi-viewport feature (imgui windows dragged outside
the main window become real OS windows).

It is the D3D11 counterpart of `imgui-winit-glow-renderer-viewports`, including
multi-DPI support: each viewport converts between logical and physical
coordinates with its own monitor's scale factor.

Each monitor maps physical pixels to imgui's coordinates with its own scale,
with its logical origin placed against its neighbours so monitors that touch on
the desktop also touch for imgui. Every viewport uses one monitor's mapping,
which the crate picks itself and reports to imgui through
`Platform_GetWindowDpiScale`: the monitor under the cursor while imgui drags
the window, otherwise the monitor under its top-left corner. The cursor is
converted with the mapping of the window it is over, not the window that
reported it. Following
Windows' `WM_DPICHANGED` instead (which picks by window area) feeds back through
the resize and makes windows oscillate at monitor boundaries. For undecorated
viewports imgui owns the size: resizes Windows applies on a DPI change are
undone rather than fed back into imgui, which would otherwise make windows
drift in size every time they cross monitors.

Windows only. Targets `imgui` 0.12 (with the `docking` feature) and `winit` 0.29.

## Usage

The application creates the D3D11 device and owns the main window's swapchain;
the renderer owns every secondary viewport window and its swapchain.

```rust
let mut renderer = Renderer::new(&mut imgui, &window, device.clone(), None)?;

// for every winit event:
renderer.handle_event(&mut imgui, &window, &event);

// on redraw:
let ui = imgui.frame();
// ... build UI ...
ui.end_frame_early();
renderer.prepare_render(&mut imgui, &window);
imgui.update_platform_windows();
renderer.update_viewports(&mut imgui, window_target)?;
let draw_data = imgui.render();
// bind + clear the main render target, then:
renderer.render(draw_data)?;
// present the main swapchain, then:
renderer.render_viewports(&mut imgui)?;
```

See `examples/viewports_basic.rs` for a complete program.

Notes:

- Rendering does not save or restore D3D11 pipeline state, and
  `render_viewports` leaves a viewport's render target bound. Rebind the main
  render target at the start of every frame.
- Viewport swapchains use the blt model (`DXGI_SWAP_EFFECT_DISCARD`) because
  flip-model swapchains ignore the per-pixel alpha that transparent windows
  rely on. They present without vsync; pace frames with the main swapchain.
- User textures go in `textures_mut()`; call `rebuild_font_texture` after
  changing fonts.
- Viewports imgui marks `NO_INPUTS` are made click-through, and viewports it
  marks `TOP_MOST` (tooltips) are always topmost.
- `refresh_topmost` re-raises topmost viewport windows; call it periodically
  to stay above windows that make themselves topmost later.
- `viewport_window_contains` tells whether a desktop point is over a secondary
  viewport window, e.g. to make a click-through main window pass input on.

## Credits

The draw-data renderer is adapted from
[imgui-dx11-renderer](https://github.com/Veykril/imgui-dx11-renderer) (MIT,
Lukas Wirth). The platform integration is ported from
`imgui-winit-glow-renderer-viewports` (MIT OR Apache-2.0, imgui-rs
contributors).
