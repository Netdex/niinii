#[cfg(windows)]
use niinii::backend::d3d11::D3D11Renderer;
use niinii::{
    app::App,
    backend::{glow::GlowRenderer, renderer::Renderer},
    view::settings::{SettingsView, SupportedRenderer},
};

const STATE_PATH: &str = "niinii.toml";

fn main() -> std::io::Result<()> {
    env_logger::init();

    let settings: SettingsView = std::fs::read_to_string(STATE_PATH)
        .ok()
        .and_then(|x| toml::from_str(&x).ok())
        .unwrap_or_default();

    let mut app = App::new(settings);
    let mut renderer: Box<dyn Renderer> = match app.settings().active_renderer() {
        SupportedRenderer::Glow => Box::new(GlowRenderer::new(app.settings())),
        #[cfg(windows)]
        SupportedRenderer::Direct3D11 => Box::new(D3D11Renderer::new(app.settings())),
    };
    renderer.main_loop(&mut app);

    std::fs::write(STATE_PATH, toml::to_string(&app.settings()).unwrap())?;

    Ok(())
}
