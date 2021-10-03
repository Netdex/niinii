use std::process::Command;
use std::sync::mpsc;
use std::thread;

use ichiran::types::Root;
use ichiran::Ichiran;
use ichiran::IchiranError;
use ichiran::JmDictData;
use imgui::*;

use crate::{
    common::Env,
    view::{RikaiView, SettingsView},
};

const ERROR_MODAL_TITLE: &'static str = "Error";

#[derive(Debug)]
struct IchiranAst {
    root: Root,
    jmdict_data: JmDictData,
}

#[derive(Debug)]
enum Message {
    Ichiran(Result<IchiranAst, IchiranError>),
}

#[derive(Debug)]
enum State {
    Displaying { rikai: RikaiView },
    Error { err: IchiranError },
    Processing,
    None,
}

pub struct App {
    channel_tx: mpsc::Sender<Message>,
    channel_rx: mpsc::Receiver<Message>,

    input_text: String,
    last_clipboard: String,
    requested_text: Option<String>,

    show_imgui_demo: bool,
    show_settings: bool,
    show_raw: bool,

    settings: SettingsView,
    state: State,
}

impl App {
    pub fn new(settings: SettingsView) -> Self {
        let (channel_tx, channel_rx) = mpsc::channel();
        App {
            channel_tx,
            channel_rx,
            requested_text: None,
            input_text: "".into(),
            last_clipboard: "".into(),
            show_imgui_demo: false,
            show_settings: false,
            show_raw: false,
            settings,
            state: State::None,
        }
    }

    pub fn start_pg_daemon(&self) {
        let ichiran = Ichiran::new(&self.settings.ichiran_path);
        let conn_params = ichiran.conn_params();
        match conn_params {
            Ok(conn_params) => {
                log::info!("db conn params: {:?}", conn_params);
                let proc = Command::new(&self.settings.postgres_path)
                    .args([
                        "-D",
                        &self.settings.db_path,
                        "-p",
                        &format!("{}", conn_params.port),
                    ])
                    .spawn();
                if let Err(err) = &proc {
                    log::warn!("failed to spawn db daemon: {}", err);
                }
                proc.ok()
            }
            Err(err) => {
                log::warn!("failed to query db conn params: {}", err);
                None
            }
        };
    }

    fn request_ast(&mut self, ui: &Ui, text: &str) {
        log::trace!("request_ast({})", text);
        let ichiran_path = self.settings.ichiran_path.clone();
        let text = text.to_owned();
        let channel_tx = self.channel_tx.clone();

        self.transition(ui, State::Processing);

        thread::spawn(move || {
            let ichiran = Ichiran::new(ichiran_path.as_str());
            let result = ichiran.romanize(&text).and_then(|root| {
                Ok(IchiranAst {
                    root,
                    jmdict_data: ichiran.jmdict_data()?,
                })
            });
            let _ = channel_tx.send(Message::Ichiran(result));
        });
    }

    fn transition(&mut self, ui: &Ui, state: State) {
        match &state {
            State::Error { .. } => {
                ui.open_popup(ERROR_MODAL_TITLE);
            }
            _ => (),
        }
        self.state = state;
    }

    fn poll(&mut self, ui: &Ui) {
        match self.channel_rx.try_recv() {
            Ok(Message::Ichiran(Ok(IchiranAst { root, jmdict_data }))) => self.transition(
                ui,
                State::Displaying {
                    rikai: RikaiView::new(root, jmdict_data),
                },
            ),
            Ok(Message::Ichiran(Err(err))) => {
                self.transition(ui, State::Error { err });
            }
            Err(mpsc::TryRecvError::Empty) => {}
            x => {
                panic!("{:?}", x);
            }
        }
    }

