use std::{io, mem, ptr};

use windows_sys::{
    core::BOOL,
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, TRUE, WPARAM},
        Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_TRANSITIONS_FORCEDISABLED},
        System::Threading::GetCurrentThreadId,
        UI::WindowsAndMessaging::{
            CallNextHookEx, GetClassNameW, SetWindowsHookExW, UnhookWindowsHookEx, CWPSTRUCT,
            HHOOK, WH_CALLWNDPROC, WM_SHOWWINDOW,
        },
    },
};

/// winit's window class, shared by every window it creates.
const WINIT_CLASS: &str = "Window Class";

/// Turns off DWM's open and close animations on this thread's winit windows.
///
/// Tooltips and menus are native windows created and shown on demand, so the
/// animation would play on every hover. dear-imgui-winit shows a window in the
/// same call that creates it, so the attribute is set from a hook on
/// `WM_SHOWWINDOW`, which is sent before the window becomes visible.
pub struct NoTransitions(HHOOK);

impl NoTransitions {
    pub fn install() -> io::Result<Self> {
        // SAFETY: a thread hook with a valid procedure; removed on drop.
        let hook = unsafe {
            SetWindowsHookExW(
                WH_CALLWNDPROC,
                Some(on_message),
                ptr::null_mut(),
                GetCurrentThreadId(),
            )
        };
        if hook.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(hook))
    }
}

impl Drop for NoTransitions {
    fn drop(&mut self) {
        // SAFETY: the hook was installed by `install` and not removed since.
        unsafe { UnhookWindowsHookEx(self.0) };
    }
}

unsafe extern "system" fn on_message(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        // SAFETY: for WH_CALLWNDPROC, lparam points to a CWPSTRUCT.
        let message = unsafe { &*(lparam as *const CWPSTRUCT) };
        if message.message == WM_SHOWWINDOW && message.wParam != 0 && is_winit(message.hwnd) {
            let disabled: BOOL = TRUE;
            // SAFETY: a live window and a BOOL-sized attribute value.
            let result = unsafe {
                DwmSetWindowAttribute(
                    message.hwnd,
                    DWMWA_TRANSITIONS_FORCEDISABLED as u32,
                    &disabled as *const BOOL as *const _,
                    mem::size_of::<BOOL>() as u32,
                )
            };
            if result < 0 {
                tracing::warn!(hresult = result, "could not disable window transitions");
            }
        }
    }
    // SAFETY: passes the message on as the hook contract requires.
    unsafe { CallNextHookEx(ptr::null_mut(), code, wparam, lparam) }
}

fn is_winit(hwnd: HWND) -> bool {
    let mut class = [0u16; 32];
    // SAFETY: the buffer length is passed alongside it.
    let len = unsafe { GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32) };
    len > 0 && String::from_utf16_lossy(&class[..len as usize]) == WINIT_CLASS
}
