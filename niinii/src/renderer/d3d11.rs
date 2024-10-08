use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;
use std::{ptr, rc::Weak};

use imgui_winit_support::WinitPlatform;
use raw_window_handle::HasRawWindowHandle;
use raw_window_handle::RawWindowHandle;
use winapi::{
    shared::{
        dxgi::*,
        dxgiformat::*,
        dxgitype::*,
        minwindef::{LPARAM, LRESULT, TRUE, UINT, WPARAM},
        windef::{HHOOK, HWND},
        winerror::S_OK,
    },
    um::{
        d3d11::*,
        d3dcommon::*,
        winuser::{
            self, CallNextHookEx, SetWindowsHookExA, UnhookWindowsHookEx, MSLLHOOKSTRUCT,
            WH_MOUSE_LL,
        },
    },
    Interface as _,
};
use winit::platform::run_on_demand::EventLoopExtRunOnDemand;
use winit::window::WindowLevel;
use winit::{
    event::{DeviceId, Event, WindowEvent},
    event_loop::EventLoop,
};
use wio::com::ComPtr;

use super::context::Context;
use super::context::ContextFlags;
use super::Renderer;
use crate::{app::App, settings::Settings};

struct Inner {
    window: winit::window::Window,
    platform: WinitPlatform,
    imgui: imgui::Context,
    ctx: Context,

    renderer: imgui_dx11_renderer::Renderer,
    context: ComPtr<ID3D11DeviceContext>,
    main_rtv: ComPtr<ID3D11RenderTargetView>,
    swapchain: ComPtr<IDXGISwapChain>,
    device: ComPtr<ID3D11Device>,

    low_level_mouse_proc: Option<HHOOK>,
    last_want_capture_mouse: bool,
    // winit_wnd_proc: winuser::WNDPROC,
}
impl Drop for Inner {
    fn drop(&mut self) {
        let Inner {
            low_level_mouse_proc,
            ..
        } = self;
        if let Some(low_level_mouse_proc) = low_level_mouse_proc {
            unsafe { UnhookWindowsHookEx(*low_level_mouse_proc) };
        }
    }
}

struct Shared {
    inner: RefCell<Inner>,
    event_loop: Cell<Option<EventLoop<()>>>,
}