    fn show_main_menu(&mut self, _env: &mut Env, ui: &Ui) {
        if let Some(_menu_bar) = ui.begin_main_menu_bar() {
            if let Some(_menu) = ui.begin_menu("File") {
                if MenuItem::new("Quit").build(ui) {}
            }
            if let Some(_menu) = ui.begin_menu("Edit") {
                if MenuItem::new("Settings").build(ui) {
                    self.show_settings = true;
                }
            }
            if let Some(_menu) = ui.begin_menu("View") {
                if MenuItem::new("Show Raw").build(ui) {
                    self.show_raw = true;
                }
                if MenuItem::new("ImGui Demo").build(ui) {
                    self.show_imgui_demo = true;
                }
            }
        }
    }

    fn show_error_modal(&mut self, _env: &mut Env, ui: &Ui) {
        match &self.state {
            State::Error { err } => {
                PopupModal::new(ERROR_MODAL_TITLE)
                    .always_auto_resize(true)
                    .build(ui, || {
                        let _wrap_token = ui.push_text_wrap_pos_with_pos(300.0);
                        ui.text(err.to_string());
                        ui.separator();
                        if ui.button_with_size("OK", [120.0, 0.0]) {
                            ui.close_current_popup();
                        }
                    });
            }
            _ => (),
        }
    }

    pub fn ui(&mut self, env: &mut Env, ui: &Ui) {
        self.show_main_menu(env, ui);

        Window::new("niinii")
            .size([300.0, 110.0], Condition::FirstUseEver)
            .build(ui, || {
                {
                    let trunc =
                        str_from_u8_nul_utf8_unchecked(self.input_text.as_bytes()).to_owned();
                    let _token = ui.begin_disabled(matches!(self.state, State::Processing));
                    if ui
                        .input_text_multiline("", &mut self.input_text, [0.0, 50.0])
                        .enter_returns_true(true)
                        .build()
                    {
                        self.requested_text.replace(trunc.clone());
                    }
                    if ui.button_with_size("Go", [120.0, 0.0]) {
                        self.requested_text.replace(trunc);
                    }
                }

                if let Some(clipboard) = ui.clipboard_text() {
                    if clipboard != self.last_clipboard {
                        self.input_text = clipboard.clone();
                        self.last_clipboard = clipboard.clone();
                        self.requested_text.replace(clipboard);
                    }
                }

                self.show_error_modal(env, ui);
                self.poll(ui);
            });

        Window::new("Rikai")
            .size([300., 110.], Condition::FirstUseEver)
            .build(ui, || match &mut self.state {
                State::Displaying { rikai, .. } => {
                    rikai.ui(env, ui, &mut self.show_raw);
                }
                _ => (),
            });

        if self.show_imgui_demo {
            ui.show_demo_window(&mut self.show_imgui_demo);
        }

        if self.show_settings {
            Window::new("Settings")
                .size([300.0, 110.0], Condition::FirstUseEver)
                .always_auto_resize(true)
                .resizable(false)
                .build(ui, || {
                    self.settings.ui(env, ui);
                    ui.separator();
                    if ui.button_with_size("OK", [120.0, 0.0]) {
                        self.show_settings = false;
                    }
                    ui.same_line();
                    ui.text("Restart to apply all changes.");
                });
        }

        if let Some(requested_text) = self.requested_text.clone() {
            match &self.state {
                State::Displaying { .. } => {
                    self.request_ast(ui, &requested_text);
                    self.requested_text = None;
                }
                State::Error { .. } | State::None => {
                    self.request_ast(ui, &requested_text);
                    self.requested_text = None;
                }
                _ => (),
            };
        }
    }

    pub fn settings(&self) -> &SettingsView {
        &self.settings
    }
}

fn str_from_u8_nul_utf8_unchecked(utf8_src: &[u8]) -> &str {
    let nul_range_end = utf8_src
        .iter()
        .position(|&c| c == b'\0')
        .unwrap_or(utf8_src.len()); // default to length if no `\0` present
    ::std::str::from_utf8(&utf8_src[0..nul_range_end]).unwrap()
}
