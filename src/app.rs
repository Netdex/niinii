use std::collections::HashMap;
use std::sync::mpsc;
use std::thread;

use ichiran::{kanji::Kanji, romanize::Root, Ichiran, IchiranError, JmDictData};
use imgui::*;

use crate::{
    backend::renderer::Env,
    view::{rikai::RikaiView, settings::SettingsView},
};
use ichiran::pgdaemon::PostgresDaemon;

const ERROR_MODAL_TITLE: &str = "Error";

#[derive(Debug)]
struct IchiranAst {
    root: Root,
    kanji_info: HashMap<char, Kanji>,
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
    show_metrics_window: bool,
    show_style_editor: bool,

    ichiran: Ichiran,
    settings: SettingsView,
    state: State,
    _pg_daemon: Option<PostgresDaemon>,
}

impl App {
    pub fn new(settings: SettingsView) -> Self {
        let (channel_tx, channel_rx) = mpsc::channel();
        let ichiran = Ichiran::new(settings.ichiran_path.clone());
        let pg_daemon = match ichiran.conn_params() {
            Ok(conn_params) => {
                let pg_daemon = PostgresDaemon::new(
                    &settings.postgres_path,
                    &settings.db_path,
                    conn_params,
                    false,
                );
                Some(pg_daemon)
            }
            Err(_) => {
                log::warn!("could not get db conn params from ichiran");
                None
            }
        };
        App {
            channel_tx,
            channel_rx,
            requested_text: None,
            input_text: "".into(),
            last_clipboard: "".into(),
            show_imgui_demo: false,
            show_settings: false,
            show_raw: false,
            show_metrics_window: false,
            show_style_editor: false,
            ichiran,
            settings,
            state: State::None,
            _pg_daemon: pg_daemon,
        }
    }

    fn request_ast(&mut self, ui: &Ui, text: &str) {
        let text = text.to_owned();
        let channel_tx = self.channel_tx.clone();

        self.transition(ui, State::Processing);

        let ichiran = &self.ichiran;
        thread::spawn(enclose! { (ichiran) move || {
            let result = (|| {
                let root =
                    thread::spawn(enclose! { (ichiran, text) move || ichiran.romanize(&text, 5) });
                let kanji_info = ichiran.kanji_from_str(&text)?;
                let root = root.join().unwrap()?;

                Ok(IchiranAst {
                    root,
                    kanji_info,
                    jmdict_data: ichiran.jmdict_data()?,
                })
            })();
            let _ = channel_tx.send(Message::Ichiran(result));
        }});
    }

    fn transition(&mut self, ui: &Ui, state: State) {
        if let State::Error { err } = &state {
            log::error!("{}", err);
            ui.open_popup(ERROR_MODAL_TITLE);
        }
        self.state = state;
    }

    fn poll(&mut self, ui: &Ui) {
        match self.channel_rx.try_recv() {
            Ok(Message::Ichiran(Ok(IchiranAst {
                root,
                kanji_info,
                jmdict_data,
            }))) => self.transition(
                ui,
                State::Displaying {
                    rikai: RikaiView::new(root, kanji_info, jmdict_data),
                },
            ),
            Ok(Message::Ichiran(Err(err))) => {
                self.transition(ui, State::Error { err });
            }
            Err(mpsc::TryRecvError::Empty) => {}
            x => {
                log::error!("unhandled message: {:?}", x);
            }
        }
    }

    fn show_main_menu(&mut self, _env: &mut Env, ui: &Ui, run: &mut bool) {
        ui.menu_bar(|| {
            if let Some(_menu) = ui.begin_menu("File") {
                if MenuItem::new("Quit").build(ui) {
                    *run = false;
                }
            }
            if let Some(_menu) = ui.begin_menu("Options") {
                if MenuItem::new("Watch clipboard")
                    .selected(self.settings.watch_clipboard)
                    .build(ui)
                {
                    self.settings.watch_clipboard = !self.settings.watch_clipboard;
                }
                ui.separator();
                if MenuItem::new("Settings").build(ui) {
                    self.show_settings = true;
                }
            }
            if let Some(_menu) = ui.begin_menu("View") {
                if MenuItem::new("Raw").build(ui) {
                    self.show_raw = true;
                }
                if MenuItem::new("Style Editor").build(ui) {
                    self.show_style_editor = true;
                }
                if MenuItem::new("Debugger").build(ui) {
                    self.show_metrics_window = true;
                }
                if MenuItem::new("ImGui Demo").build(ui) {
                    self.show_imgui_demo = true;
                }
            }
        });
    }

    fn show_error_modal(&mut self, _env: &mut Env, ui: &Ui) {
        if let State::Error { err } = &self.state {
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
    }

    pub fn ui(&mut self, env: &mut Env, ui: &mut Ui, run: &mut bool) {
        let io = ui.io();

        let niinii = Window::new("niinii");

        let niinii = if self.settings().overlay_mode {
            niinii.flags(WindowFlags::MENU_BAR | WindowFlags::NO_BACKGROUND)
        } else {
            niinii
                .position([0.0, 0.0], Condition::Always)
                .size(io.display_size, Condition::Always)
                .flags(WindowFlags::MENU_BAR | WindowFlags::NO_DECORATION)
                .bring_to_front_on_focus(false)
        };

        niinii.build(ui, || {
            self.show_main_menu(env, ui, run);

            if self.settings.show_manual_input {
                let _token = ui.begin_disabled(matches!(self.state, State::Processing));
                let trunc = str_from_u8_nul_utf8_unchecked(self.input_text.as_bytes()).to_owned();
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
                ui.separator();
            } else {
                ui.new_line();
            }

            if self.settings.watch_clipboard {
                if let Some(clipboard) = ui.clipboard_text() {
                    if clipboard != self.last_clipboard {
                        self.input_text = clipboard.clone();
                        self.last_clipboard = clipboard.clone();
                        self.requested_text.replace(clipboard);
                    }
                }
            }

            match &mut self.state {
                State::Displaying { rikai, .. } => {
                    rikai.ui(env, ui, &self.settings, &mut self.show_raw);
                }
                State::Processing => ui.set_mouse_cursor(Some(MouseCursor::NotAllowed)),
                _ => (),
            }

            self.show_error_modal(env, ui);
            self.poll(ui);
        });

        if self.show_imgui_demo {
            ui.show_demo_window(&mut self.show_imgui_demo);
        }

        if self.show_settings {
            if let Some(_token) = Window::new("Settings").always_auto_resize(true).begin(ui) {
                self.settings.ui(ui);
                ui.separator();
                if ui.button_with_size("OK", [120.0, 0.0]) {
                    self.show_settings = false;
                }
                ui.same_line();
                ui.text("* Restart to apply these changes");
            }
        }

        if self.show_metrics_window {
            ui.show_metrics_window(&mut self.show_metrics_window);
        }

        if self.show_style_editor {
            Window::new("Style Editor")
                .opened(&mut self.show_style_editor)
                .build(ui, || {
                    ui.show_default_style_editor();
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
    pub fn settings_mut(&mut self) -> &mut SettingsView {
        &mut self.settings
    }
}

fn str_from_u8_nul_utf8_unchecked(utf8_src: &[u8]) -> &str {
    let nul_range_end = utf8_src
        .iter()
        .position(|&c| c == b'\0')
        .unwrap_or(utf8_src.len()); // default to length if no `\0` present
    ::std::str::from_utf8(&utf8_src[0..nul_range_end]).unwrap()
}
