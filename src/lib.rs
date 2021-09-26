use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::{path::PathBuf, sync::Once};

use detour::GenericDetour;
use lazy_static::lazy_static;
use winapi::shared::minwindef::FALSE;
use winapi::shared::windef::HWND;
use winapi::{
    shared::{
        d3d9, d3d9types,
        minwindef::{BOOL, DWORD, HINSTANCE, LPARAM, LPVOID, LRESULT, TRUE, UINT, WPARAM},
        ntdef::NULL,
    },
    um::{
        consoleapi, libloaderapi, objbase,
        winnt::{self, HRESULT},
        winuser,
    },
};

use app::App;
use common::Env;

use crate::view::SettingsView;

mod app;
mod clipboard;
mod common;
mod hook;
mod view;

lazy_static! {
    static ref DETOUR: GenericDetour<hook::d3d9_hook::EndScene> =
        hook::d3d9_hook::hook::<hook::d3d9_hook::EndScene>(hk_end_scene).unwrap();
}

const STATE_PATH: &str = "niinii.json";

struct AppContext {
    imgui: imgui::Context,
    env: Env,
    app: app::App,
}

struct RenderContext {
    renderer: imgui_dx9_renderer::Renderer,
    // state_block: ComPtr<d3d9::IDirect3DStateBlock9>,
}

static INIT: Once = Once::new();

static mut APP_CONTEXT: Option<AppContext> = None;
static mut RENDER_CONTEXT: Option<RenderContext> = None;
static mut BASE_WND_PROC: winuser::WNDPROC = None;
static mut INVALIDATE_RENDERER: AtomicBool = AtomicBool::new(true);

unsafe fn create_app_context(p_device: d3d9::LPDIRECT3DDEVICE9) -> AppContext {
    println!("create app context");
    // initialize imgui context
    let mut imgui = imgui::Context::create();
    imgui.set_ini_filename(Some(PathBuf::from("imgui.ini")));

    if let Some(backend) = clipboard::init() {
        imgui.set_clipboard_backend(backend);
    } else {
        panic!("failed to initialize clipboard");
    }

    let hwnd = hook::get_hwnd_from_device(p_device).unwrap();
    // hook wnd proc
    BASE_WND_PROC = hook::win32_hook::hook(hwnd, Some(wnd_proc_hook));

    // initialize environment fonts
    let mut env = Env::default();
    common::init_fonts(&mut env, &mut imgui, 1.0);

    let state: SettingsView = File::open(STATE_PATH)
        .ok()
        .map(BufReader::new)
        .and_then(|x| serde_json::from_reader(x).ok())
        .unwrap_or_default();
    let app = App::new(state);

    // let mut hidpi_factor = 1.0;
    // let mut hidpi_factor =
    //     imgui_win32_sys::ImGui_ImplWin32_GetDpiScaleForHwnd(hwnd as *mut _) as f64;
    // hidpi_factor = 1.0 / hidpi_factor;
    // let mut hidpi_factor = winuser::GetDpiForWindow(hwnd) as f64 / 96.0;

    // println!("detected dpi: {}", hidpi_factor);
    // imgui_win32_sys::ImGui_ImplWin32_EnableDpiAwareness(); // this might be bad

    // let mut io = imgui.io_mut();
    // io.font_allow_user_scaling = true;
    // io.display_framebuffer_scale = [hidpi_factor as f32, hidpi_factor as f32];

    // intialize imgui backend
    // imgui_win32_sys::ImGui_ImplWin32_EnableDpiAwareness();
    imgui_win32_sys::ImGui_ImplWin32_Init(hwnd as *mut _);

    AppContext { imgui, env, app }
}

unsafe fn create_render_context(p_device: d3d9::LPDIRECT3DDEVICE9) -> RenderContext {
    println!("create renderer context {:?}", p_device);
    let imgui = &mut APP_CONTEXT.as_mut().unwrap().imgui;
    let renderer = imgui_dx9_renderer::Renderer::new_raw(imgui, p_device).unwrap();

    // backup d3d9 state
    // let mut state_block: d3d9::LPDIRECT3DSTATEBLOCK9 = ptr::null_mut();
    // let hresult = (*p_device).CreateStateBlock(d3d9types::D3DSBT_ALL, &mut state_block);
    // assert!(winerror::SUCCEEDED(hresult));

    RenderContext { renderer }
}