pub struct D3D11Renderer {
    shared: Rc<Shared>,
}
impl D3D11Renderer {
    pub fn new(settings: &Settings) -> Self {
        let event_loop = EventLoop::new().unwrap();

        // let on_top = settings.on_top || settings.overlay_mode;
        let maximized = settings.overlay_mode;
        let decorations = !settings.overlay_mode;
        let fullscreen = if settings.overlay_mode {
            Some(winit::window::Fullscreen::Borderless(None))
        } else {
            None
        };

        let window = winit::window::WindowBuilder::new()
            .with_title("niinii")
            .with_transparent(true)
            .with_maximized(maximized)
            .with_decorations(decorations)
            // we set this later, or else we segfault ¯\_(ツ)_/¯
            // .with_window_level(if on_top {
            //     WindowLevel::AlwaysOnTop
            // } else {
            //     WindowLevel::Normal
            // })
            .with_fullscreen(fullscreen)
            .build(&event_loop)
            .unwrap();

        if settings.overlay_mode {
            window.set_cursor_hittest(false).unwrap();
        }

        let hwnd = match window.raw_window_handle() {
            RawWindowHandle::Win32(handle) => handle.hwnd,
            _ => unreachable!(),
        };

        let (swapchain, device, context) = unsafe { create_device(hwnd as _) }.unwrap();
        let main_rtv = unsafe { create_render_target(&swapchain, &device) };

        let mut imgui = imgui::Context::create();
        Self::configure_imgui(&mut imgui, settings);
        let dpi = match settings.use_force_dpi {
            true => Some(settings.force_dpi),
            false => None,
        };
        let platform = Self::create_platform(&mut imgui, &window, dpi);
        let mut ctx = Context::new(ContextFlags::SUPPORTS_ATLAS_UPDATE);
        ctx.update_fonts(&mut imgui, platform.hidpi_factor());

        let renderer =
            unsafe { imgui_dx11_renderer::Renderer::new(&mut imgui, device.clone()).unwrap() };

        let mut low_level_mouse_proc: Option<HHOOK> = None;
        // let mut winit_wnd_proc: winuser::WNDPROC = None;
        if settings.overlay_mode {
            // this has to be after renderer is created or else we segfault
            window.set_window_level(WindowLevel::AlwaysOnTop);
            unsafe {
                low_level_mouse_proc.replace(SetWindowsHookExA(
                    WH_MOUSE_LL,
                    Some(mouse_proc),
                    ptr::null_mut(),
                    0,
                ));

                // let orig_wnd_proc = SetWindowLongPtrA(
                //     hwnd as HWND,
                //     winuser::GWL_WNDPROC as i32,
                //     window_proc as *const () as isize,
                // );
                // winit_wnd_proc.replace(std::mem::transmute(orig_wnd_proc));
            }
        }

        let d3d11_renderer = Self {
            shared: Rc::new(Shared {
                inner: RefCell::new(Inner {
                    window,
                    platform,
                    imgui,
                    ctx,
                    renderer,
                    context,
                    main_rtv,
                    swapchain,
                    device,
                    low_level_mouse_proc,
                    last_want_capture_mouse: false,
                    // winit_wnd_proc,
                }),
                event_loop: Cell::new(Some(event_loop)),
            }),
        };
        SYSTEM.with(|system| *system.borrow_mut() = d3d11_renderer.downgrade());
        d3d11_renderer
    }
}
impl Renderer for D3D11Renderer {
    fn run(&mut self, app: &mut App) {
        let clear_color = [0.00, 0.00, 0.00, 0.00];
        let mut last_frame = Instant::now();
        let mut event_loop = self.shared.event_loop.replace(None).unwrap();

        event_loop
            .run_on_demand(|event, window_target| {
                let Inner {
                    window,
                    platform,
                    imgui,
                    ctx,
                    renderer,
                    context,
                    main_rtv,
                    swapchain,
                    device,
                    ..
                } = &mut *self.shared.inner.borrow_mut();
                match event {
                    Event::NewEvents(_) => {
                        let now = Instant::now();
                        imgui
                            .io_mut()
                            .update_delta_time(now.duration_since(last_frame));
                        last_frame = now;
                    }
                    Event::AboutToWait => {
                        let io = imgui.io_mut();
                        platform
                            .prepare_frame(io, window)
                            .expect("failed to start frame");
                        window.request_redraw();
                    }
                    Event::WindowEvent {
                        event: winit::event::WindowEvent::RedrawRequested,
                        ..
                    } => {
                        unsafe {
                            context.OMSetRenderTargets(1, &main_rtv.as_raw(), ptr::null_mut());
                            context.ClearRenderTargetView(main_rtv.as_raw(), &clear_color);
                        }

                        let now = std::time::Instant::now();
                        if ctx.update_fonts(imgui, platform.hidpi_factor()) {
                            unsafe { renderer.rebuild_font_texture(imgui.fonts()).unwrap() };
                            let elapsed = now.elapsed();
                            tracing::info!("rebuilt font atlas (took {:?})", elapsed);
                        }
                        let ui = imgui.frame();
                        let mut run = true;
                        app.ui(ctx, ui, &mut run);
                        if !run {
                            window_target.exit()
                        }
                        platform.prepare_render(ui, window);
                        let draw_data = imgui.render();
                        renderer.render(draw_data).unwrap();
                        unsafe {
                            swapchain.Present(1, 0);
                        }
                    }
                    Event::WindowEvent {
                        event: WindowEvent::CloseRequested,
                        ..
                    } => window_target.exit(),
                    Event::WindowEvent {
                        event: WindowEvent::Resized(winit::dpi::PhysicalSize { height, width }),
                        ..
                    } => unsafe {
                        ptr::drop_in_place(main_rtv);
                        swapchain.ResizeBuffers(0, width, height, DXGI_FORMAT_UNKNOWN, 0);
                        ptr::write(main_rtv, create_render_target(swapchain, device));
                        platform.handle_event(imgui.io_mut(), window, &event);
                    },
                    event => {
                        platform.handle_event(imgui.io_mut(), window, &event);
                    }
                }
            })
            .unwrap();
    }
}
impl D3D11Renderer {
    pub fn downgrade(&self) -> WeakD3D11Renderer {
        WeakD3D11Renderer {
            shared: Rc::downgrade(&self.shared),
        }
    }
}

pub struct WeakD3D11Renderer {
    shared: Weak<Shared>,
}
impl WeakD3D11Renderer {
    pub fn new() -> Self {
        WeakD3D11Renderer {
            shared: Weak::new(),
        }
    }
    pub fn upgrade(&self) -> Option<D3D11Renderer> {
        let shared = self.shared.upgrade()?;
        Some(D3D11Renderer { shared })
    }
}
impl Default for WeakD3D11Renderer {
    fn default() -> Self {
        Self::new()
    }
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
    (*back_buffer).Release();
    ComPtr::from_raw(main_rtv)
}

