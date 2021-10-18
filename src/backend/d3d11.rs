use std::ptr;
use std::time::Instant;

use imgui::*;
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use winapi::{
    shared::{dxgi::*, dxgiformat::*, dxgitype::*, minwindef::TRUE, windef::HWND, winerror::S_OK},
    um::{d3d11::*, d3dcommon::*},
    Interface as _,
};
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    platform::run_return::EventLoopExtRunReturn,
};
use wio::com::ComPtr;

use crate::common::{imgui_init, Env};

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

pub fn main_loop<F>(window: winit::window::WindowBuilder, mut run_ui: F)
where
    F: FnMut(&mut bool, &mut Env, &mut Ui),
{
    let mut event_loop = EventLoop::new();
    let window = window.build(&event_loop).unwrap();

    let hwnd = if let RawWindowHandle::Windows(handle) = window.raw_window_handle() {
        handle.hwnd
    } else {
        unreachable!()
    };

    // unsafe {
    //     let style = winuser::GetWindowLongA(hwnd as *mut _, winuser::GWL_EXSTYLE);
    //     winuser::SetWindowLongA(
    //         hwnd as *mut _,
    //         winuser::GWL_EXSTYLE,
    //         (style as u32 | winuser::WS_EX_LAYERED) as i32,
    //     );
    //     // winuser::SetLayeredWindowAttributes(hwnd as *mut _, 0, 0, winuser::LWA_COLORKEY);
    //     winuser::SetLayeredWindowAttributes(hwnd as *mut _, 0, 255, winuser::LWA_ALPHA);
    //     let margin = uxtheme::MARGINS {
    //         cxLeftWidth: -1,
    //         cxRightWidth: -1,
    //         cyTopHeight: -1,
    //         cyBottomHeight: -1,
    //     };
    //     dwmapi::DwmExtendFrameIntoClientArea(hwnd as *mut _, &margin);
    // }

    let (swapchain, device, context) = unsafe { create_device(hwnd.cast()) }.unwrap();
    let mut main_rtv = unsafe { create_render_target(&swapchain, &device) };

    let (mut platform, mut imgui, mut env) = imgui_init(&window);

    let mut renderer =
        unsafe { imgui_dx11_renderer::Renderer::new(&mut imgui, device.clone()).unwrap() };
    let clear_color = [0.00, 0.00, 0.00, 0.00];

    let mut last_frame = Instant::now();

    event_loop.run_return(move |event, _, control_flow| {
        // *control_flow = ControlFlow::Wait;
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
        }
    });
}