pub unsafe extern "stdcall" fn hk_end_scene(p_device: d3d9::LPDIRECT3DDEVICE9) -> HRESULT {
    INIT.call_once(|| {
        APP_CONTEXT.replace(create_app_context(p_device));
    });

    // fuck everything
    match INVALIDATE_RENDERER.compare_exchange(true, false, Ordering::SeqCst, Ordering::Acquire) {
        Ok(_) => {
            println!("req invalidate renderer cmp excg");
            RENDER_CONTEXT.replace(create_render_context(p_device));
        }
        Err(_) => {}
    }

    let AppContext {
        ref mut imgui,
        ref mut env,
        ref mut app,
    } = &mut APP_CONTEXT.as_mut().unwrap();

    let RenderContext { ref mut renderer } = &mut RENDER_CONTEXT.as_mut().unwrap();

    let x = 0;
    let y = x;
    let width = 100;
    let height = width;
    let rect = d3d9types::D3DRECT {
        x1: x,
        y1: y,
        x2: x + width,
        y2: y + height,
    };

    (*p_device).Clear(
        1,
        &rect as *const d3d9types::D3DRECT,
        d3d9types::D3DCLEAR_TARGET,
        d3d9types::D3DCOLOR_XRGB(0, 0xff, 0),
        0f32,
        0,
    );

    // let mut io = imgui.io_mut();
    // println!("{:?}", io.display_size);
    // println!("{:?}", io.display_framebuffer_scale);
    // println!("{:?} {:?}", io.mouse_pos, io.display_framebuffer_scale);

    imgui_win32_sys::ImGui_ImplWin32_NewFrame();
    let ui = imgui.frame();
    app.ui(env, &ui);

    // backup current d3d9 state
    // let mut preserve_block: d3d9::LPDIRECT3DSTATEBLOCK9 = ptr::null_mut();
    // let hresult = (*p_device).CreateStateBlock(d3d9types::D3DSBT_ALL, &mut preserve_block);
    // assert!(winerror::SUCCEEDED(hresult));
    // let preserve_block = ComPtr::from_raw(preserve_block);

    // apply known-good d3d9 state
    // state_block.Apply();

    match renderer.render(ui.render()) {
        Ok(_) => {}
        Err(err) => println!("{:?}", err),
    }
    // restore previous d3d9 state
    // preserve_block.Apply();

    DETOUR.call(p_device)
}

unsafe extern "system" fn wnd_proc_hook(
    hwnd: HWND,
    msg: UINT,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    use winapi::um::winuser::*;
    let imgui = APP_CONTEXT.as_ref().map(|ctx| &ctx.imgui);

    if imgui_win32_sys::ImGui_ImplWin32_WndProcHandler(hwnd, msg, wparam, lparam) != 0 {
        return TRUE as LRESULT;
    }
    match msg {
        WM_SIZE => {
            if wparam != SIZE_MINIMIZED {
                println!("invalidate renderer due to wm_size");
                INVALIDATE_RENDERER.store(true, Ordering::SeqCst);
            }
        }
        WM_SYSCOMMAND => {
            if wparam & 0xFFF0 == SC_KEYMENU {
                return FALSE as LRESULT;
            }
        }
        WM_MOUSEMOVE | WM_MOUSELEAVE | WM_LBUTTONDOWN | WM_LBUTTONDBLCLK | WM_RBUTTONDOWN
        | WM_RBUTTONDBLCLK | WM_MBUTTONDOWN | WM_MBUTTONDBLCLK | WM_XBUTTONDOWN
        | WM_XBUTTONDBLCLK | WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP | WM_XBUTTONUP
        | WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
            if let Some(imgui) = imgui {
                if imgui.io().want_capture_mouse {
                    return FALSE as LRESULT;
                }
            }
        }
        WM_KEYDOWN | WM_KEYUP | WM_SYSKEYDOWN | WM_SYSKEYUP | WM_CHAR => {
            if let Some(imgui) = imgui {
                if imgui.io().want_capture_keyboard {
                    return FALSE as LRESULT;
                }
            }
        }
        _ => {}
    }
    winuser::CallWindowProcA(BASE_WND_PROC, hwnd, msg, wparam, lparam)
}

unsafe fn attach() {
    consoleapi::AllocConsole();
    println!("attach");
    DETOUR.enable().unwrap();
}
unsafe fn detach() {
    println!("detach");
    if let Some(ctx) = &mut APP_CONTEXT {
        let writer = BufWriter::new(File::create(STATE_PATH).unwrap());
        serde_json::to_writer(writer, &ctx.app.settings()).unwrap();
    }
}

#[no_mangle]
pub unsafe extern "stdcall" fn DllMain(h_inst: HINSTANCE, fdw_reason: DWORD, _: LPVOID) -> BOOL {
    match fdw_reason {
        winnt::DLL_PROCESS_ATTACH => {
            objbase::CoInitialize(NULL);
            libloaderapi::DisableThreadLibraryCalls(h_inst);
            thread::spawn(|| attach());
            TRUE
        }
        winnt::DLL_PROCESS_DETACH => {
            detach();
            TRUE
        }
        _ => TRUE,
    }
}
