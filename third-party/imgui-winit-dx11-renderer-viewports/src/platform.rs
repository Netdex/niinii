//! winit platform integration: input translation and the imgui viewport
//! backends. Ported from imgui-winit-glow-renderer-viewports with multi-DPI
//! support: each viewport maps to imgui's coordinates through one monitor's
//! mapping (see `desktop`), which this crate chooses (see
//! `Renderer::update_dpi`) rather than winit or imgui.

use std::{cell::RefCell, collections::VecDeque, ptr::null_mut, rc::Rc};

use crate::desktop::Desktop;

use imgui::{BackendFlags, Id, Io, Key, MouseButton, ViewportFlags};
use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::{ElementState, MouseScrollDelta, TouchPhase, WindowEvent},
    keyboard::{Key as WinitKey, KeyLocation, NamedKey},
    platform::modifier_supplement::KeyEventExtModifierSupplement,
    window::{CursorIcon, Window},
};

#[derive(Debug)]
pub(crate) enum ViewportEvent {
    Create(Id),
    Destroy(Id),
    SetPos(Id, [f32; 2]),
    SetSize(Id, [f32; 2]),
    SetVisible(Id),
    SetFocus(Id),
    SetTitle(Id, String),
    SetNoInputs(Id, bool),
}

/// Per-viewport state imgui polls through the platform backend. Stored behind
/// `Viewport::platform_user_data`; positions and sizes are logical.
pub(crate) struct ViewportData {
    pub(crate) pos: [f32; 2],
    pub(crate) size: [f32; 2],
    pub(crate) focus: bool,
    pub(crate) minimized: bool,
    /// Last `ViewportFlags::NO_INPUTS` state applied to the OS window.
    pub(crate) no_inputs: bool,
    /// Index of the monitor whose mapping this viewport's coordinates use.
    pub(crate) monitor: usize,
    /// That monitor's scale; reported to imgui through
    /// `Platform_GetWindowDpiScale` so imgui never derives its own.
    pub(crate) dpi: f32,
}

pub(crate) unsafe extern "C" fn get_window_dpi_scale(
    viewport: *mut imgui::sys::ImGuiViewport,
) -> f32 {
    let data = (*viewport).PlatformUserData.cast::<ViewportData>();
    if data.is_null() {
        1.0
    } else {
        (*data).dpi
    }
}

unsafe fn viewport_data(viewport: &mut imgui::Viewport) -> &mut ViewportData {
    &mut *viewport.platform_user_data.cast::<ViewportData>()
}

/// Sets up io, the monitors and the main viewport for `main_window`.
pub(crate) fn init(imgui: &mut imgui::Context, main_window: &Window, desktop: &Desktop) {
    let dpi_scale = main_window.scale_factor();
    let io = imgui.io_mut();
    io.backend_flags
        .insert(BackendFlags::PLATFORM_HAS_VIEWPORTS);
    io.backend_flags
        .insert(BackendFlags::RENDERER_HAS_VIEWPORTS);
    io.backend_flags.insert(BackendFlags::HAS_MOUSE_CURSORS);
    io.backend_flags.insert(BackendFlags::HAS_SET_MOUSE_POS);

    let logical_size = main_window
        .inner_size()
        .cast::<f32>()
        .to_logical::<f32>(dpi_scale);
    io.display_size = logical_size.into();
    io.display_framebuffer_scale = [dpi_scale as f32, dpi_scale as f32];

    let inner_pos = main_window.inner_position().unwrap_or_default();
    let monitor = desktop.monitor_at(PhysicalPosition::new(inner_pos.x + 1, inner_pos.y + 1));
    let logical_pos: [f32; 2] = desktop.to_logical(monitor, inner_pos);
    let viewport = imgui.main_viewport_mut();
    viewport.pos = logical_pos;
    viewport.work_pos = logical_pos;
    viewport.size = logical_size.into();
    viewport.work_size = logical_size.into();
    viewport.dpi_scale = dpi_scale as f32;
    viewport.platform_user_data = Box::into_raw(Box::new(ViewportData {
        pos: logical_pos,
        size: logical_size.into(),
        focus: true,
        minimized: false,
        no_inputs: false,
        monitor,
        dpi: dpi_scale as f32,
    }))
    .cast();

    imgui
        .platform_io_mut()
        .monitors
        .replace_from_slice(&desktop.platform_monitors());

    imgui.set_platform_name(Some(format!(
        "imgui-winit-dx11-renderer-viewports {}",
        env!("CARGO_PKG_VERSION")
    )));
}

