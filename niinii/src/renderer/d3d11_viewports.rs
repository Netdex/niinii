use std::{
    ptr,
    time::{Duration, Instant},
};

use imgui::{ConfigFlags, ViewportFlags};
use imgui_winit_dx11_renderer_viewports::Renderer as ViewportsRenderer;
use raw_window_handle_06::{HasWindowHandle, RawWindowHandle};
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
        d3dcommon::{D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_10_0, D3D_FEATURE_LEVEL_11_1},
        winuser::{
            self, GetCursorPos, SetWindowLongA, SetWindowPos, HWND_TOPMOST, SWP_FRAMECHANGED,
            SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW,
        },
    },
    Interface as _,
};
use winit::{
    dpi::PhysicalPosition,
    event::{DeviceId, Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    platform::run_on_demand::EventLoopExtRunOnDemand,
    window::{Fullscreen, Window, WindowBuilder, WindowLevel},
};
use wio::com::ComPtr;

use super::{
    context::{Context, ContextFlags},
    Renderer,
};
use crate::{app::App, settings::Settings, support::docking::UiDocking};

const TOPMOST_REFRESH: Duration = Duration::from_millis(250);

pub struct D3D11ViewportsRenderer {
    event_loop: EventLoop<()>,
    window: Window,
    hwnd: HWND,
    on_top: bool,
    imgui: imgui::Context,
    ctx: Context,
    renderer: ViewportsRenderer,
    device: ComPtr<ID3D11Device>,
    context: ComPtr<ID3D11DeviceContext>,
    main_rtv: Option<ComPtr<ID3D11RenderTargetView>>,
    swapchain: ComPtr<IDXGISwapChain>,
}

impl D3D11ViewportsRenderer {
    pub fn new(settings: &Settings) -> Self {
        // The main window is a transparent, click-through host covering the
        // primary monitor, so windows can dock to the screen; windows dragged
        // off it become their own OS windows.
        let event_loop = EventLoop::new().unwrap();
        let window = WindowBuilder::new()
            .with_title("niinii")
            .with_transparent(true)
            .with_decorations(false)
            .with_fullscreen(Some(Fullscreen::Borderless(None)))
            .build(&event_loop)
            .unwrap();
        window.set_cursor_hittest(false).unwrap();

        let hwnd = match window.window_handle().unwrap().as_raw() {
            RawWindowHandle::Win32(handle) => handle.hwnd.get() as HWND,
            _ => unreachable!(),
        };
        unsafe { apply_host_style(hwnd, settings.on_top) };

        let (swapchain, device, context) = unsafe { create_device(hwnd) }.unwrap();
        let main_rtv = Some(unsafe { create_render_target(&swapchain, &device) });

        let mut imgui = imgui::Context::create();
        Self::configure_imgui(&mut imgui, settings);
        imgui
            .io_mut()
            .config_flags
            .insert(ConfigFlags::VIEWPORTS_ENABLE);

        let mut ctx = Context::new(ContextFlags::SUPPORTS_ATLAS_UPDATE);
        ctx.update_fonts(&mut imgui, window.scale_factor());

        let on_top = settings.on_top;
        let renderer = ViewportsRenderer::new(
            &mut imgui,
            &window,
            device.clone(),
            Some(Box::new(move |viewport: &imgui::Viewport| {
                WindowBuilder::new()
                    .with_resizable(true)
                    .with_transparent(true)
                    .with_decorations(!viewport.flags.contains(ViewportFlags::NO_DECORATION))
                    .with_window_level(if on_top {
                        WindowLevel::AlwaysOnTop
                    } else {
                        WindowLevel::Normal
                    })
            })),
        )
        .unwrap();

        Self {
            event_loop,
            window,
            hwnd,
            on_top: settings.on_top,
            imgui,
            ctx,
            renderer,
            device,
            context,
            main_rtv,
            swapchain,
        }
    }
}

impl Renderer for D3D11ViewportsRenderer {
    fn run(&mut self, app: &mut App) {
        let D3D11ViewportsRenderer {
            event_loop,
            window,
            hwnd,
            on_top,
            imgui,
            ctx,
            renderer,
            device,
            context,
            main_rtv,
            swapchain,
        } = self;
        let hwnd = *hwnd;
        let on_top = *on_top;
        let mut last_frame = Instant::now();
        let mut last_topmost_refresh = Instant::now();
        let mut passthrough = true;

        event_loop
            .run_on_demand(|event, window_target| {
                window_target.set_control_flow(ControlFlow::Wait);
                renderer.handle_event(imgui, window, &event);

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
                        *main_rtv = None;
                        swapchain.ResizeBuffers(0, size.width, size.height, DXGI_FORMAT_UNKNOWN, 0);
                        *main_rtv = Some(create_render_target(swapchain, device));
                    },
                    Event::AboutToWait => window.request_redraw(),
                    Event::WindowEvent {
                        window_id,
                        event: WindowEvent::RedrawRequested,
                    } if window_id == window.id() => {
                        // Best effort: borderless-fullscreen games keep raising
                        // themselves above topmost windows.
                        if on_top && last_topmost_refresh.elapsed() >= TOPMOST_REFRESH {
                            // Host first, so floating windows end up above it.
                            unsafe { apply_host_style(hwnd, true) };
                            renderer
                                .refresh_topmost()
                                .expect("failed to refresh topmost windows");
                            last_topmost_refresh = Instant::now();
                        }
                        update_passthrough(imgui, window, hwnd, renderer, &mut passthrough);

                        if ctx.poll_fonts(imgui, window.scale_factor()) {
                            renderer
                                .rebuild_font_texture(imgui.fonts())
                                .expect("failed to rebuild font texture");
                        }

                        let ui = imgui.frame();
                        ui.dockspace_over_viewport();
                        let mut run = true;
                        app.ui(ctx, ui, &mut run);
                        if !run {
                            window_target.exit();
                        }
                        ui.end_frame_early();

                        renderer.prepare_render(imgui, window);
                        imgui.update_platform_windows();
                        renderer
                            .update_viewports(imgui, window_target)
                            .expect("failed to update viewports");

                        let draw_data = imgui.render();
                        unsafe {
                            let rtv = main_rtv.as_ref().unwrap().as_raw();
                            context.OMSetRenderTargets(1, &rtv, ptr::null_mut());
                            context.ClearRenderTargetView(rtv, &[0.0; 4]);
                        }
                        renderer
                            .render(draw_data)
                            .expect("failed to render main viewport");
                        // Viewport windows present without vsync; this paces the loop.
                        unsafe { swapchain.Present(1, 0) };
                        renderer
                            .render_viewports(imgui)
                            .expect("failed to render viewports");
                    }
                    _ => {}
                }
            })
            .unwrap();
    }
}

