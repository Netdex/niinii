#[cfg(windows)]
use niinii::backend::d3d11::D3D11Renderer;
use niinii::{
    app::App,
    backend::{glow::GlowRenderer, renderer::Renderer},
    view::settings::{SettingsView, SupportedRenderer},
};
use std::{
    fs::File,
    io::{BufReader, BufWriter},
};

const STATE_PATH: &str = "niinii.json";

fn main() {
    env_logger::init();

    let settings: SettingsView = File::open(STATE_PATH)
        .ok()
        .map(BufReader::new)
        .and_then(|x| serde_json::from_reader(x).ok())
        .unwrap_or_default();

    let mut app = App::new(settings);
    let mut renderer: Box<dyn Renderer> = match app.settings().active_renderer() {
        SupportedRenderer::Glow => Box::new(GlowRenderer::new(app.settings())),
        #[cfg(windows)]
        SupportedRenderer::Direct3D11 => Box::new(D3D11Renderer::new(app.settings())),
    };
    renderer.main_loop(&mut app);

    let writer = BufWriter::new(File::create(STATE_PATH).unwrap());
    serde_json::to_writer(writer, &app.settings()).unwrap();
}
