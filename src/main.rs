// Before settings this, figure out how to stop spawned processes from making
// their own consoles
// #![windows_subsystem = "windows"]

#[cfg(windows)]
use libniinii::renderer::d3d11::D3D11Renderer;
use libniinii::{
    app::App,
    renderer::{glow::GlowRenderer, Renderer},
    settings::{RendererType, Settings},
};

fn main() -> std::io::Result<()> {
    // env_logger::init();
    let subscriber = tracing_subscriber::fmt::fmt()
        // .compact()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .finish();
    #[cfg(feature = "tracy")]
    let subscriber = {
        use tracing_subscriber::prelude::*;
        let tracy_layer = tracing_tracy::TracyLayer::new();
        subscriber.with(tracy_layer)
    };
    tracing::subscriber::set_global_default(subscriber).unwrap();

    let settings = Settings::from_file();

    let mut app = App::new(settings);
    let mut renderer: Box<dyn Renderer> = match app.settings().renderer_type() {
        RendererType::Glow => Box::new(GlowRenderer::new(app.settings())),
        #[cfg(windows)]
        RendererType::Direct3D11 => Box::new(D3D11Renderer::new(app.settings())),
    };
    renderer.main_loop(&mut app);

    app.settings().write_to_file()?;

    Ok(())
}