/// Makes the host window click-through except where imgui wants the mouse.
/// While click-through it receives no mouse events, so the cursor position is
/// fed to imgui directly.
fn update_passthrough(
    imgui: &mut imgui::Context,
    window: &Window,
    hwnd: HWND,
    renderer: &mut ViewportsRenderer,
    passthrough: &mut bool,
) {
    let mut point = unsafe { std::mem::zeroed() };
    let over_host = unsafe { GetCursorPos(&mut point) } != 0
        && !renderer.viewport_window_contains(PhysicalPosition::new(point.x, point.y));
    if *passthrough && over_host {
        if let Ok(origin) = window.inner_position() {
            let event: Event<()> = Event::WindowEvent {
                window_id: window.id(),
                event: WindowEvent::CursorMoved {
                    device_id: unsafe { DeviceId::dummy() },
                    position: PhysicalPosition::new(
                        f64::from(point.x - origin.x),
                        f64::from(point.y - origin.y),
                    ),
                },
            };
            renderer.handle_event(imgui, window, &event);
        }
    }

    let want = !(over_host && imgui.io().want_capture_mouse);
    if want != *passthrough {
        unsafe {
            let style = winuser::GetWindowLongA(hwnd, winuser::GWL_EXSTYLE) as u32;
            let style = if want {
                style | winuser::WS_EX_TRANSPARENT
            } else {
                style & !winuser::WS_EX_TRANSPARENT
            };
            SetWindowLongA(hwnd, winuser::GWL_EXSTYLE, style as i32);
        }
        *passthrough = want;
    }
}

/// Layered (for click-through) and kept off the taskbar; topmost only when the
/// "Always on-top" setting is on.
unsafe fn apply_host_style(hwnd: HWND, topmost: bool) {
    let style = winuser::GetWindowLongA(hwnd, winuser::GWL_EXSTYLE) as u32;
    let mut ex_style = style | winuser::WS_EX_LAYERED | winuser::WS_EX_TOOLWINDOW;
    if topmost {
        ex_style |= winuser::WS_EX_TOPMOST;
    }
    let mut flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW;
    if !topmost {
        flags |= SWP_NOZORDER;
    }
    // SWP_FRAMECHANGED forces a non-client recalculation, so only request it when the style
    // actually changed.
    if ex_style != style {
        SetWindowLongA(hwnd, winuser::GWL_EXSTYLE, ex_style as i32);
        flags |= SWP_FRAMECHANGED;
    }
    let insert_after = if topmost {
        HWND_TOPMOST
    } else {
        ptr::null_mut()
    };
    SetWindowPos(hwnd, insert_after, 0, 0, 0, 0, flags);
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

    let mut swapchain = ptr::null_mut();
    let mut device = ptr::null_mut();
    let mut context = ptr::null_mut();
    let feature_levels = [D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_10_0];
    let hr = D3D11CreateDeviceAndSwapChain(
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
        ptr::null_mut(),
        &mut context,
    );
    if hr < 0 {
        tracing::error!(
            hr = format_args!("{hr:#010x}"),
            "failed to create D3D11 device"
        );
        return None;
    }
    Some((
        ComPtr::from_raw(swapchain),
        ComPtr::from_raw(device),
        ComPtr::from_raw(context),
    ))
}

unsafe fn create_render_target(
    swapchain: &ComPtr<IDXGISwapChain>,
    device: &ComPtr<ID3D11Device>,
) -> ComPtr<ID3D11RenderTargetView> {
    let mut back_buffer = ptr::null_mut::<ID3D11Resource>();
    let mut main_rtv = ptr::null_mut();
    swapchain.GetBuffer(
        0,
        &ID3D11Resource::uuidof(),
        &mut back_buffer as *mut *mut _ as *mut *mut _,
    );
    device.CreateRenderTargetView(back_buffer, ptr::null(), &mut main_rtv);
    (*back_buffer).Release();
    ComPtr::from_raw(main_rtv)
}
