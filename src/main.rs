use glutin::{platform::windows::WindowBuilderExtWindows, window::Fullscreen};
use libniinii::{
    app::App,
    view::settings::{SettingsView, SupportedRenderer},
};
use std::{
    fs::File,
    io::{BufReader, BufWriter},
};
use winit::window;

const TITLE: &'static str = "niinii";
const STATE_PATH: &'static str = "niinii.json";

fn create_window(settings: &SettingsView) -> window::WindowBuilder {
    let transparent = settings.transparent || settings.overlay_mode;
    let on_top = settings.on_top || settings.overlay_mode;
    let maximized = settings.overlay_mode;
    let decorations = !settings.overlay_mode;

    let window = window::WindowBuilder::new()
        .with_title(TITLE)
        .with_inner_size(glutin::dpi::LogicalSize::new(768, 768))
        .with_transparent(transparent)
        .with_drag_and_drop(false)
        .with_maximized(maximized)
        .with_decorations(decorations)
        .with_always_on_top(on_top);
    window
}

fn main() {
    env_logger::init();

    let settings: SettingsView = File::open(STATE_PATH)
        .ok()
        .map(BufReader::new)
        .and_then(|x| serde_json::from_reader(x).ok())
        .unwrap_or_default();

    let renderer = settings.active_renderer();
    let window = create_window(&settings);

    let mut app = App::new(settings);

    match renderer {
        SupportedRenderer::Glow => {
            libniinii::backend::glow::main_loop(window, &mut app);
        }
        SupportedRenderer::Direct3D11 => {
            libniinii::backend::d3d11::main_loop(window, &mut app);
        }
    }

    let writer = BufWriter::new(File::create(STATE_PATH).unwrap());
    serde_json::to_writer(writer, &app.settings()).unwrap();
}
