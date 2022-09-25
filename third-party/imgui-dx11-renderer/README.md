# imgui-dx11-renderer

[![Documentation](https://docs.rs/imgui-dx11-renderer/badge.svg)](https://docs.rs/imgui-dx11-renderer)
[![Version](https://img.shields.io/crates/v/imgui-dx11-renderer.svg)](https://crates.io/crates/imgui-dx11-renderer)

DirectX 11 renderer for [imgui-rs](https://github.com/Gekkio/imgui-rs).

## Usage

This crate makes use of the ComPtr wrapper of the [wio](https://crates.io/crates/wio) crate.
You have to wrap your device pointer in one to pass it to the renderer should you not use these internally already,
tho care must be taken in regards of the reference count.

```rust
let device: ComPtr<ID3D11Device> = ...;
let imgui: imgui::Context = ...;
let mut renderer = imgui_dx11_renderer::Renderer::new(&mut imgui, device.clone()).expect("imgui dx11 renderer creation failed");

// rendering loop

let ui = imgui.frame();

// build your window via ui here
...

// then to render call
renderer.render(ui.render()).expect("imgui rendering failed");
```

The renderer backs up and reapplies the majority of the d3d11 renderering state when invoked.

## Documentation

The crate is documented but imgui-rs doesn't currently build on docs.rs
for the windows target. Due to this one has to either build it
themselves or look into the source itself.

## License

Licensed under the MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)
