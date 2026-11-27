#![cfg(windows)]
//! Combined winit platform backend + Direct3D 11 renderer for imgui-rs with
//! multi-viewport support.
//!
//! The application owns the main window, its swapchain and render target; this
//! crate owns every secondary viewport window and its swapchain. Per frame:
//!
//! 1. [`Renderer::handle_event`] for every winit event.
//! 2. Build the UI, then `ui.end_frame_early()`.
//! 3. [`Renderer::prepare_render`], `imgui.update_platform_windows()`,
//!    [`Renderer::update_viewports`].
//! 4. `imgui.render()`, bind and clear the main render target, then
//!    [`Renderer::render`] and present the main swapchain.
//! 5. [`Renderer::render_viewports`].
//!
//! Rendering does not save or restore D3D11 pipeline state, and
//! [`Renderer::render_viewports`] leaves a viewport's render target bound, so
//! rebind the main render target at the start of every frame.

mod d3d11;
mod desktop;
mod platform;

use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    ptr,
    rc::Rc,
};

use desktop::Desktop;
use imgui::{BackendFlags, Id, TextureId, Textures, ViewportFlags};
use platform::ViewportData;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use thiserror::Error;
use winapi::{
    shared::{
        dxgi::{IDXGISwapChain, DXGI_SWAP_CHAIN_DESC, DXGI_SWAP_EFFECT_DISCARD},
        dxgiformat::{DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_FORMAT_UNKNOWN},
        dxgitype::{
            DXGI_MODE_DESC, DXGI_RATIONAL, DXGI_SAMPLE_DESC, DXGI_USAGE_RENDER_TARGET_OUTPUT,
        },
        minwindef::{BOOL, TRUE},
        windef::HWND,
    },
    um::{
        d3d11::{ID3D11RenderTargetView, ID3D11Resource, ID3D11ShaderResourceView},
        dwmapi::{DwmSetWindowAttribute, DWMWA_TRANSITIONS_FORCEDISABLED},
        winuser::{
            GetCursorPos, GetWindowLongW, SetLayeredWindowAttributes, SetWindowLongW, SetWindowPos,
            GWL_EXSTYLE, HWND_TOPMOST, LWA_ALPHA, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
            WS_EX_LAYERED, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
        },
    },
    Interface,
};
use winit::{
    dpi::{LogicalSize, PhysicalPosition, PhysicalSize},
    event_loop::EventLoopWindowTarget,
    window::{CursorIcon, Window, WindowBuilder, WindowLevel},
};
use wio::com::ComPtr;

pub use winapi::um::d3d11::ID3D11Device;
/// Re-exported so callers use the same `ComPtr` type this crate expects.
pub use wio;

use crate::{
    d3d11::{check, com_ptr, DeviceObjects},
    platform::{PlatformBackend, RendererBackend, ViewportEvent},
};

/// `DXGI_MWA_NO_ALT_ENTER`; not exported by winapi.
const DXGI_MWA_NO_ALT_ENTER: u32 = 1 << 1;

