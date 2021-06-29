use std::{
    borrow::Borrow,
    cell::RefCell,
    fs::File,
    io::{BufReader, BufWriter},
    rc::Rc,
};

use app::App;
use support::View;

mod app;
mod config;
mod rikai;
mod support;

fn main() {
    const STATE_PATH: &str = "niinii.json";

    let mut app: App = File::open(STATE_PATH)
        .ok()
        .map(|x| BufReader::new(x))
        .and_then(|x| serde_json::from_reader(x).ok())
        .unwrap_or_default();

    let system = support::init("niinii");
    system.main_loop(|_opened, env, ui| {
        app.ui(env, ui);
    });

    let writer = BufWriter::new(File::create(STATE_PATH).unwrap());
    serde_json::to_writer(writer, &app).unwrap();
}
