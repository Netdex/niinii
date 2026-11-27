use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use dear_imgui_rs::NativeWindowInstanceId;
use dear_imgui_winit::WinitPlatform;
use winit::{
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
    window::{Window, WindowLevel},
};

use super::{Error, Frame};

/// Borderless-fullscreen games keep raising themselves above top-most
/// windows, so those are raised again this often.
const TOPMOST_REFRESH: Duration = Duration::from_millis(250);

/// Native window state the gui has applied on top of what the platform
/// backend manages.
#[derive(Clone, Copy, Default, PartialEq)]
struct Policy {
    /// Pointer input reaches other applications. ImGui's own `NoInputs`
    /// viewport flag only passes input to windows of the same thread.
    passthrough: bool,
    /// Raised to always-on-top. Lowered again only if raised here, so windows
    /// the backend makes top-most (tooltips, popups) are left alone.
    on_top: bool,
}

impl Policy {
    fn apply(self, applied: Policy, window: &Window) -> Result<(), Error> {
        if self.passthrough != applied.passthrough {
            window.set_cursor_hittest(!self.passthrough)?;
        }
        if self.on_top != applied.on_top {
            window.set_window_level(if self.on_top {
                WindowLevel::AlwaysOnTop
            } else {
                WindowLevel::Normal
            });
        }
        Ok(())
    }
}

pub(super) struct NativeWindows {
    applied: HashMap<NativeWindowInstanceId, Policy>,
    last_raise: Instant,
}

impl Default for NativeWindows {
    fn default() -> Self {
        Self {
            applied: HashMap::new(),
            last_raise: Instant::now(),
        }
    }
}

impl NativeWindows {
    /// Brings every live secondary window in line with what `frame` declared.
    pub(super) fn reconcile(
        &mut self,
        platform: &WinitPlatform,
        frame: &Frame,
    ) -> Result<(), Error> {
        let raise = frame.on_top() && self.last_raise.elapsed() >= TOPMOST_REFRESH;
        if raise {
            self.last_raise = Instant::now();
        }
        let mut live = HashMap::new();
        for snapshot in platform.window_snapshots()? {
            if snapshot.target.is_main {
                continue;
            }
            let instance = snapshot.target.instance_id;
            let want = Policy {
                passthrough: frame.passes_input_through(snapshot.target.viewport_id),
                on_top: frame.on_top(),
            };
            let applied = self.applied.get(&instance).copied().unwrap_or_default();
            if want != applied {
                platform
                    .with_window(instance, |window| want.apply(applied, window))?
                    .transpose()?;
            } else if raise && want.on_top {
                platform.with_window(instance, raise_topmost)?.transpose()?;
            }
            live.insert(instance, want);
        }
        self.applied = live;
        Ok(())
    }
}

/// Moves `window` to the top of the top-most windows, without activating it.
/// winit skips this when the window is already marked always-on-top.
fn raise_topmost(window: &Window) -> Result<(), Error> {
    use winapi::{
        shared::windef::HWND,
        um::winuser::{SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE},
    };
    let RawWindowHandle::Win32(handle) = window.window_handle()?.as_raw() else {
        return Ok(());
    };
    let hwnd = handle.hwnd.get() as HWND;
    // SAFETY: `hwnd` belongs to a live window borrowed for this call.
    let ok = unsafe {
        SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )
    };
    if ok == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}
