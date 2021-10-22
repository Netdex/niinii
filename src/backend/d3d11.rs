use std::ptr;
use std::time::Instant;

use imgui::*;
use imgui_winit_support::WinitPlatform;
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use std::cell::RefCell;
use winapi::{
    shared::{
        dxgi::*,
        dxgiformat::*,
        dxgitype::*,
        minwindef::{LPARAM, LRESULT, TRUE, UINT, WPARAM},
        windef::{HHOOK, HWND},
        winerror::{SUCCEEDED, S_OK},
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

use crate::{
    app::App,
    common::{imgui_init, Env},
};

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
fn to_winit_cursor(cursor: imgui::MouseCursor) -> winit::window::CursorIcon {
    use winit::window::CursorIcon;
    match cursor {
        imgui::MouseCursor::Arrow => CursorIcon::Default,
        imgui::MouseCursor::TextInput => CursorIcon::Text,
        imgui::MouseCursor::ResizeAll => CursorIcon::Move,
        imgui::MouseCursor::ResizeNS => CursorIcon::NsResize,
        imgui::MouseCursor::ResizeEW => CursorIcon::EwResize,
        imgui::MouseCursor::ResizeNESW => CursorIcon::NeswResize,
        imgui::MouseCursor::ResizeNWSE => CursorIcon::NwseResize,
        imgui::MouseCursor::Hand => CursorIcon::Hand,
        imgui::MouseCursor::NotAllowed => CursorIcon::NotAllowed,
    }
}

struct System {
    imgui: Context,
    platform: WinitPlatform,
    window: winit::window::Window,
}

thread_local! {
    static SYSTEM: RefCell<Option<System>> = RefCell::new(None);
}

unsafe extern "system" fn low_level_mouse_proc(
    ncode: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let ms = &mut *(lparam as *mut MSLLHOOKSTRUCT);

    let sink_mouse_event = SYSTEM.with(|system| {
        let mut system = system.borrow_mut();
        let System {
            imgui,
            platform,
            window,
        } = system.as_mut().unwrap();

        let hwnd = if let RawWindowHandle::Windows(handle) = window.raw_window_handle() {
            handle.hwnd
        } else {
            unreachable!()
        };

        use winit::event::{ElementState::*, ModifiersState, MouseButton::*};
        let wparam = wparam as UINT;
        match wparam {
            winuser::WM_LBUTTONDOWN
            | winuser::WM_LBUTTONUP
            | winuser::WM_RBUTTONDOWN
            | winuser::WM_RBUTTONUP => platform.handle_event::<()>(
                imgui.io_mut(),
                &window,
                &Event::WindowEvent {
                    window_id: window.id(),
                    event: WindowEvent::MouseInput {
                        device_id: DeviceId::dummy(),
                        state: match wparam {
                            winuser::WM_LBUTTONDOWN | winuser::WM_RBUTTONDOWN => Pressed,
                            winuser::WM_LBUTTONUP | winuser::WM_RBUTTONUP => Released,
                            _ => unreachable!(),
                        },
                        button: match wparam {
                            winuser::WM_LBUTTONDOWN | winuser::WM_LBUTTONUP => Left,
                            winuser::WM_RBUTTONDOWN | winuser::WM_RBUTTONUP => Right,
                            _ => unreachable!(),
                        },
                        modifiers: ModifiersState::empty(),
                    },
                },
            ),
            winuser::WM_MOUSEMOVE => {
                use winit::dpi::PhysicalPosition;

                let mut client_pos = ms.pt.clone();
                assert_eq!(
                    winuser::ScreenToClient(hwnd as *mut _, &mut client_pos as *mut _),
                    TRUE
                );
                let position = PhysicalPosition::new(client_pos.x as f64, client_pos.y as f64);
                platform.handle_event::<()>(
                    imgui.io_mut(),
                    &window,
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
            winuser::WM_MOUSEWHEEL => {
                use winit::event::{MouseScrollDelta::LineDelta, TouchPhase};

                let value = (ms.mouseData >> 16) as i16;
                let value = value as i32;
                let value = value as f32 / winuser::WHEEL_DELTA as f32;

                platform.handle_event::<()>(
                    imgui.io_mut(),
                    &window,
                    &Event::WindowEvent {
                        window_id: window.id(),
                        event: WindowEvent::MouseWheel {
                            device_id: DeviceId::dummy(),
                            delta: LineDelta(0.0, value),
                            phase: TouchPhase::Moved,
                            modifiers: ModifiersState::empty(),
                        },
                    },
                );
            }
            // winuser::WM_MOUSEHWHEEL => {}
            _ => {}
        }
        let io = imgui.io_mut();
        match wparam {
            winuser::WM_LBUTTONDOWN
            | winuser::WM_LBUTTONUP
            | winuser::WM_RBUTTONDOWN
            | winuser::WM_RBUTTONUP
            | winuser::WM_MOUSEWHEEL
                if io.want_capture_mouse =>
            {
                true
            }
            _ => false,
        }
    });

    if ncode < 0 {
        CallNextHookEx(ptr::null_mut(), ncode, wparam, lparam)
    } else {
        let ret = CallNextHookEx(ptr::null_mut(), ncode, wparam, lparam);
        if sink_mouse_event {
            1
        } else {
            ret
        }
    }
}

pub fn main_loop(window: winit::window::WindowBuilder, app: &mut App) {
    let mut event_loop = EventLoop::new();
    let window = window.build(&event_loop).unwrap();

    let hwnd = if let RawWindowHandle::Windows(handle) = window.raw_window_handle() {
        handle.hwnd
    } else {
        unreachable!()
    };

    let mut mousellhook: Option<HHOOK> = None;
    if app.settings().overlay_mode {
        unsafe {
            let style = winuser::GetWindowLongA(hwnd as *mut _, winuser::GWL_EXSTYLE);
            winuser::SetWindowLongA(
                hwnd as *mut _,
                winuser::GWL_EXSTYLE,
                (style as u32 | winuser::WS_EX_LAYERED | winuser::WS_EX_TRANSPARENT) as i32,
            );
            mousellhook.replace(SetWindowsHookExA(
                WH_MOUSE_LL,
                Some(low_level_mouse_proc),
                ptr::null_mut(),
                0,
            ));
            // winuser::SetLayeredWindowAttributes(hwnd as *mut _, 0, 0, winuser::LWA_COLORKEY);
            // winuser::SetLayeredWindowAttributes(hwnd as *mut _, 0, 255, winuser::LWA_ALPHA);
            // let margin = uxtheme::MARGINS {
            //     cxLeftWidth: -1,
            //     cxRightWidth: -1,
            //     cyTopHeight: -1,
            //     cyBottomHeight: -1,
            // };
            // dwmapi::DwmExtendFrameIntoClientArea(hwnd as *mut _, &margin);
        }
    }

    let (swapchain, device, context) = unsafe { create_device(hwnd.cast()) }.unwrap();
    let mut main_rtv = unsafe { create_render_target(&swapchain, &device) };

    let (platform, mut imgui, mut env) = imgui_init(&window);

    let mut renderer =
        unsafe { imgui_dx11_renderer::Renderer::new(&mut imgui, device.clone()).unwrap() };

    SYSTEM.with(|f| {
        *f.borrow_mut() = Some(System {
            imgui,
            platform,
            window,
        });
    });

    let clear_color = [0.00, 0.00, 0.00, 0.00];
    let mut last_frame = Instant::now();
    event_loop.run_return(move |event, _, control_flow| {
        // *control_flow = ControlFlow::Wait;
        SYSTEM.with(|system| {
            let mut system = system.borrow_mut();
            let System {
                imgui,
                platform,
                window,
            } = system.as_mut().unwrap();

            match event {
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
                    app.ui(&mut env, &mut ui, &mut run);
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
            }
        });
    });

    if let Some(mousellhook) = mousellhook {
        unsafe {
            UnhookWindowsHookEx(mousellhook);
        }
    }
}
