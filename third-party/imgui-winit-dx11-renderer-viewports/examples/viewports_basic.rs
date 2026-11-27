use std::{ptr, time::Instant};

use imgui::{Condition, ConfigFlags, FontConfig, FontSource};
use imgui_winit_dx11_renderer_viewports::{wio::com::ComPtr, Renderer};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winapi::{
    shared::{
        dxgi::{IDXGISwapChain, DXGI_SWAP_CHAIN_DESC, DXGI_SWAP_EFFECT_DISCARD},
        dxgiformat::{DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_FORMAT_UNKNOWN},
        dxgitype::{
            DXGI_MODE_DESC, DXGI_RATIONAL, DXGI_SAMPLE_DESC, DXGI_USAGE_RENDER_TARGET_OUTPUT,
        },
        minwindef::TRUE,
        windef::HWND,
    },
    um::{
        d3d11::{
            D3D11CreateDeviceAndSwapChain, ID3D11Device, ID3D11DeviceContext,
            ID3D11RenderTargetView, ID3D11Resource, D3D11_SDK_VERSION,
        },
        d3dcommon::{D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_10_0, D3D_FEATURE_LEVEL_11_0},
    },
    Interface,
};
use winit::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

fn main() {
    let event_loop = EventLoop::new().expect("Failed to create EventLoop");
    let window = WindowBuilder::new()
        .with_inner_size(LogicalSize::new(800.0, 600.0))
        .with_title("Viewports example")
        .build(&event_loop)
        .expect("Failed to create main window");
    let hwnd = match window.window_handle().unwrap().as_raw() {
        RawWindowHandle::Win32(handle) => handle.hwnd.get() as HWND,
        _ => unreachable!(),
    };

    let (swapchain, device, context) = unsafe { create_device(hwnd) };
    let mut main_rtv = Some(unsafe { create_render_target(&swapchain, &device) });

    let mut imgui = imgui::Context::create();
    imgui
        .io_mut()
        .config_flags
        .insert(ConfigFlags::DOCKING_ENABLE);
    imgui
        .io_mut()
        .config_flags
        .insert(ConfigFlags::VIEWPORTS_ENABLE);
    imgui.set_ini_filename(None);
    load_fonts(&mut imgui, window.scale_factor());

    let mut renderer =
        Renderer::new(&mut imgui, &window, device.clone(), None).expect("Failed to init Renderer");

    let mut last_frame = Instant::now();
    event_loop
        .run(move |event, window_target| {
            window_target.set_control_flow(ControlFlow::Poll);
            renderer.handle_event(&mut imgui, &window, &event);

            match event {
                Event::NewEvents(_) => {
                    let now = Instant::now();
                    imgui.io_mut().update_delta_time(now - last_frame);
                    last_frame = now;
                }
                Event::WindowEvent {
                    window_id,
                    event: WindowEvent::CloseRequested,
                } if window_id == window.id() => window_target.exit(),
                Event::WindowEvent {
                    window_id,
                    event: WindowEvent::Resized(size),
                } if window_id == window.id() && size.width > 0 && size.height > 0 => unsafe {
                    main_rtv = None;
                    swapchain.ResizeBuffers(0, size.width, size.height, DXGI_FORMAT_UNKNOWN, 0);
                    main_rtv = Some(create_render_target(&swapchain, &device));
                },
                Event::WindowEvent {
                    window_id,
                    event: WindowEvent::ScaleFactorChanged { scale_factor, .. },
                } if window_id == window.id() => {
                    load_fonts(&mut imgui, scale_factor);
                    renderer
                        .rebuild_font_texture(imgui.fonts())
                        .expect("Failed to rebuild font texture");
                }
                Event::AboutToWait => window.request_redraw(),
                Event::WindowEvent {
                    window_id,
                    event: WindowEvent::RedrawRequested,
                } if window_id == window.id() => {
                    let main_pos = imgui.main_viewport().pos;
                    let main_size = imgui.main_viewport().size;
                    let ui = imgui.frame();
                    ui.dockspace_over_main_viewport();
                    ui.show_demo_window(&mut true);
                    ui.window("Style Editor")
                        .build(|| ui.show_default_style_editor());
                    // Starts outside the main window, so it opens in its own
                    // viewport window.
                    ui.window("Secondary viewport")
                        .position(
                            [main_pos[0] + main_size[0] + 20.0, main_pos[1]],
                            Condition::FirstUseEver,
                        )
                        .size([320.0, 200.0], Condition::FirstUseEver)
                        .build(|| {
                            ui.text("Rendered into its own D3D11 swapchain.");
                            ui.text(format!("{:.1} FPS", ui.io().framerate));
                        });
                    ui.end_frame_early();

                    renderer.prepare_render(&mut imgui, &window);
                    imgui.update_platform_windows();
                    renderer
                        .update_viewports(&mut imgui, window_target)
                        .expect("Failed to update viewports");

                    let draw_data = imgui.render();
                    let rtv = main_rtv.as_ref().unwrap().as_raw();
                    unsafe {
                        context.OMSetRenderTargets(1, &rtv, ptr::null_mut());
                        context.ClearRenderTargetView(rtv, &[0.1, 0.1, 0.1, 1.0]);
                    }
                    renderer
                        .render(draw_data)
                        .expect("Failed to render main viewport");
                    unsafe { swapchain.Present(1, 0) };

                    renderer
                        .render_viewports(&mut imgui)
                        .expect("Failed to render viewports");
                }
                _ => {}
            }
        })
        .expect("EventLoop error");
}

