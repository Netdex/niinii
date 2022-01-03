use std::sync::mpsc;

use imgui::*;

use crate::{
    backend::renderer::Env,
    gloss::{Gloss, GlossError, Glossator},
    view::{mixins::help_marker, rikai::RikaiView, settings::SettingsView},
};

const ERROR_MODAL_TITLE: &str = "Error";

#[derive(Debug)]
enum Message {
    Gloss(Result<Gloss, GlossError>),
}

#[derive(Debug)]
enum State {
    Displaying { rikai: RikaiView },
    Error { err: GlossError },
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

    settings: SettingsView,
    state: State,
    gloss_engine: Glossator,
}

impl App {
    pub fn new(settings: SettingsView) -> Self {
        let (channel_tx, channel_rx) = mpsc::channel();
        let gloss_engine = Glossator::new(&settings);
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
            settings,
            state: State::None,
            gloss_engine,
        }
    }

    fn request_ast(&mut self, ui: &Ui, text: &str) {
        let text = text.to_owned();
        let channel_tx = self.channel_tx.clone();

        self.transition(ui, State::Processing);

        let gloss_engine = &self.gloss_engine;
        let use_deepl = self.settings.use_deepl;
        rayon::spawn(enclose! { (gloss_engine) move || {
            let gloss = gloss_engine.gloss(&text, use_deepl);
            let _ = channel_tx.send(Message::Gloss(gloss));
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
            Ok(Message::Gloss(Ok(gloss))) => self.transition(
                ui,
                State::Displaying {
                    rikai: RikaiView::new(gloss),
                },
            ),
            Ok(Message::Gloss(Err(err))) => {
                self.transition(ui, State::Error { err });
            }
            Err(mpsc::TryRecvError::Empty) => {}
            x => {
                log::error!("unhandled message: {:?}", x);
            }
        }
    }

    fn show_main_menu(&mut self, _env: &mut Env, ui: &Ui, run: &mut bool) {
        if let Some(_token) = ui.begin_menu_bar() {
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
        }
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
            niinii
                .menu_bar(true)
                .draw_background(!self.settings().transparent)
        } else {
            niinii
                .position([0.0, 0.0], Condition::Always)
                .size(io.display_size, Condition::Always)
                .menu_bar(true)
                .draw_background(!self.settings().transparent)
                .no_decoration()
        };

        niinii.build(ui, || {
            self.show_main_menu(env, ui, run);

            if self.settings.show_manual_input {
                if CollapsingHeader::new("Manual input")
                    .default_open(true)
                    .build(ui)
                {
                    let _token = ui.begin_disabled(matches!(self.state, State::Processing));
                    if ui
                        .input_text_multiline("", &mut self.input_text, [0.0, 50.0])
                        .enter_returns_true(true)
                        .build()
                    {
                        self.requested_text.replace(self.input_text.clone());
                    }
                    if ui.button_with_size("Go", [120.0, 0.0]) {
                        self.requested_text.replace(self.input_text.clone());
                    }
                    ui.same_line();
                    ui.checkbox("Enable DeepL integration", &mut self.settings.use_deepl);
                }
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
            let mut show_style_editor = self.show_style_editor;
            Window::new("Style Editor")
                .opened(&mut show_style_editor)
                .menu_bar(true)
                .build(ui, || {
                    ui.menu_bar(|| {
                        if ui.button("Save") {
                            self.settings_mut().set_style(Some(&ui.clone_style()));
                        }
                        if ui.button("Reset") {
                            self.settings_mut().set_style(None);
                        }
                        if self.settings.style.is_some() {
                            ui.menu_with_enabled("Style saved", false, || {});
                            help_marker(ui, "Saved style will be restored on start-up. Reset will clear the stored style.");
                        }
                    });
                    ui.show_default_style_editor();
                });
            self.show_style_editor = show_style_editor;
        }

        if let Some(requested_text) = self.requested_text.clone() {
            match &self.state {
                State::Displaying { .. } | State::Error { .. } | State::None => {
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
