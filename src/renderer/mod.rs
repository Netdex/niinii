use std::path::PathBuf;

use imgui_winit_support::WinitPlatform;
use winit::window;

use crate::support::clipboard;
use crate::{app::App, settings::Settings};

pub mod context;
#[cfg(windows)]
pub mod d3d11;
pub mod glow_viewports;
mod ranges;

pub trait Renderer {
    fn main_loop(&mut self, _app: &mut App) {}

    fn create_window_builder(settings: &Settings) -> window::WindowBuilder
    where
        Self: Sized,
    {
        // let on_top = settings.on_top || settings.overlay_mode;
        let maximized = settings.overlay_mode;
        let decorations = !settings.overlay_mode;
        let fullscreen = if settings.overlay_mode {
            Some(window::Fullscreen::Borderless(None))
        } else {
            None
        };

        window::WindowBuilder::new()
            .with_title("niinii")
            // .with_inner_size(glutin::dpi::LogicalSize::new(768, 768))
            .with_transparent(true)
            // .with_drag_and_drop(false)
            .with_maximized(maximized)
            .with_decorations(decorations)
            // .with_always_on_top(on_top) // can't set this here?
            .with_fullscreen(fullscreen)
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
        io.config_flags |= imgui::ConfigFlags::DOCKING_ENABLE;

        if let Some(backend) = clipboard::init() {
            imgui.set_clipboard_backend(backend);
        } else {
            panic!("failed to initialize clipboard");
        }
    }

    fn create_platform(
        imgui: &mut imgui::Context,
        window: &winit::window::Window,
        dpi: Option<f64>,
    ) -> WinitPlatform
    where
        Self: Sized,
    {
        let dpi_mode = match dpi {
            Some(dpi) => imgui_winit_support::HiDpiMode::Locked(dpi),
            None => imgui_winit_support::HiDpiMode::Default,
        };
        let mut platform = WinitPlatform::init(imgui);
        platform.attach_window(imgui.io_mut(), window, dpi_mode);
        platform
    }
}
