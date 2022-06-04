use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;
use std::{ptr, rc::Weak};

use imgui_winit_support::WinitPlatform;
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
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
use winit::{
    event::{DeviceId, Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    platform::run_return::EventLoopExtRunReturn,
};
use wio::com::ComPtr;

use super::env::Env;
use super::renderer::Renderer;
use crate::{app::App, view::settings::SettingsView};

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

thread_local! {
    static SYSTEM: RefCell<WeakD3D11Renderer> = RefCell::new(WeakD3D11Renderer::new());
}

unsafe extern "system" fn low_level_mouse_proc(
    ncode: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let ms = &mut *(lparam as *mut MSLLHOOKSTRUCT);

    SYSTEM.with(|system| {
        let weak_d3d11_renderer = system.borrow_mut();
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

                let hwnd = if let RawWindowHandle::Win32(handle) = window.raw_window_handle() {
                    handle.hwnd
                } else {
                    unreachable!()
                };

                use winit::event::ModifiersState;
                let wparam = wparam as UINT;
                if wparam == winuser::WM_MOUSEMOVE {
                    use winit::dpi::PhysicalPosition;

                    let mut client_pos = ms.pt;
                    let r = winuser::ScreenToClient(hwnd as *mut _, &mut client_pos as *mut _);
                    debug_assert_eq!(r, TRUE);
                    let position = PhysicalPosition::new(client_pos.x as f64, client_pos.y as f64);
                    platform.handle_event::<()>(
                        imgui.io_mut(),
                        window,
                        &Event::WindowEvent {
                            window_id: window.id(),
                            event: WindowEvent::CursorMoved {
                                device_id: DeviceId::dummy(),
                                position,
                                modifiers: ModifiersState::empty(),
                            },
                        },
                    );
                }
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
                log::warn!(
                    "failed to acquire ctx in hook ncode={} wparam={} lparam={}",
                    ncode,
                    wparam,
                    lparam
                );
            }
        }
    });
    CallNextHookEx(ptr::null_mut(), ncode, wparam, lparam)
}

pub struct D3D11Renderer {
    shared: Rc<Shared>,
}
struct Shared {
    inner: RefCell<Inner>,
    event_loop: Cell<Option<EventLoop<()>>>,
}
struct Inner {
    window: winit::window::Window,
    platform: WinitPlatform,
    imgui: imgui::Context,
    env: Env,
    renderer: imgui_dx11_renderer::Renderer,
    context: ComPtr<ID3D11DeviceContext>,
    main_rtv: ComPtr<ID3D11RenderTargetView>,
    swapchain: ComPtr<IDXGISwapChain>,
    device: ComPtr<ID3D11Device>,
    mousellhook: Option<HHOOK>,
    last_want_capture_mouse: bool,
}
impl D3D11Renderer {
    pub fn new(settings: &SettingsView) -> Self {
        let event_loop = EventLoop::new();
        let window = Self::create_window_builder(settings)
            .build(&event_loop)
            .unwrap();

        let hwnd = if let RawWindowHandle::Win32(handle) = window.raw_window_handle() {
            handle.hwnd
        } else {
            unreachable!()
        };

        let (swapchain, device, context) = unsafe { create_device(hwnd.cast()) }.unwrap();
        let main_rtv = unsafe { create_render_target(&swapchain, &device) };

        let mut imgui = Self::create_imgui(settings);
        let platform = Self::create_platform(&mut imgui, &window);
        let mut env = Env::new();
        env.update_fonts(&mut imgui, &platform);

        let renderer =
            unsafe { imgui_dx11_renderer::Renderer::new(&mut imgui, device.clone()).unwrap() };

        let mut mousellhook: Option<HHOOK> = None;
        if settings.overlay_mode {
            unsafe {
                let style = winuser::GetWindowLongA(hwnd as *mut _, winuser::GWL_EXSTYLE);
                winuser::SetWindowLongA(
                    hwnd as *mut _,
                    winuser::GWL_EXSTYLE,
                    (style as u32 | winuser::WS_EX_LAYERED) as i32,
                );
                mousellhook.replace(SetWindowsHookExA(
                    WH_MOUSE_LL,
                    Some(low_level_mouse_proc),
                    ptr::null_mut(),
                    0,
                ));
            }
        }

        let d3d11_renderer = Self {
            shared: Rc::new(Shared {
                inner: RefCell::new(Inner {
                    window,
                    platform,
                    imgui,
                    env,
                    renderer,
                    context,
                    main_rtv,
                    swapchain,
                    device,
                    mousellhook,
                    last_want_capture_mouse: false,
                }),
                event_loop: Cell::new(Some(event_loop)),
            }),
        };
        SYSTEM.with(|system| *system.borrow_mut() = d3d11_renderer.downgrade());
        d3d11_renderer
    }
}
impl Renderer for D3D11Renderer {
    fn main_loop(&mut self, app: &mut App) {
        let clear_color = [0.00, 0.00, 0.00, 0.00];
        let mut last_frame = Instant::now();
        let mut event_loop = self.shared.event_loop.replace(None).unwrap();

        event_loop.run_return(|event, _, control_flow| {
            let Inner {
                window,
                platform,
                imgui,
                env,
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
                    imgui.io_mut().update_delta_time(now - last_frame);
                    last_frame = now;
                }
                Event::MainEventsCleared => {
                    let io = imgui.io_mut();
                    platform
                        .prepare_frame(io, window)
                        .expect("failed to start frame");
                    window.request_redraw();
                }
                Event::RedrawRequested(_) => {
                    unsafe {
                        context.OMSetRenderTargets(1, &main_rtv.as_raw(), ptr::null_mut());
                        context.ClearRenderTargetView(main_rtv.as_raw(), &clear_color);
                    }

                    let now = std::time::Instant::now();
                    if env.update_fonts(imgui, platform) {
                        unsafe { renderer.rebuild_font_texture(&mut imgui.fonts()).unwrap() };
                        let elapsed = now.elapsed();
                        log::info!("rebuilt font atlas (took {:?})", elapsed);
                    }
                    let mut ui = imgui.frame();
                    let mut run = true;
                    app.ui(env, &mut ui, &mut run);
                    if !run {
                        *control_flow = ControlFlow::Exit;
                    }
                    platform.prepare_render(&ui, window);
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
                    ptr::drop_in_place(main_rtv);
                    swapchain.ResizeBuffers(0, width, height, DXGI_FORMAT_UNKNOWN, 0);
                    ptr::write(main_rtv, create_render_target(swapchain, device));
                    platform.handle_event(imgui.io_mut(), window, &event);
                },
                Event::LoopDestroyed => (),
                event => {
                    platform.handle_event(imgui.io_mut(), window, &event);
                }
            }
        });
    }
}
impl Drop for Inner {
    fn drop(&mut self) {
        let Inner { mousellhook, .. } = self;
        if let Some(mousellhook) = mousellhook {
            unsafe { UnhookWindowsHookEx(*mousellhook) };
        }
    }
}

pub struct WeakD3D11Renderer {
    shared: Weak<Shared>,
}
impl D3D11Renderer {
    pub fn downgrade(&self) -> WeakD3D11Renderer {
        WeakD3D11Renderer {
            shared: Rc::downgrade(&self.shared),
        }
    }
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
