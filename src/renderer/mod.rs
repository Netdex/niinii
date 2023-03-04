use std::path::PathBuf;

use glutin::window;
use imgui_winit_support::WinitPlatform;

use crate::clipboard;
use crate::{app::App, settings::Settings};

pub mod context;
#[cfg(windows)]
pub mod d3d11;
pub mod glow;

pub trait Renderer {
    fn main_loop(&mut self, _app: &mut App) {}

    fn create_window_builder(settings: &Settings) -> window::WindowBuilder
    where
        Self: Sized,
    {
        let on_top = settings.on_top || settings.overlay_mode;
        let maximized = settings.overlay_mode;
        let decorations = !settings.overlay_mode;

        window::WindowBuilder::new()
            .with_title("niinii")
            // .with_inner_size(glutin::dpi::LogicalSize::new(768, 768))
            .with_transparent(true)
            // .with_drag_and_drop(false)
            .with_maximized(maximized)
            .with_decorations(decorations)
            .with_always_on_top(on_top)
    }

    fn configure_imgui(imgui: &mut imgui::Context, settings: &Settings)
    where
        Self: Sized,
    {
        imgui.set_ini_filename(Some(PathBuf::from("imgui.ini")));

        if let Some(style) = settings.style() {
            *imgui.style_mut() = style;
        }

        let io = imgui.io_mut();
        io.font_allow_user_scaling = true;

        if let Some(backend) = clipboard::init() {
            imgui.set_clipboard_backend(backend);
        } else {
            panic!("failed to initialize clipboard");
        }
    }

    fn create_platform(imgui: &mut imgui::Context, window: &winit::window::Window) -> WinitPlatform
    where
        Self: Sized,
    {
        let mut platform = WinitPlatform::init(imgui);
        platform.attach_window(
            imgui.io_mut(),
            window,
            imgui_winit_support::HiDpiMode::Default,
        );
        platform
    }
}