/// Handles one event for `window`. `viewport_id` is `None` for the main window.
pub(crate) fn handle_window_event(
    imgui: &mut imgui::Context,
    window: &Window,
    viewport_id: Option<Id>,
    event: &WindowEvent,
    desktop: &Desktop,
) {
    fn viewport(imgui: &mut imgui::Context, id: Option<Id>) -> Option<&mut imgui::Viewport> {
        match id {
            None => Some(imgui.main_viewport_mut()),
            Some(id) => imgui.viewport_by_id_mut(id),
        }
    }
    let is_main = viewport_id.is_none();
    let Some(dpi_scale) = viewport(imgui, viewport_id).map(|vp| f64::from(vp.dpi_scale)) else {
        return;
    };

    match *event {
        WindowEvent::Resized(new_size) => {
            let logical_size = new_size.to_logical::<f32>(dpi_scale);
            let vp = viewport(imgui, viewport_id).unwrap();
            // imgui owns the size of undecorated viewports; the OS only resizes
            // them on DPI changes, with a size scaled for a DPI we may not use.
            let os_owns_size = is_main || !vp.flags.contains(ViewportFlags::NO_DECORATION);
            let data = unsafe { viewport_data(vp) };
            data.minimized = new_size.width == 0 || new_size.height == 0;
            if os_owns_size {
                data.size = logical_size.into();
                vp.platform_request_resize = true;
            }
            if is_main {
                imgui.io_mut().display_size = logical_size.into();
            }
        }
        WindowEvent::Moved(_) => {
            let Ok(pos) = window.inner_position() else {
                return;
            };
            let vp = viewport(imgui, viewport_id).unwrap();
            let data = unsafe { viewport_data(vp) };
            if is_main {
                data.monitor = desktop.monitor_at(PhysicalPosition::new(pos.x + 1, pos.y + 1));
            }
            data.pos = desktop.to_logical(data.monitor, pos);
            vp.platform_request_move = true;
        }
        WindowEvent::CloseRequested if !is_main => {
            viewport(imgui, viewport_id).unwrap().platform_request_close = true;
        }
        WindowEvent::Focused(focused) => {
            let vp = viewport(imgui, viewport_id).unwrap();
            unsafe { viewport_data(vp).focus = focused };
        }
        WindowEvent::KeyboardInput { ref event, .. } => {
            let io = imgui.io_mut();
            if let Some(txt) = &event.text {
                for ch in txt.chars() {
                    io.add_input_character(ch);
                }
            }
            let key = event.key_without_modifiers();
            let pressed = event.state == ElementState::Pressed;
            // Report both the generic modifier and the specific key so apps can
            // check either. https://github.com/ocornut/imgui/issues/5047
            handle_key_modifier(io, &key, pressed);
            if let Some(key) = to_imgui_key(key, event.location) {
                io.add_key_event(key, pressed);
            }
        }
        WindowEvent::ModifiersChanged(modifiers) => {
            let state = modifiers.state();
            let io = imgui.io_mut();
            io.add_key_event(Key::ModShift, state.shift_key());
            io.add_key_event(Key::ModCtrl, state.control_key());
            io.add_key_event(Key::ModAlt, state.alt_key());
            io.add_key_event(Key::ModSuper, state.super_key());
        }
        // With viewports enabled `Renderer::handle_event` converts the cursor.
        WindowEvent::CursorMoved { position, .. } => {
            imgui
                .io_mut()
                .add_mouse_pos_event([position.x as f32, position.y as f32]);
        }
        WindowEvent::MouseWheel {
            delta,
            phase: TouchPhase::Moved,
            ..
        } => {
            let [h, v] = match delta {
                MouseScrollDelta::LineDelta(h, v) => [h, v],
                MouseScrollDelta::PixelDelta(pos) => [step(pos.x), step(pos.y)],
            };
            imgui.io_mut().add_mouse_wheel_event([h, v]);
        }
        WindowEvent::MouseInput { state, button, .. } => {
            if let Some(button) = to_imgui_mouse_button(button) {
                imgui
                    .io_mut()
                    .add_mouse_button_event(button, state == ElementState::Pressed);
            }
        }
        WindowEvent::ScaleFactorChanged {
            scale_factor,
            ref inner_size_writer,
        } => {
            let vp = viewport(imgui, viewport_id).unwrap();
            if is_main {
                vp.dpi_scale = scale_factor as f32;
                unsafe { viewport_data(vp).dpi = scale_factor as f32 };
                vp.platform_request_resize = true;
                imgui.io_mut().display_framebuffer_scale =
                    [scale_factor as f32, scale_factor as f32];
            } else {
                // Windows' choice of monitor depends on the window's size, which
                // depends on the DPI, so following it oscillates at a monitor
                // boundary. Keep the size this crate laid the viewport out at.
                let size = unsafe { viewport_data(vp).size };
                let dpi = vp.dpi_scale;
                // Only fails after the event has been dispatched, which cannot
                // have happened while we are handling it.
                let _ = inner_size_writer
                    .clone()
                    .request_inner_size(PhysicalSize::new(
                        (size[0] * dpi).round() as u32,
                        (size[1] * dpi).round() as u32,
                    ));
            }
        }
        _ => {}
    }
}

