use std::{
    fs::File,
    io::{BufReader, BufWriter},
};

use app::App;

mod app;
mod support;
mod view;

fn main() {
    const STATE_PATH: &str = "niinii.json";

    let mut app: App = File::open(STATE_PATH)
        .ok()
        .map(|x| BufReader::new(x))
        .and_then(|x| serde_json::from_reader(x).ok())
        .unwrap_or_default();

    support::main_loop("niinii", |_opened, env, ui| {
        app.ui(env, ui);
    });

    let writer = BufWriter::new(File::create(STATE_PATH).unwrap());
    serde_json::to_writer(writer, &app).unwrap();
}
