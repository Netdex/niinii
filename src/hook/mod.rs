use winapi::{
    shared::{d3d9, d3d9types, winerror},
    um::{processthreadsapi, winuser},
};

use {
    std::ptr,
    winapi::shared::{
        minwindef::{BOOL, DWORD, FALSE, LPARAM, TRUE},
        windef::HWND,
    },
};

pub mod d3d9_hook;
pub mod win32_hook;

/// Gets *a* HWND belonging to this process. Not necessarily of the main window.
pub unsafe fn get_arbitrary_hwnd() -> Option<HWND> {
    extern "system" fn enum_windows_callback(hwnd: HWND, l_param: LPARAM) -> BOOL {
        let mut wnd_proc_id: DWORD = 0;
        unsafe {
            winuser::GetWindowThreadProcessId(hwnd, &mut wnd_proc_id as *mut DWORD);
            if processthreadsapi::GetCurrentProcessId() != wnd_proc_id {
                return TRUE;
            }
            *(l_param as *mut HWND) = hwnd;
        }
        FALSE
    }

    let mut hwnd: HWND = ptr::null_mut();
    winuser::EnumWindows(
        Some(enum_windows_callback),
        &mut hwnd as *mut HWND as LPARAM,
    );
    if hwnd.is_null() {
        None
    } else {
        Some(hwnd)
    }
}

/// Gets the HWND of the window attached to a Direct3DDevice9.
pub unsafe fn get_hwnd_from_device(p_device: d3d9::LPDIRECT3DDEVICE9) -> Option<HWND> {
    let mut parameters = d3d9types::D3DDEVICE_CREATION_PARAMETERS {
        AdapterOrdinal: 0,
        DeviceType: 0,
        hFocusWindow: ptr::null_mut(),
        BehaviorFlags: 0,
    };
    let hresult = (*p_device).GetCreationParameters(&mut parameters);
    if winerror::SUCCEEDED(hresult) {
        Some(parameters.hFocusWindow)
    } else {
        None
    }
}
