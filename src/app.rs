use std::sync::mpsc;

use imgui::*;

use crate::{
    backend::renderer::Env,
    gloss::{Gloss, GlossError, Glossator},
    translation::{self, Translation},
    view::{mixins::help_marker, rikai::RikaiView, settings::SettingsView},
};

const ERROR_MODAL_TITLE: &str = "Error";

#[derive(thiserror::Error, Debug)]
enum Error {
    #[error(transparent)]
    Gloss(#[from] GlossError),
    #[error(transparent)]
    DeepL(#[from] deepl_api::Error),
}

#[derive(Debug)]
enum Message {
    Gloss(Result<Gloss, GlossError>),
    Translation(Result<Translation, deepl_api::Error>),
}

#[derive(Debug)]
enum State {
    Error(Error),
    Processing,
    None,
}

pub struct App {
    channel_tx: mpsc::Sender<Message>,
    channel_rx: mpsc::Receiver<Message>,

    input_text: String,
    last_clipboard: String,
    request_gloss_text: Option<String>,
    request_translate_text: bool,

    show_imgui_demo: bool,
    show_settings: bool,
    show_raw: bool,
    show_metrics_window: bool,
    show_style_editor: bool,

    settings: SettingsView,
    state: State,
    glossator: Glossator,
    rikai: RikaiView,
}

impl App {
    pub fn new(settings: SettingsView) -> Self {
        let (channel_tx, channel_rx) = mpsc::channel();
        let glossator = Glossator::new(&settings);
        App {
            channel_tx,
            channel_rx,
            input_text: "".into(),
            last_clipboard: "".into(),
            request_gloss_text: None,
            request_translate_text: false,
            show_imgui_demo: false,
            show_settings: false,
            show_raw: false,
            show_metrics_window: false,
            show_style_editor: false,
            settings,
            state: State::None,
            glossator,
            rikai: RikaiView::new(),
        }
    }

    fn request_gloss(&self, text: &str) {
        let channel_tx = self.channel_tx.clone();
        let glossator = &self.glossator;
        let text = text.to_owned();
        let variants = if self.settings.more_variants { 5 } else { 1 };
        rayon::spawn(enclose! { (glossator) move || {
            let gloss = glossator.gloss(&text, variants);
            let _ = channel_tx.send(Message::Gloss(gloss));
        }});
    }

    fn request_translation(&self, text: &str) {
        let channel_tx = self.channel_tx.clone();
        let text = text.to_owned();
        let deepl_api_key = self.settings.deepl_api_key.clone();
        rayon::spawn(move || {
            let translation = translation::translate(&deepl_api_key, &text);
            let _ = channel_tx.send(Message::Translation(translation));
        });
    }

    fn transition(&mut self, ui: &Ui, state: State) {
        if let State::Error(err) = &state {
            log::error!("{}", err);
            ui.open_popup(ERROR_MODAL_TITLE);
        }
        self.state = state;
    }

    fn poll(&mut self, ui: &Ui) {
        match self.channel_rx.try_recv() {
            Ok(Message::Gloss(Ok(gloss))) => {
                self.rikai.set_gloss(Some(gloss));
                self.rikai.set_translation(None);
                self.transition(ui, State::None)
            }
            Ok(Message::Translation(Ok(translation))) => {
                self.rikai.set_translation(Some(translation));
                self.transition(ui, State::None)
            }
            Ok(Message::Gloss(Err(err))) => {
                self.transition(ui, State::Error(err.into()));
            }
            Ok(Message::Translation(Err(err))) => {
                self.transition(ui, State::Error(err.into()));
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
                if MenuItem::new("Show input")
                    .selected(self.settings.show_manual_input)
                    .build(ui)
                {
                    self.settings.show_manual_input = !self.settings.show_manual_input;
                }
                ui.separator();
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
        if let State::Error(err) = &self.state {
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

            let _disable_input = ui.begin_disabled(matches!(self.state, State::Processing));
            if self.settings().show_manual_input {
                if ui
                    .input_text_multiline("", &mut self.input_text, [0.0, 50.0])
                    .enter_returns_true(true)
                    .build()
                {
                    self.request_gloss_text = Some(self.input_text.clone());
                }
                if ui.button_with_size("Gloss", [120.0, 0.0]) {
                    self.request_gloss_text = Some(self.input_text.clone());
                }
                ui.same_line();

                let mut _disable_tl = ui.begin_disabled(
                    self.rikai.gloss().is_none() || self.rikai.translation().is_some(),
                );
                if ui.button_with_size("Translate", [120.0, 0.0]) {
                    self.request_translate_text = true;
                }
                ui.separator();
            }

            {
                let _disable_ready = ui.begin_disabled(!matches!(self.state, State::None));
                self.rikai.ui(env, ui, &self.settings, &mut self.show_raw);
                if let State::Processing = &self.state {
                    ui.set_mouse_cursor(Some(MouseCursor::NotAllowed));
                }
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

        if self.settings.watch_clipboard {
            if let Some(clipboard) = ui.clipboard_text() {
                if clipboard != self.last_clipboard {
                    self.input_text = clipboard.clone();
                    self.last_clipboard = clipboard.clone();
                    self.request_gloss_text = Some(clipboard.clone());
                    if self.settings.tl_clipboard {
                        self.request_translate_text = true;
                    }
                }
            }
        }

        match &self.state {
            State::Error(_) | State::None => {
                if let Some(request_gloss_text) = self.request_gloss_text.clone() {
                    self.request_gloss_text = None;
                    self.transition(ui, State::Processing);
                    self.request_gloss(&request_gloss_text);
                }
            }
            _ => (),
        };
        match &self.state {
            State::Error(_) | State::None => {
                if self.request_translate_text {
                    self.request_translate_text = false;
                    self.transition(ui, State::Processing);
                    if let Some(gloss) = self.rikai.gloss() {
                        self.request_translation(&gloss.root.text_flatten());
                    }
                }
            }
            _ => (),
        };
    }

    pub fn settings(&self) -> &SettingsView {
        &self.settings
    }
    pub fn settings_mut(&mut self) -> &mut SettingsView {
        &mut self.settings
    }
}
