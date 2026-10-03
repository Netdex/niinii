use std::time::{Duration, Instant};

use copypasta::{ClipboardContext, ClipboardProvider};
use imgui::ClipboardBackend;
use winapi::um::winuser::{
    keybd_event, GetClipboardSequenceNumber, GetKeyState, KEYEVENTF_KEYUP, VK_SCROLL,
};

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

/// Changes whenever the clipboard contents change. Returns 0 if the sequence number is unavailable.
pub fn clipboard_sequence_number() -> u32 {
    unsafe { GetClipboardSequenceNumber() }
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

const SCROLL_LOCK_SETTLE_TIMEOUT: Duration = Duration::from_millis(500);

/// Two-way binding to the scroll lock toggle state. A toggle injected by `set`
/// lags in `GetKeyState`, so `poll` ignores readings until it settles.
pub struct ScrollLockSync {
    state: bool,
    pending_since: Option<Instant>,
}

impl ScrollLockSync {
    pub fn new(initial: bool) -> Self {
        let mut sync = ScrollLockSync {
            state: get_scroll_lock(),
            pending_since: None,
        };
        sync.set(initial);
        sync
    }

    pub fn set(&mut self, enabled: bool) {
        if self.state == enabled {
            return;
        }
        self.state = enabled;
        if get_scroll_lock() != enabled {
            set_scroll_lock(enabled);
            self.pending_since = Some(Instant::now());
        }
    }

    /// Returns the new state if scroll lock was toggled externally.
    pub fn poll(&mut self) -> Option<bool> {
        let current = get_scroll_lock();
        if let Some(since) = self.pending_since {
            if current != self.state && since.elapsed() < SCROLL_LOCK_SETTLE_TIMEOUT {
                return None;
            }
            self.pending_since = None;
        }
        if current != self.state {
            self.state = current;
            Some(current)
        } else {
            None
        }
    }
}