/// Collapses a pixel scroll delta to one wheel step; `f64::signum` maps 0 to 1.
fn step(delta: f64) -> f32 {
    if delta > 0.0 {
        1.0
    } else if delta < 0.0 {
        -1.0
    } else {
        0.0
    }
}

pub(crate) struct PlatformBackend {
    pub(crate) event_queue: Rc<RefCell<VecDeque<ViewportEvent>>>,
    pub(crate) desktop: Rc<Desktop>,
}

impl PlatformBackend {
    fn push(&self, event: ViewportEvent) {
        self.event_queue.borrow_mut().push_back(event);
    }
}

impl imgui::PlatformViewportBackend for PlatformBackend {
    fn create_window(&mut self, viewport: &mut imgui::Viewport) {
        // Same rule as `Renderer::update_dpi` uses for windows at rest.
        let monitor = self.desktop.monitor_at_logical(viewport.pos);
        let dpi = self.desktop.scale(monitor);
        viewport.dpi_scale = dpi;
        viewport.platform_user_data = Box::into_raw(Box::new(ViewportData {
            pos: viewport.pos,
            size: viewport.size,
            focus: false,
            minimized: false,
            no_inputs: false,
            monitor,
            dpi,
        }))
        .cast();
        self.push(ViewportEvent::Create(viewport.id));
    }

    fn destroy_window(&mut self, viewport: &mut imgui::Viewport) {
        if !viewport.platform_user_data.is_null() {
            unsafe {
                drop(Box::from_raw(
                    viewport.platform_user_data.cast::<ViewportData>(),
                ))
            };
            viewport.platform_user_data = null_mut();
        }
        self.push(ViewportEvent::Destroy(viewport.id));
    }

    fn show_window(&mut self, viewport: &mut imgui::Viewport) {
        self.push(ViewportEvent::SetVisible(viewport.id));
    }

    fn set_window_pos(&mut self, viewport: &mut imgui::Viewport, pos: [f32; 2]) {
        self.push(ViewportEvent::SetPos(viewport.id, pos));
    }

    fn get_window_pos(&mut self, viewport: &mut imgui::Viewport) -> [f32; 2] {
        unsafe { viewport_data(viewport).pos }
    }

    fn set_window_size(&mut self, viewport: &mut imgui::Viewport, size: [f32; 2]) {
        self.push(ViewportEvent::SetSize(viewport.id, size));
    }

    fn get_window_size(&mut self, viewport: &mut imgui::Viewport) -> [f32; 2] {
        unsafe { viewport_data(viewport).size }
    }

    fn set_window_focus(&mut self, viewport: &mut imgui::Viewport) {
        self.push(ViewportEvent::SetFocus(viewport.id));
    }

    fn get_window_focus(&mut self, viewport: &mut imgui::Viewport) -> bool {
        unsafe { viewport_data(viewport).focus }
    }

    fn get_window_minimized(&mut self, viewport: &mut imgui::Viewport) -> bool {
        unsafe { viewport_data(viewport).minimized }
    }

    fn set_window_title(&mut self, viewport: &mut imgui::Viewport, title: &str) {
        self.push(ViewportEvent::SetTitle(viewport.id, title.to_owned()));
    }

    fn set_window_alpha(&mut self, _viewport: &mut imgui::Viewport, _alpha: f32) {}

    fn update_window(&mut self, viewport: &mut imgui::Viewport) {
        let no_inputs = viewport.flags.contains(ViewportFlags::NO_INPUTS);
        let id = viewport.id;
        let data = unsafe { viewport_data(viewport) };
        if data.no_inputs != no_inputs {
            data.no_inputs = no_inputs;
            self.push(ViewportEvent::SetNoInputs(id, no_inputs));
        }
    }

