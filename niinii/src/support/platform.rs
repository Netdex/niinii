use copypasta::{ClipboardContext, ClipboardProvider};
use imgui::ClipboardBackend;
use winapi::um::winuser::{keybd_event, GetKeyState, KEYEVENTF_KEYUP, VK_SCROLL};

pub struct ClipboardSupport(ClipboardContext);

pub fn init() -> Option<ClipboardSupport> {
    ClipboardContext::new().ok().map(ClipboardSupport)
}

impl ClipboardBackend for ClipboardSupport {
    fn get(&mut self) -> Option<String> {
        self.0.get_contents().ok()
    }
    fn set(&mut self, text: &str) {
        let _ = self.0.set_contents(text.to_owned());
    }
}

pub fn get_scroll_lock() -> bool {
    unsafe {
        let state = GetKeyState(VK_SCROLL);
        (state & 0x0001) != 0
    }
}

pub fn set_scroll_lock(enabled: bool) {
    if get_scroll_lock() != enabled {
        unsafe {
            keybd_event(VK_SCROLL as u8, 0, 0, 0);
            keybd_event(VK_SCROLL as u8, 0, KEYEVENTF_KEYUP, 0);
        }
    }
}
