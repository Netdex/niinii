use imgui::*;
use imgui_winit_support::{HiDpiMode, WinitPlatform};
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;
use std::{fs, ptr};
use winapi::{
    shared::{dxgi::*, dxgiformat::*, dxgitype::*, minwindef::TRUE, windef::HWND, winerror::S_OK},
    um::{d3d11::*, d3dcommon::*},
    Interface as _,
};
use winit::event_loop::ControlFlow;
use winit::platform::run_return::EventLoopExtRunReturn;
use winit::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::EventLoop,
    window::WindowBuilder,
};
use wio::com::ComPtr;

mod clipboard;

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub enum TextStyle {
    Kanji,
    Body,
}

#[derive(Default)]
pub struct Env {
    fonts: HashMap<TextStyle, FontId>,
}
impl Env {
    pub fn get_font(&self, style: TextStyle) -> FontId {
        *self.fonts.get(&style).unwrap()
    }
}

#[derive(Deserialize, Serialize)]
#[serde(remote = "ImString")]
pub struct ImStringDef(#[serde(getter = "ImString::to_string")] String);
impl From<ImStringDef> for ImString {
    fn from(str: ImStringDef) -> Self {
        ImString::new(str.0)
    }
}

fn init_fonts(env: &mut Env, imgui: &mut Context, hidpi_factor: f64) {
    let mut add_font = |style: TextStyle, path: &str, size_pt: f64, config: &[FontConfig]| {
        let font_data = &fs::read(path).unwrap();
        let font_sources: Vec<_> = config
            .iter()
            .map(|config| FontSource::TtfData {
                data: font_data,
                size_pixels: (size_pt * hidpi_factor) as f32,
                config: Some(FontConfig {
                    name: Some(format!("{:?}", style)),
                    ..config.clone()
                }),
            })
            .collect();
        env.fonts
            .insert(style, imgui.fonts().add_font(font_sources.as_slice()));
    };
    let jp_font_config = [
        // japanese
        FontConfig {
            rasterizer_multiply: 1.75,
            glyph_ranges: FontGlyphRanges::japanese(),
            oversample_h: 2,
            ..Default::default()
        },
        // latin extended-a
        FontConfig {
            rasterizer_multiply: 1.75,
            glyph_ranges: FontGlyphRanges::from_slice(&[0x0100, 0x017F, 0x0]),
            oversample_h: 2,
            ..Default::default()
        },
    ];
    add_font(
        TextStyle::Body,
        "res/sarasa-mono-j-regular.ttf",
        16.0,
        &jp_font_config,
    );
    add_font(
        TextStyle::Kanji,
        "res/sarasa-mono-j-regular.ttf",
        40.0,
        &jp_font_config,
    );
    imgui.io_mut().font_global_scale = (1.0 / hidpi_factor) as f32;
}

unsafe fn create_device(
    hwnd: HWND,
) -> Option<(
    ComPtr<IDXGISwapChain>,
    ComPtr<ID3D11Device>,
    ComPtr<ID3D11DeviceContext>,
)> {
    let sc_desc = DXGI_SWAP_CHAIN_DESC {
        BufferDesc: DXGI_MODE_DESC {
            Width: 0,
            Height: 0,
            RefreshRate: DXGI_RATIONAL {
                Numerator: 60,
                Denominator: 1,
            },
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            ScanlineOrdering: 0,
            Scaling: 0,
        },
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
        BufferCount: 3,
        OutputWindow: hwnd,
        Windowed: TRUE,
        SwapEffect: DXGI_SWAP_EFFECT_DISCARD,
        Flags: DXGI_SWAP_CHAIN_FLAG_ALLOW_MODE_SWITCH,
    };

    let mut swapchain = ptr::null_mut();
    let mut device = ptr::null_mut();
    let mut context = ptr::null_mut();

    let mut feature_level = 0;
    let feature_levels = [D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_10_0];
    if D3D11CreateDeviceAndSwapChain(
        ptr::null_mut(),
        D3D_DRIVER_TYPE_HARDWARE,
        ptr::null_mut(),
        0,
        feature_levels.as_ptr(),
        feature_levels.len() as u32,
        D3D11_SDK_VERSION,
        &sc_desc,
        &mut swapchain,
        &mut device,
        &mut feature_level,
        &mut context,
    ) != S_OK
    {
        None
    } else {
        Some((
            ComPtr::from_raw(swapchain),
            ComPtr::from_raw(device),
            ComPtr::from_raw(context),
        ))
    }
}

unsafe fn create_render_target(
    swapchain: &ComPtr<IDXGISwapChain>,
    device: &ComPtr<ID3D11Device>,
) -> ComPtr<ID3D11RenderTargetView> {
    let mut back_buffer = ptr::null_mut::<ID3D11Texture2D>();
    let mut main_rtv = ptr::null_mut();
    swapchain.GetBuffer(
        0,
        &ID3D11Resource::uuidof(),
        &mut back_buffer as *mut *mut _ as *mut *mut _,
    );
    device.CreateRenderTargetView(back_buffer.cast(), ptr::null_mut(), &mut main_rtv);
    (&*back_buffer).Release();
    ComPtr::from_raw(main_rtv)
}

pub fn main_loop<F>(title: &str, mut run_ui: F)
where
    F: FnMut(&mut bool, &mut Env, &mut Ui),
{
    let mut event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title(title.to_owned())
        .with_inner_size(LogicalSize {
            width: 1024f64,
            height: 768f64,
        })
        .build(&event_loop)
        .unwrap();
    let hwnd = if let RawWindowHandle::Windows(handle) = window.raw_window_handle() {
        handle.hwnd
    } else {
        unreachable!()
    };
    let (swapchain, device, context) = unsafe { create_device(hwnd.cast()) }.unwrap();
    let mut main_rtv = unsafe { create_render_target(&swapchain, &device) };

    let mut imgui = imgui::Context::create();
    imgui.set_ini_filename(Some(PathBuf::from("imgui.ini")));

    if let Some(backend) = clipboard::init() {
        imgui.set_clipboard_backend(Box::new(backend));
    } else {
        eprintln!("Failed to initialize clipboard");
    }

    let mut platform = WinitPlatform::init(&mut imgui);
    platform.attach_window(imgui.io_mut(), &window, HiDpiMode::Rounded);

    let mut env = Env::default();
    let hidpi_factor = platform.hidpi_factor();
    init_fonts(&mut env, &mut imgui, hidpi_factor);

    let mut renderer =
        unsafe { imgui_dx11_renderer::Renderer::new(&mut imgui, device.clone()).unwrap() };
    let clear_color = [0.45, 0.55, 0.60, 1.00];

    let mut last_frame = Instant::now();

    event_loop.run_return(move |event, _, control_flow| match event {
        Event::NewEvents(_) => {
            let now = Instant::now();
            imgui.io_mut().update_delta_time(now - last_frame);
            last_frame = now;
        }
        Event::MainEventsCleared => {
            let io = imgui.io_mut();
            platform
                .prepare_frame(io, &window)
                .expect("Failed to start frame");
            window.request_redraw();
        }
        Event::RedrawRequested(_) => {
            unsafe {
                context.OMSetRenderTargets(1, &main_rtv.as_raw(), ptr::null_mut());
                context.ClearRenderTargetView(main_rtv.as_raw(), &clear_color);
            }
            let mut ui = imgui.frame();

            let mut run = true;
            run_ui(&mut run, &mut env, &mut ui);
            if !run {
                *control_flow = ControlFlow::Exit;
            }

            platform.prepare_render(&ui, &window);
            renderer.render(ui.render()).unwrap();
            unsafe {
                swapchain.Present(1, 0);
            }
        }
        Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } => *control_flow = winit::event_loop::ControlFlow::Exit,
        Event::WindowEvent {
            event: WindowEvent::Resized(winit::dpi::PhysicalSize { height, width }),
            ..
        } => unsafe {
            ptr::drop_in_place(&mut main_rtv);
            swapchain.ResizeBuffers(0, width, height, DXGI_FORMAT_UNKNOWN, 0);
            ptr::write(&mut main_rtv, create_render_target(&swapchain, &device));
            platform.handle_event(imgui.io_mut(), &window, &event);
        },
        Event::LoopDestroyed => (),
        event => {
            platform.handle_event(imgui.io_mut(), &window, &event);
        }
    });
}
