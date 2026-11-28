// Before settings this, figure out how to stop spawned processes from making
// their own consoles
// #![windows_subsystem = "windows"]

use niinii::{app::App, gui, settings::Settings};
use tracing_subscriber::fmt::format::FmtSpan;

#[cfg(windows)]
fn enable_ansi_support() {
    let win32_ansi = nu_ansi_term::enable_ansi_support();
    if let Err(err) = win32_ansi {
        eprintln!("Failed to enable Windows ANSI support: {:?}", err);
    }
}
fn main() -> Result<(), gui::Error> {
    #[cfg(windows)]
    enable_ansi_support();

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
        let tracy_layer = tracing_tracy::TracyLayer::new(tracing_tracy::DefaultConfig::default());
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

    let options = gui::Options {
        title: "niinii".into(),
        ini_path: "imgui.ini".into(),
        theme: app.settings().theme.clone(),
    };
    let result = gui::run(options, |ui, frame| app.ui(ui, frame));

    app.settings().write_to_file()?;
    result
}
