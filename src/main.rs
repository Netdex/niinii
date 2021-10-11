use glutin::platform::windows::WindowBuilderExtWindows;
use libniinii::{
    app::App,
    view::settings::{RendererType, SettingsView},
};
use std::{
    fs::File,
    io::{BufReader, BufWriter},
};
use winit::window;

const TITLE: &'static str = "niinii";
const STATE_PATH: &'static str = "niinii.json";

fn create_window(transparent: bool, on_top: bool) -> window::WindowBuilder {
    let window = window::WindowBuilder::new()
        .with_title(TITLE)
        .with_inner_size(glutin::dpi::LogicalSize::new(768, 768))
        .with_transparent(transparent)
        .with_drag_and_drop(false)
        // .with_decorations(false)
        .with_always_on_top(on_top);
    window
}

fn main() {
    env_logger::init();

    let state: SettingsView = File::open(STATE_PATH)
        .ok()
        .map(BufReader::new)
        .and_then(|x| serde_json::from_reader(x).ok())
        .unwrap_or_default();

    let renderer = state.renderer();
    let window = create_window(state.transparent, state.on_top);

    let mut app = App::new(state);

    match renderer {
        RendererType::Glow => {
            libniinii::backend::glow::main_loop(window, |_opened, env, ui| {
                app.ui(env, ui);
            });
        }
        RendererType::Direct3D11 => {
            libniinii::backend::d3d11::main_loop(window, |_opened, env, ui| {
                app.ui(env, ui);
            });
        }
    }

    let writer = BufWriter::new(File::create(STATE_PATH).unwrap());
    serde_json::to_writer(writer, &app.settings()).unwrap();
}