thread_local! {
    static SYSTEM: RefCell<WeakD3D11Renderer> = RefCell::new(WeakD3D11Renderer::new());
}

// unsafe extern "system" fn window_proc(
//     hwnd: HWND,
//     msg: u32,
//     wparam: WPARAM,
//     lparam: LPARAM,
// ) -> LRESULT {
//     SYSTEM.with(|system| {
//         let weak_d3d11_renderer = system.borrow();
//         if let Some(d3d11_renderer) = weak_d3d11_renderer.upgrade() {
//             let inner = d3d11_renderer.shared.inner.try_borrow_mut();
//             if let Ok(mut inner) = inner {
//                 let Inner {
//                     // imgui,
//                     winit_wnd_proc,
//                     ..
//                 } = &mut *inner;
//                 match msg {
//                     winuser::WM_NCHITTEST => {}
//                     _ => {
//                         if let Some(winit_wnd_proc) = *winit_wnd_proc {
//                             drop(inner); // RefCell is not re-entrant
//                             return winuser::CallWindowProcA(
//                                 Some(winit_wnd_proc),
//                                 hwnd,
//                                 msg,
//                                 wparam,
//                                 lparam,
//                             );
//                         }
//                     }
//                 }
//             }
//         }
//         0
//     })
// }
unsafe extern "system" fn mouse_proc(ncode: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let ms = &mut *(lparam as *mut MSLLHOOKSTRUCT);

    if ncode == winuser::HC_ACTION {
        SYSTEM.with(|system| {
            let weak_d3d11_renderer = system.borrow();
            if let Some(d3d11_renderer) = weak_d3d11_renderer.upgrade() {
                let inner = d3d11_renderer.shared.inner.try_borrow_mut();
                if let Ok(mut inner) = inner {
                    let Inner {
                        window,
                        platform,
                        imgui,
                        last_want_capture_mouse,
                        ..
                    } = &mut *inner;

                    let hwnd = match window.raw_window_handle() {
                        RawWindowHandle::Win32(handle) => handle.hwnd,
                        _ => unreachable!(),
                    };

                    // if the window is transparent, we need to fake mouse move
                    // events so it knows when it wants to capture the mouse
                    if !*last_want_capture_mouse {
                        let wparam = wparam as UINT;
                        if wparam == winuser::WM_MOUSEMOVE {
                            use winit::dpi::PhysicalPosition;

                            let mut client_pos = ms.pt;
                            let r =
                                winuser::ScreenToClient(hwnd as *mut _, &mut client_pos as *mut _);
                            debug_assert_eq!(r, TRUE);
                            let position =
                                PhysicalPosition::new(client_pos.x as f64, client_pos.y as f64);
                            platform.handle_event::<()>(
                                imgui.io_mut(),
                                window,
                                &Event::WindowEvent {
                                    window_id: window.id(),
                                    event: WindowEvent::CursorMoved {
                                        device_id: DeviceId::dummy(),
                                        position,
                                    },
                                },
                            );
                        }
                    }

                    // when we want to capture the mouse make the window opaque,
                    // and when we no longer want to make the window transparent
                    // again (this code causes me physical pain)
                    let io = imgui.io_mut();
                    if *last_want_capture_mouse != io.want_capture_mouse {
                        let style = winuser::GetWindowLongA(hwnd as *mut _, winuser::GWL_EXSTYLE);
                        if io.want_capture_mouse {
                            winuser::SetWindowLongA(
                                hwnd as *mut _,
                                winuser::GWL_EXSTYLE,
                                (style as u32 & (!winuser::WS_EX_TRANSPARENT)) as i32,
                            );
                        } else {
                            winuser::SetWindowLongA(
                                hwnd as *mut _,
                                winuser::GWL_EXSTYLE,
                                (style as u32 | winuser::WS_EX_TRANSPARENT) as i32,
                            );
                        }
                        *last_want_capture_mouse = io.want_capture_mouse;
                    }
                } else {
                    tracing::warn!(
                        "failed to acquire ctx in hook ncode={} wparam={} lparam={}",
                        ncode,
                        wparam,
                        lparam
                    );
                }
            }
        });
    }
    // NOTE: Don't try consuming the message by returning non-zero, because it
    // will also consume mouse move events. Selectively consuming mouse click
    // events doesn't work either because hover still happens.
    CallNextHookEx(ptr::null_mut(), ncode, wparam, lparam)
}
