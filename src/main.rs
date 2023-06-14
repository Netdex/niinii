// Before settings this, figure out how to stop spawned processes from making
// their own consoles
// #![windows_subsystem = "windows"]

#[cfg(windows)]
use libniinii::renderer::d3d11::D3D11Renderer;
use libniinii::{
    app::App,
    renderer::{glow_viewports::GlowRenderer, Renderer},
    settings::{RendererType, Settings},
};

fn main() -> std::io::Result<()> {
    // env_logger::init();
    let rust_log_style = std::env::var("RUST_LOG_STYLE").unwrap_or("auto".into());
    let with_ansi = match rust_log_style.as_str() {
        "auto" | "always" => true,
        "never" => false,
        _ => true,
    };
    let subscriber = tracing_subscriber::fmt::fmt()
        // .compact()
        .with_ansi(with_ansi)
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
    tracing::info!("initializing renderer {:?}", app.settings().renderer_type());
    let mut renderer: Box<dyn Renderer> = match app.settings().renderer_type() {
        RendererType::Glow => Box::new(GlowRenderer::new(app.settings())),
        #[cfg(windows)]
        RendererType::Direct3D11 => Box::new(D3D11Renderer::new(app.settings())),
    };
    renderer.main_loop(&mut app);

    app.settings().write_to_file()?;

    Ok(())
}
