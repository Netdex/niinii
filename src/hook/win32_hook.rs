use winapi::{shared::windef::HWND, um::winuser};

pub unsafe fn hook(hwnd: HWND, func: winuser::WNDPROC) -> winuser::WNDPROC {
    std::mem::transmute(winuser::SetWindowLongPtrA(
        hwnd,
        winuser::GWLP_WNDPROC,
        std::mem::transmute(func),
    ))
}
