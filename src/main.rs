use libniinii::{app::App, view::SettingsView};
use std::{
    fs::File,
    io::{BufReader, BufWriter},
    mem,
};
use winit::window;

mod job;

const TITLE: &'static str = "niinii";
const STATE_PATH: &'static str = "niinii.json";

fn create_window(transparent: bool, on_top: bool) -> window::WindowBuilder {
    let window = window::WindowBuilder::new()
        .with_title(TITLE)
        .with_inner_size(glutin::dpi::LogicalSize::new(768, 768))
        .with_transparent(transparent)
        // .with_decorations(false)
        .with_always_on_top(on_top);
    window
}

fn main() {
    env_logger::init();
    let _job_object = job::setup();
    mem::forget(_job_object);

    let state: SettingsView = File::open(STATE_PATH)
        .ok()
        .map(BufReader::new)
        .and_then(|x| serde_json::from_reader(x).ok())
        .unwrap_or_default();

    let window = create_window(state.transparent, state.on_top);

    let mut app = App::new(state);
    app.start_pg_daemon();
    libniinii::backend::d3d11::main_loop(window, |_opened, env, ui| {
        app.ui(env, ui);
    });

    let writer = BufWriter::new(File::create(STATE_PATH).unwrap());
    serde_json::to_writer(writer, &app.settings()).unwrap();
}
