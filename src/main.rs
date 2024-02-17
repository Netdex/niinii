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
use tracing_subscriber::fmt::format::FmtSpan;

fn main() -> std::io::Result<()> {
    let with_ansi = std::env::var("RUST_LOG_STYLE")
        .map(|val| val != "never")
        .unwrap_or(true);
    let subscriber = tracing_subscriber::fmt::fmt()
        .with_ansi(with_ansi)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_span_events(FmtSpan::CLOSE)
        .finish();
    #[cfg(feature = "tracing-tracy")]
    let subscriber = {
        use tracing_subscriber::prelude::*;
        let tracy_layer = tracing_tracy::TracyLayer::new();
        subscriber.with(tracy_layer)
    };
    #[cfg(feature = "tracing-chrome")]
    let (subscriber, _guard) = {
        use tracing_subscriber::prelude::*;
        let (chrome_layer, _guard) = tracing_chrome::ChromeLayerBuilder::new().build();
        (subscriber.with(chrome_layer), _guard)
    };
    tracing::subscriber::set_global_default(subscriber).unwrap();

    let settings = Settings::from_file();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let _runtime_guard = runtime.enter();

    let mut app = runtime.block_on(App::new(settings));

    tracing::info!(renderer=?app.settings().renderer_type);
    let mut renderer: Box<dyn Renderer> = match app.settings().renderer_type {
        RendererType::Glow => Box::new(GlowRenderer::new(app.settings())),
        #[cfg(windows)]
        RendererType::Direct3D11 => Box::new(D3D11Renderer::new(app.settings())),
    };

    renderer.main_loop(&mut app);

    app.settings().write_to_file()?;

    Ok(())
}
