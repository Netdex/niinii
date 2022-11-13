#[cfg(windows)]
use libniinii::backend::d3d11::D3D11Renderer;
use libniinii::{
    app::App,
    backend::{glow::GlowRenderer, renderer::Renderer},
    view::settings::{Settings, SupportedRenderer},
};

fn main() -> std::io::Result<()> {
    env_logger::init();

    let settings = Settings::from_file();

    let mut app = App::new(settings);
    let mut renderer: Box<dyn Renderer> = match app.settings().active_renderer() {
        SupportedRenderer::Glow => Box::new(GlowRenderer::new(app.settings())),
        #[cfg(windows)]
        SupportedRenderer::Direct3D11 => Box::new(D3D11Renderer::new(app.settings())),
    };
    renderer.main_loop(&mut app);

    app.settings().write_to_file()?;

    Ok(())
}