    fn render_window(&mut self, _viewport: &mut imgui::Viewport) {}

    fn swap_buffers(&mut self, _viewport: &mut imgui::Viewport) {}

    fn create_vk_surface(
        &mut self,
        _viewport: &mut imgui::Viewport,
        _instance: u64,
        _out_surface: &mut u64,
    ) -> i32 {
        0
    }
}

/// Rendering is driven explicitly by `Renderer::render_viewports`, so imgui's
/// renderer callbacks are no-ops.
pub(crate) struct RendererBackend;

impl imgui::RendererViewportBackend for RendererBackend {
    fn create_window(&mut self, _viewport: &mut imgui::Viewport) {}

    fn destroy_window(&mut self, _viewport: &mut imgui::Viewport) {}

    fn set_window_size(&mut self, _viewport: &mut imgui::Viewport, _size: [f32; 2]) {}

    fn render_window(&mut self, _viewport: &mut imgui::Viewport) {}

    fn swap_buffers(&mut self, _viewport: &mut imgui::Viewport) {}
}

pub(crate) fn to_winit_cursor(cursor: imgui::MouseCursor) -> CursorIcon {
    match cursor {
        imgui::MouseCursor::Arrow => CursorIcon::Default,
        imgui::MouseCursor::TextInput => CursorIcon::Text,
        imgui::MouseCursor::ResizeAll => CursorIcon::Move,
        imgui::MouseCursor::ResizeNS => CursorIcon::NsResize,
        imgui::MouseCursor::ResizeEW => CursorIcon::EwResize,
        imgui::MouseCursor::ResizeNESW => CursorIcon::NeswResize,
        imgui::MouseCursor::ResizeNWSE => CursorIcon::NwseResize,
        imgui::MouseCursor::Hand => CursorIcon::Grab,
        imgui::MouseCursor::NotAllowed => CursorIcon::NotAllowed,
    }
}

fn handle_key_modifier(io: &mut Io, key: &WinitKey, down: bool) {
    match key {
        WinitKey::Named(NamedKey::Shift) => io.add_key_event(Key::ModShift, down),
        WinitKey::Named(NamedKey::Control) => io.add_key_event(Key::ModCtrl, down),
        WinitKey::Named(NamedKey::Alt) => io.add_key_event(Key::ModAlt, down),
        WinitKey::Named(NamedKey::Super) => io.add_key_event(Key::ModSuper, down),
        _ => {}
    }
}