/// Rasterizes the font at the monitor's pixel size and scales it back down in
/// logical units, so text is drawn 1:1 instead of stretched by the DPI scale.
fn load_fonts(imgui: &mut imgui::Context, scale_factor: f64) {
    let scale = scale_factor as f32;
    imgui.fonts().clear();
    imgui.fonts().add_font(&[FontSource::DefaultFontData {
        config: Some(FontConfig {
            size_pixels: (13.0 * scale).round(),
            ..FontConfig::default()
        }),
    }]);
    imgui.io_mut().font_global_scale = 1.0 / scale;
}

unsafe fn create_device(
    hwnd: HWND,
) -> (
    ComPtr<IDXGISwapChain>,
    ComPtr<ID3D11Device>,
    ComPtr<ID3D11DeviceContext>,
) {
    let desc = DXGI_SWAP_CHAIN_DESC {
        BufferDesc: DXGI_MODE_DESC {
            Width: 0,
            Height: 0,
            RefreshRate: DXGI_RATIONAL {
                Numerator: 0,
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
        BufferCount: 2,
        OutputWindow: hwnd,
        Windowed: TRUE,
        SwapEffect: DXGI_SWAP_EFFECT_DISCARD,
        Flags: 0,
    };
    let levels = [D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_10_0];
    let (mut swapchain, mut device, mut context) =
        (ptr::null_mut(), ptr::null_mut(), ptr::null_mut());
    let hr = D3D11CreateDeviceAndSwapChain(
        ptr::null_mut(),
        D3D_DRIVER_TYPE_HARDWARE,
        ptr::null_mut(),
        0,
        levels.as_ptr(),
        levels.len() as u32,
        D3D11_SDK_VERSION,
        &desc,
        &mut swapchain,
        &mut device,
        ptr::null_mut(),
        &mut context,
    );
    assert!(hr >= 0, "D3D11CreateDeviceAndSwapChain failed: {hr:#010x}");
    (
        ComPtr::from_raw(swapchain),
        ComPtr::from_raw(device),
        ComPtr::from_raw(context),
    )
}

unsafe fn create_render_target(
    swapchain: &ComPtr<IDXGISwapChain>,
    device: &ComPtr<ID3D11Device>,
) -> ComPtr<ID3D11RenderTargetView> {
    let mut back_buffer: *mut ID3D11Resource = ptr::null_mut();
    swapchain.GetBuffer(
        0,
        &ID3D11Resource::uuidof(),
        &mut back_buffer as *mut *mut _ as *mut *mut _,
    );
    let mut rtv = ptr::null_mut();
    device.CreateRenderTargetView(back_buffer, ptr::null(), &mut rtv);
    (*back_buffer).Release();
    ComPtr::from_raw(rtv)
}
