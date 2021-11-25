use niinii::{
    app::App,
    backend::{d3d11::D3D11Renderer, renderer::Renderer},
    view::settings::{SettingsView, SupportedRenderer},
};
use std::{
    fs::File,
    io::{BufReader, BufWriter},
};
use winit::platform::windows::WindowBuilderExtWindows;
use winit::window;

const STATE_PATH: &'static str = "niinii.json";

fn main() {
    env_logger::init();

    let settings: SettingsView = File::open(STATE_PATH)
        .ok()
        .map(BufReader::new)
        .and_then(|x| serde_json::from_reader(x).ok())
        .unwrap_or_default();

    let active_renderer = settings.active_renderer();

    let mut app = App::new(settings);
    let mut renderer = D3D11Renderer::new(app.settings());
    renderer.main_loop(&mut app);

    let writer = BufWriter::new(File::create(STATE_PATH).unwrap());
    serde_json::to_writer(writer, &app.settings()).unwrap();
}