fn to_imgui_key(key: WinitKey, location: KeyLocation) -> Option<Key> {
    use KeyLocation::{Left, Numpad, Right, Standard};
    use NamedKey as N;
    use WinitKey::{Character as C, Named};
    Some(match (key.as_ref(), location) {
        (Named(N::Tab), _) => Key::Tab,
        (Named(N::ArrowLeft), _) => Key::LeftArrow,
        (Named(N::ArrowRight), _) => Key::RightArrow,
        (Named(N::ArrowUp), _) => Key::UpArrow,
        (Named(N::ArrowDown), _) => Key::DownArrow,
        (Named(N::PageUp), _) => Key::PageUp,
        (Named(N::PageDown), _) => Key::PageDown,
        (Named(N::Home), _) => Key::Home,
        (Named(N::End), _) => Key::End,
        (Named(N::Insert), _) => Key::Insert,
        (Named(N::Delete), _) => Key::Delete,
        (Named(N::Backspace), _) => Key::Backspace,
        (Named(N::Space), _) => Key::Space,
        (Named(N::Enter), Standard) => Key::Enter,
        (Named(N::Enter), Numpad) => Key::KeypadEnter,
        (Named(N::Escape), _) => Key::Escape,
        (Named(N::Control), Left) => Key::LeftCtrl,
        (Named(N::Control), Right) => Key::RightCtrl,
        (Named(N::Shift), Left) => Key::LeftShift,
        (Named(N::Shift), Right) => Key::RightShift,
        (Named(N::Alt), Left) => Key::LeftAlt,
        (Named(N::Alt), Right) => Key::RightAlt,
        (Named(N::Super), Left) => Key::LeftSuper,
        (Named(N::Super), Right) => Key::RightSuper,
        (Named(N::ContextMenu), _) => Key::Menu,
        (Named(N::F1), _) => Key::F1,
        (Named(N::F2), _) => Key::F2,
        (Named(N::F3), _) => Key::F3,
        (Named(N::F4), _) => Key::F4,
        (Named(N::F5), _) => Key::F5,
        (Named(N::F6), _) => Key::F6,
        (Named(N::F7), _) => Key::F7,
        (Named(N::F8), _) => Key::F8,
        (Named(N::F9), _) => Key::F9,
        (Named(N::F10), _) => Key::F10,
        (Named(N::F11), _) => Key::F11,
        (Named(N::F12), _) => Key::F12,
        (Named(N::CapsLock), _) => Key::CapsLock,
        (Named(N::ScrollLock), _) => Key::ScrollLock,
        (Named(N::NumLock), _) => Key::NumLock,
        (Named(N::PrintScreen), _) => Key::PrintScreen,
        (Named(N::Pause), _) => Key::Pause,
        (C("0"), Standard) => Key::Alpha0,
        (C("1"), Standard) => Key::Alpha1,
        (C("2"), Standard) => Key::Alpha2,
        (C("3"), Standard) => Key::Alpha3,
        (C("4"), Standard) => Key::Alpha4,
        (C("5"), Standard) => Key::Alpha5,
        (C("6"), Standard) => Key::Alpha6,
        (C("7"), Standard) => Key::Alpha7,
        (C("8"), Standard) => Key::Alpha8,
        (C("9"), Standard) => Key::Alpha9,
        (C("0"), Numpad) => Key::Keypad0,
        (C("1"), Numpad) => Key::Keypad1,
        (C("2"), Numpad) => Key::Keypad2,
        (C("3"), Numpad) => Key::Keypad3,
        (C("4"), Numpad) => Key::Keypad4,
        (C("5"), Numpad) => Key::Keypad5,
        (C("6"), Numpad) => Key::Keypad6,
        (C("7"), Numpad) => Key::Keypad7,
        (C("8"), Numpad) => Key::Keypad8,
        (C("9"), Numpad) => Key::Keypad9,
        (C("a"), _) => Key::A,
        (C("b"), _) => Key::B,
        (C("c"), _) => Key::C,
        (C("d"), _) => Key::D,
        (C("e"), _) => Key::E,
        (C("f"), _) => Key::F,
        (C("g"), _) => Key::G,
        (C("h"), _) => Key::H,
        (C("i"), _) => Key::I,
        (C("j"), _) => Key::J,
        (C("k"), _) => Key::K,
        (C("l"), _) => Key::L,
        (C("m"), _) => Key::M,
        (C("n"), _) => Key::N,
        (C("o"), _) => Key::O,
        (C("p"), _) => Key::P,
        (C("q"), _) => Key::Q,
        (C("r"), _) => Key::R,
        (C("s"), _) => Key::S,
        (C("t"), _) => Key::T,
        (C("u"), _) => Key::U,
        (C("v"), _) => Key::V,
        (C("w"), _) => Key::W,
        (C("x"), _) => Key::X,
        (C("y"), _) => Key::Y,
        (C("z"), _) => Key::Z,
        (C("'"), _) => Key::Apostrophe,
        (C(","), Standard) => Key::Comma,
        (C("-"), Standard) => Key::Minus,
        (C("-"), Numpad) => Key::KeypadSubtract,
        (C("."), Standard) => Key::Period,
        (C("."), Numpad) => Key::KeypadDecimal,
        (C("/"), Standard) => Key::Slash,
        (C("/"), Numpad) => Key::KeypadDivide,
        (C(";"), _) => Key::Semicolon,
        (C("="), Standard) => Key::Equal,
        (C("="), Numpad) => Key::KeypadEqual,
        (C("["), _) => Key::LeftBracket,
        (C("\\"), _) => Key::Backslash,
        (C("]"), _) => Key::RightBracket,
        (C("`"), _) => Key::GraveAccent,
        (C("*"), Numpad) => Key::KeypadMultiply,
        (C("+"), Numpad) => Key::KeypadAdd,
        _ => return None,
    })
}

fn to_imgui_mouse_button(button: winit::event::MouseButton) -> Option<MouseButton> {
    use winit::event::MouseButton as B;
    match button {
        B::Left | B::Other(0) => Some(MouseButton::Left),
        B::Right | B::Other(1) => Some(MouseButton::Right),
        B::Middle | B::Other(2) => Some(MouseButton::Middle),
        B::Back | B::Other(3) => Some(MouseButton::Extra1),
        B::Forward | B::Other(4) => Some(MouseButton::Extra2),
        _ => None,
    }
}