#[derive(Debug, Error)]
pub enum RendererError {
    #[error("{context} failed: HRESULT {hr:#010x}")]
    Hresult { context: &'static str, hr: i32 },
    #[error("draw data references unregistered texture {0:?}")]
    BadTexture(TextureId),
    #[error("failed to create viewport window: {0}")]
    WindowCreationFailed(#[from] winit::error::OsError),
    #[error("viewport window has no Win32 handle")]
    NoWin32Handle,
}

pub type WindowBuilderCallback = dyn Fn(&imgui::Viewport) -> WindowBuilder;

/// A secondary viewport window. Field order matters: the render target and
/// swapchain must be released before the window they present to is destroyed.
struct ViewportWindow {
    rtv: Option<ComPtr<ID3D11RenderTargetView>>,
    swapchain: ComPtr<IDXGISwapChain>,
    size: PhysicalSize<u32>,
    window: Window,
}

pub struct Renderer {
    objects: DeviceObjects,
    window_builder: Option<Box<WindowBuilderCallback>>,
    extra_windows: HashMap<Id, ViewportWindow>,
    event_queue: Rc<RefCell<VecDeque<ViewportEvent>>>,
    desktop: Rc<Desktop>,
    last_cursor: CursorIcon,
    clear_color: [f32; 4],
}

impl Renderer {
    /// Creates the renderer for `device` and installs the platform and renderer
    /// viewport backends on `imgui`. `main_window` must be the window the
    /// application renders the main viewport into.
    ///
    /// `window_builder`, if set, supplies the [`WindowBuilder`] for each new
    /// viewport window; visibility, position and size are always overridden.
    pub fn new(
        imgui: &mut imgui::Context,
        main_window: &Window,
        device: ComPtr<ID3D11Device>,
        window_builder: Option<Box<WindowBuilderCallback>>,
    ) -> Result<Self, RendererError> {
        let objects = unsafe { DeviceObjects::new(device, imgui.fonts())? };
        let event_queue = Rc::new(RefCell::new(Default::default()));
        let desktop = Rc::new(Desktop::new(
            main_window
                .available_monitors()
                .map(|m| (m.position(), m.size(), m.scale_factor() as f32)),
        ));
        platform::init(imgui, main_window, &desktop);
        imgui
            .io_mut()
            .backend_flags
            .insert(BackendFlags::RENDERER_HAS_VTX_OFFSET);
        imgui.set_platform_backend(PlatformBackend {
            event_queue: event_queue.clone(),
            desktop: desktop.clone(),
        });
        imgui.set_renderer_backend(RendererBackend);
        // imgui-rs has no hook for this callback. Without it imgui derives each
        // viewport's DPI from monitor overlap on its own, fighting ours.
        unsafe {
            (*imgui::sys::igGetPlatformIO()).Platform_GetWindowDpiScale =
                Some(platform::get_window_dpi_scale);
        }
        imgui.set_renderer_name(Some(format!(
            "imgui-winit-dx11-renderer-viewports {}",
            env!("CARGO_PKG_VERSION")
        )));

        Ok(Self {
            objects,
            window_builder,
            extra_windows: HashMap::new(),
            event_queue,
            desktop,
            last_cursor: CursorIcon::Default,
            clear_color: [0.0; 4],
        })
    }

    /// Color secondary viewports are cleared to before drawing, unless the
    /// viewport has [`ViewportFlags::NO_RENDERER_CLEAR`]. Defaults to fully
    /// transparent.
    pub fn set_clear_color(&mut self, color: [f32; 4]) {
        self.clear_color = color;
    }

    /// The textures registry. The slot at `!0` is reserved for the font atlas.
    pub fn textures(&self) -> &Textures<ComPtr<ID3D11ShaderResourceView>> {
        &self.objects.textures
    }

    pub fn textures_mut(&mut self) -> &mut Textures<ComPtr<ID3D11ShaderResourceView>> {
        &mut self.objects.textures
    }

    /// Rebuilds the font atlas texture, e.g. after fonts were added or the DPI
    /// changed. Must be called between frames.
    pub fn rebuild_font_texture(
        &mut self,
        fonts: &mut imgui::FontAtlas,
    ) -> Result<(), RendererError> {
        unsafe { self.objects.rebuild_font_texture(fonts) }
    }

    pub fn handle_event<T>(
        &mut self,
        imgui: &mut imgui::Context,
        main_window: &Window,
        event: &winit::event::Event<T>,
    ) {
        let winit::event::Event::WindowEvent { window_id, event } = event else {
            return;
        };
        let (window, viewport_id) = if *window_id == main_window.id() {
            (main_window, None)
        } else if let Some((id, vw)) = self
            .extra_windows
            .iter()
            .find(|(_, vw)| vw.window.id() == *window_id)
        {
            (&vw.window, Some(*id))
        } else {
            return;
        };
        if let winit::event::WindowEvent::CursorMoved { position, .. } = event {
            if imgui
                .io()
                .config_flags
                .contains(imgui::ConfigFlags::VIEWPORTS_ENABLE)
            {
                if let Ok(origin) = window.inner_position() {
                    let cursor = PhysicalPosition::new(
                        origin.x + position.x as i32,
                        origin.y + position.y as i32,
                    );
                    let monitor = self.monitor_under(imgui, cursor);
                    let pos = self.desktop.to_logical(monitor, cursor);
                    imgui.io_mut().add_mouse_pos_event(pos);
                }
                return;
            }
        }
        platform::handle_window_event(imgui, window, viewport_id, event, &self.desktop);
    }

    /// Monitor mapping for converting the desktop point `p` into imgui's
    /// coordinates: the one used by the window `p` is over, not by the window
    /// that reported it (they differ while another window holds the mouse
    /// capture, e.g. during a drag).
    fn monitor_under(&self, imgui: &mut imgui::Context, p: PhysicalPosition<i32>) -> usize {
        let moving = moving_viewport();
        let mut hit = None;
        for (id, vw) in &self.extra_windows {
            let Some(viewport) = imgui.viewport_by_id_mut(*id) else {
                continue;
            };
            if window_contains(&vw.window, p) {
                let is_moving = ptr::eq(viewport, moving);
                hit = Some(unsafe { platform_data(viewport) }.monitor);
                if is_moving {
                    break;
                }
            }
        }
        hit.unwrap_or_else(|| self.desktop.monitor_at(p))
    }

    /// Applies the window operations imgui requested during
    /// `update_platform_windows`, creating and destroying viewport windows.
    pub fn update_viewports<T>(
        &mut self,
        imgui: &mut imgui::Context,
        window_target: &EventLoopWindowTarget<T>,
    ) -> Result<(), RendererError> {
        loop {
            let Some(event) = self.event_queue.borrow_mut().pop_front() else {
                break;
            };
            match event {
                ViewportEvent::Create(id) => {
                    if let Some(viewport) = imgui.viewport_by_id_mut(id) {
                        let vw = self.create_viewport_window(viewport, window_target)?;
                        self.extra_windows.insert(id, vw);
                    }
                }
                ViewportEvent::Destroy(id) => {
                    self.extra_windows.remove(&id);
                }
                ViewportEvent::SetPos(id, pos) => {
                    if let (Some(viewport), Some(vw)) =
                        (imgui.viewport_by_id_mut(id), self.extra_windows.get(&id))
                    {
                        let monitor = unsafe { platform_data(viewport) }.monitor;
                        vw.window
                            .set_outer_position(self.desktop.to_physical(monitor, pos));
                    }
                }
                ViewportEvent::SetSize(id, size) => {
                    if let (Some(viewport), Some(vw)) =
                        (imgui.viewport_by_id_mut(id), self.extra_windows.get(&id))
                    {
                        unsafe { platform_data(viewport) }.size = size;
                        let size = LogicalSize::new(size[0], size[1])
                            .to_physical::<f32>(viewport.dpi_scale.into());
                        let _ = vw.window.request_inner_size(size);
                    }
                }
                ViewportEvent::SetVisible(id) => {
                    if let Some(vw) = self.extra_windows.get(&id) {
                        vw.window.set_visible(true);
                    }
                }
                ViewportEvent::SetFocus(id) => {
                    if let Some(vw) = self.extra_windows.get(&id) {
                        vw.window.focus_window();
                    }
                }
                ViewportEvent::SetTitle(id, title) => {
                    if let Some(vw) = self.extra_windows.get(&id) {
                        vw.window.set_title(&title);
                    }
                }
                ViewportEvent::SetNoInputs(id, no_inputs) => {
                    if let Some(vw) = self.extra_windows.get(&id) {
                        unsafe { set_click_through(hwnd(&vw.window)?, no_inputs) };
                    }
                }
            }
        }
        self.update_dpi(imgui);
        Ok(())
    }

    /// Picks each secondary viewport's scale from a point a resize cannot move:
    /// the cursor while imgui drags the window, otherwise its top-left corner.
    /// Picking by the window's area (as Windows does) feeds back through the
    /// resize and oscillates at monitor boundaries.
    fn update_dpi(&mut self, imgui: &mut imgui::Context) {
        let moving = moving_viewport();
        for (id, vw) in &self.extra_windows {
            let Some(viewport) = imgui.viewport_by_id_mut(*id) else {
                continue;
            };
            let Ok(inner_pos) = vw.window.inner_position() else {
                continue;
            };
            let anchor = if ptr::eq(viewport, moving) {
                cursor_pos().unwrap_or(inner_pos)
            } else {
                PhysicalPosition::new(inner_pos.x + 1, inner_pos.y + 1)
            };
            let monitor = self.desktop.monitor_at(anchor);
            let dpi = self.desktop.scale(monitor);
            let data = unsafe { &mut *viewport.platform_user_data.cast::<ViewportData>() };
            if monitor != data.monitor {
                data.monitor = monitor;
                data.dpi = dpi;
                data.pos = self.desktop.to_logical(monitor, inner_pos);
                viewport.dpi_scale = dpi;
                viewport.platform_request_move = true;
            }
            // Undo any resize Windows applied for its own DPI choice. winit applies
            // those itself when WM_DPICHANGED arrives while our event handler is
            // running (e.g. from our own SetWindowPos), so they cannot be vetoed.
            if viewport.flags.contains(ViewportFlags::NO_DECORATION) && !data.minimized {
                let want = PhysicalSize::new(
                    (data.size[0] * dpi).round() as u32,
                    (data.size[1] * dpi).round() as u32,
                );
                if vw.window.inner_size() != want {
                    let _ = vw.window.request_inner_size(want);
                }
            }
        }
    }

    /// Raises every topmost viewport window to the top of the topmost band.
    /// Windows that make themselves topmost later (e.g. borderless-fullscreen
    /// games) otherwise cover them; call this periodically to stay on top.
    pub fn refresh_topmost(&self) -> Result<(), RendererError> {
        for vw in self.extra_windows.values() {
            let hwnd = hwnd(&vw.window)?;
            unsafe {
                if GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST != 0 {
                    SetWindowPos(
                        hwnd,
                        HWND_TOPMOST,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                    );
                }
            }
        }
        Ok(())
    }

    /// Whether `pos` (physical desktop coordinates) is inside a secondary
    /// viewport window.
    pub fn viewport_window_contains(&self, pos: PhysicalPosition<i32>) -> bool {
        self.extra_windows
            .values()
            .any(|vw| window_contains(&vw.window, pos))
    }

    /// Applies imgui's requested mouse cursor to all windows.
    pub fn prepare_render(&mut self, imgui: &mut imgui::Context, main_window: &Window) {
        let Some(cursor) = imgui.mouse_cursor() else {
            return;
        };
        let cursor = platform::to_winit_cursor(cursor);
        if self.last_cursor != cursor {
            main_window.set_cursor_icon(cursor);
            for vw in self.extra_windows.values() {
                vw.window.set_cursor_icon(cursor);
            }
            self.last_cursor = cursor;
        }
    }

    /// Draws the main viewport into the currently bound render target.
    pub fn render(&mut self, draw_data: &imgui::DrawData) -> Result<(), RendererError> {
        unsafe { self.objects.render(draw_data, draw_data.framebuffer_scale) }
    }

    /// Draws and presents every secondary viewport window.
    pub fn render_viewports(&mut self, imgui: &mut imgui::Context) -> Result<(), RendererError> {
        for (id, vw) in &mut self.extra_windows {
            let Some(viewport) = imgui.viewport_by_id(*id) else {
                continue;
            };
            if viewport.flags.contains(ViewportFlags::MINIMIZED) {
                continue;
            }
            let size = vw.window.inner_size();
            if size.width == 0 || size.height == 0 {
                continue;
            }
            unsafe {
                if size != vw.size || vw.rtv.is_none() {
                    vw.rtv = None;
                    check(
                        vw.swapchain.ResizeBuffers(
                            0,
                            size.width,
                            size.height,
                            DXGI_FORMAT_UNKNOWN,
                            0,
                        ),
                        "IDXGISwapChain::ResizeBuffers",
                    )?;
                    vw.rtv = Some(create_render_target(&self.objects, &vw.swapchain)?);
                    vw.size = size;
                }
                let rtv = vw.rtv.as_ref().unwrap().as_raw();
                let ctx = &self.objects.context;
                ctx.OMSetRenderTargets(1, &rtv, ptr::null_mut());
                if !viewport.flags.contains(ViewportFlags::NO_RENDERER_CLEAR) {
                    ctx.ClearRenderTargetView(rtv, &self.clear_color);
                }
                let scale = [viewport.dpi_scale, viewport.dpi_scale];
                self.objects.render(viewport.draw_data(), scale)?;
                // Vsync is left to the main swapchain; syncing every viewport
                // would divide the frame rate by the number of windows.
                check(vw.swapchain.Present(0, 0), "IDXGISwapChain::Present")?;
            }
        }
        Ok(())
    }

    fn create_viewport_window<T>(
        &self,
        viewport: &mut imgui::Viewport,
        window_target: &EventLoopWindowTarget<T>,
    ) -> Result<ViewportWindow, RendererError> {
        let scale = f64::from(viewport.dpi_scale);
        let monitor = unsafe { platform_data(viewport) }.monitor;
        let pos = self.desktop.to_physical(monitor, viewport.pos);
        let size = LogicalSize::new(viewport.size[0], viewport.size[1]).to_physical::<f32>(scale);

        let builder = match &self.window_builder {
            Some(callback) => callback(viewport),
            None => WindowBuilder::new()
                .with_resizable(true)
                .with_transparent(true)
                .with_decorations(!viewport.flags.contains(ViewportFlags::NO_DECORATION))
                .with_window_level(if viewport.flags.contains(ViewportFlags::TOP_MOST) {
                    WindowLevel::AlwaysOnTop
                } else {
                    WindowLevel::Normal
                }),
        };
        let builder = {
            use winit::platform::windows::WindowBuilderExtWindows;
            builder.with_skip_taskbar(viewport.flags.contains(ViewportFlags::NO_TASK_BAR_ICON))
        };
        // imgui sets this on tooltips, which must never end up behind other
        // windows, whatever level the caller's builder chose.
        let builder = if viewport.flags.contains(ViewportFlags::TOP_MOST) {
            builder.with_window_level(WindowLevel::AlwaysOnTop)
        } else {
            builder
        };
        let window = builder
            .with_visible(false)
            .with_position(pos)
            .with_inner_size(size)
            .build(window_target)?;

        let hwnd = hwnd(&window)?;
        unsafe {
            if viewport.flags.contains(ViewportFlags::NO_DECORATION) {
                // Undecorated viewports are moved and resized every frame while
                // dragging; DWM's open/close animations make that visibly lag.
                let value: BOOL = TRUE;
                DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_TRANSITIONS_FORCEDISABLED,
                    (&value as *const BOOL).cast(),
                    std::mem::size_of::<BOOL>() as u32,
                );
            }

            let inner = window.inner_size();
            let mut desc = DXGI_SWAP_CHAIN_DESC {
                BufferDesc: DXGI_MODE_DESC {
                    Width: inner.width,
                    Height: inner.height,
                    RefreshRate: DXGI_RATIONAL {
                        Numerator: 0,
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
                BufferCount: 1,
                OutputWindow: hwnd,
                Windowed: TRUE,
                // Blt model: flip-model swapchains ignore the per-pixel alpha
                // that transparent winit windows rely on.
                SwapEffect: DXGI_SWAP_EFFECT_DISCARD,
                Flags: 0,
            };
            let factory = &self.objects.factory;
            let swapchain = com_ptr::<IDXGISwapChain>("IDXGIFactory::CreateSwapChain", |out| {
                factory.CreateSwapChain(self.objects.device.as_raw().cast(), &mut desc, out)
            })?;
            check(
                factory.MakeWindowAssociation(hwnd, DXGI_MWA_NO_ALT_ENTER),
                "IDXGIFactory::MakeWindowAssociation",
            )?;
            let rtv = create_render_target(&self.objects, &swapchain)?;
            Ok(ViewportWindow {
                rtv: Some(rtv),
                swapchain,
                size: inner,
                window,
            })
        }
    }
}

/// Toggles mouse click-through. Uses `WS_EX_TRANSPARENT` on a layered window
/// with explicit opaque attributes: a window made layered without them (as
/// winit's `set_cursor_hittest` does) is not drawn at all.
unsafe fn set_click_through(hwnd: HWND, on: bool) {
    let style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
    if on && style & WS_EX_LAYERED == 0 {
        SetWindowLongW(hwnd, GWL_EXSTYLE, (style | WS_EX_LAYERED) as i32);
        SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA);
    }
    let style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
    let new = if on {
        style | WS_EX_TRANSPARENT
    } else {
        style & !WS_EX_TRANSPARENT
    };
    if new != style {
        SetWindowLongW(hwnd, GWL_EXSTYLE, new as i32);
    }
}

/// # Safety
/// `viewport` must be a secondary viewport created by `PlatformBackend`.
unsafe fn platform_data(viewport: &mut imgui::Viewport) -> &mut ViewportData {
    &mut *viewport.platform_user_data.cast::<ViewportData>()
}

/// The viewport of the window imgui is currently dragging, or null.
fn moving_viewport() -> *const imgui::Viewport {
    unsafe {
        let window = (*imgui::sys::igGetCurrentContext()).MovingWindow;
        if window.is_null() {
            ptr::null()
        } else {
            (*window).Viewport.cast_const().cast()
        }
    }
}

fn window_contains(window: &Window, p: PhysicalPosition<i32>) -> bool {
    let (Ok(origin), size) = (window.inner_position(), window.inner_size()) else {
        return false;
    };
    p.x >= origin.x
        && p.y >= origin.y
        && p.x < origin.x + size.width as i32
        && p.y < origin.y + size.height as i32
}

fn cursor_pos() -> Option<PhysicalPosition<i32>> {
    let mut p = unsafe { std::mem::zeroed() };
    (unsafe { GetCursorPos(&mut p) } != 0).then(|| PhysicalPosition::new(p.x, p.y))
}

fn hwnd(window: &Window) -> Result<HWND, RendererError> {
    match window.window_handle().map(|h| h.as_raw()) {
        Ok(RawWindowHandle::Win32(handle)) => Ok(handle.hwnd.get() as HWND),
        _ => Err(RendererError::NoWin32Handle),
    }
}

unsafe fn create_render_target(
    objects: &DeviceObjects,
    swapchain: &ComPtr<IDXGISwapChain>,
) -> Result<ComPtr<ID3D11RenderTargetView>, RendererError> {
    let back_buffer = com_ptr::<ID3D11Resource>("IDXGISwapChain::GetBuffer", |out| {
        swapchain.GetBuffer(
            0,
            &ID3D11Resource::uuidof(),
            out as *mut *mut _ as *mut *mut _,
        )
    })?;
    com_ptr("CreateRenderTargetView", |out| {
        objects
            .device
            .CreateRenderTargetView(back_buffer.as_raw(), ptr::null(), out)
    })
}
