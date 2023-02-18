use std::sync::mpsc;

use fancy_regex::Regex;
use imgui::*;

use crate::{
    backend::env::{Env, EnvFlags},
    gloss::{Gloss, GlossError, Glossator},
    translation::{self, Translation},
    view::{inject::InjectView, mixins::help_marker, rikai::RikaiView, settings::Settings},
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
    Completed,
}

pub struct App {
    channel_tx: mpsc::Sender<Message>,
    channel_rx: mpsc::Receiver<Message>,

    input_text: String,
    last_clipboard: String,
    request_gloss_text: Option<String>,

    show_imgui_demo: bool,
    show_settings: bool,
    show_raw: bool,
    show_metrics_window: bool,
    show_style_editor: bool,
    show_inject: bool,

    settings: Settings,
    state: State,
    glossator: Glossator,
    rikai: RikaiView,
    inject: InjectView,
}

impl App {
    pub fn new(settings: Settings) -> Self {
        let (channel_tx, channel_rx) = mpsc::channel();
        let glossator = Glossator::new(&settings);
        App {
            channel_tx,
            channel_rx,
            input_text: "".into(),
            last_clipboard: "".into(),
            request_gloss_text: None,
            show_imgui_demo: false,
            show_settings: false,
            show_raw: false,
            show_metrics_window: false,
            show_style_editor: false,
            show_inject: false,
            settings,
            state: State::Completed,
            glossator,
            rikai: RikaiView::new(),
            inject: InjectView::new(),
        }
    }

    fn request_gloss(&mut self, ui: &Ui, text: &str) {
        let channel_tx = self.channel_tx.clone();
        let glossator = &self.glossator;
        let regex = Regex::new(&self.settings.regex_match);
        match regex {
            Ok(regex) => {
                let text = regex
                    .replace(text, &self.settings.regex_replace)
                    .into_owned();
                let variants = if self.settings.more_variants { 5 } else { 1 };

                self.rikai.set_text(text.clone());

                rayon::spawn(enclose! { (glossator) move || {
                    let gloss = glossator.gloss(&text, variants);
                    let _ = channel_tx.send(Message::Gloss(gloss));
                }});
            }
            Err(err) => self.transition(ui, State::Error(Error::Gloss(err.into()))),
        }
    }

    fn request_translation(&mut self, text: &str) {
        let channel_tx = self.channel_tx.clone();
        let text = text.to_owned();
        let deepl_api_key = self.settings.deepl_api_key.clone();

        self.rikai.set_translation_pending(true);

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

    fn poll(&mut self, ui: &Ui, env: &mut Env) {
        match self.channel_rx.try_recv() {
            Ok(Message::Gloss(Ok(gloss))) => {
                if env.flags().contains(EnvFlags::SUPPORTS_ATLAS_UPDATE) {
                    env.add_unknown_glyphs_from_root(&gloss.root);
                }
                let should_translate = self.settings.auto_translate && gloss.translatable;
                let text = gloss.original_text.clone();
                self.rikai.set_gloss(gloss);
                if should_translate {
                    self.request_translation(&text);
                } else {
                    self.transition(ui, State::Completed);
                    self.rikai.set_translation(None);
                }
            }
            Ok(Message::Translation(Ok(translation))) => {
                self.rikai.set_translation(Some(translation));
                self.transition(ui, State::Completed)
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

        match &self.state {
            State::Error(_) | State::Completed => {
                if let Some(request_gloss_text) = self.request_gloss_text.clone() {
                    self.request_gloss_text = None;
                    self.transition(ui, State::Processing);
                    self.request_gloss(ui, &request_gloss_text);
                }
            }
            _ => (),
        };

        if self.settings.watch_clipboard {
            if let Some(clipboard) = ui.clipboard_text() {
                if clipboard != self.last_clipboard {
                    self.input_text = clipboard.clone();
                    self.last_clipboard = clipboard.clone();
                    self.request_gloss_text = Some(clipboard);
                }
            }
        }
    }

    fn show_main_menu(&mut self, env: &mut Env, ui: &Ui) {
        if let Some(_token) = ui.begin_menu_bar() {
            if let Some(_menu) = ui.begin_menu("Options") {
                if ui
                    .menu_item_config("Watch clipboard")
                    .selected(self.settings.watch_clipboard)
                    .build()
                {
                    self.settings.watch_clipboard = !self.settings.watch_clipboard;
                }
                ui.separator();
                if ui.menu_item("Settings") {
                    self.show_settings = true;
                }
            }
            if let Some(_menu) = ui.begin_menu("View") {
                if ui
                    .menu_item_config("Show input")
                    .selected(self.settings.show_manual_input)
                    .build()
                {
                    self.settings.show_manual_input = !self.settings.show_manual_input;
                }
                ui.separator();
                if ui.menu_item("Raw") {
                    self.show_raw = true;
                }
                if ui.menu_item("Style Editor") {
                    self.show_style_editor = true;
                }
                if ui.menu_item("Debugger") {
                    self.show_metrics_window = true;
                }
                if ui.menu_item("ImGui Demo") {
                    self.show_imgui_demo = true;
                }
                if !env.flags().contains(EnvFlags::SHARED_RENDER_CONTEXT) {
                    if ui.menu_item("Inject") {
                        self.show_inject = true;
                    }
                }
            }
        }
    }

    fn show_error_modal(&mut self, _env: &mut Env, ui: &Ui) {
        if let State::Error(err) = &self.state {
            ui.modal_popup_config(ERROR_MODAL_TITLE)
                .always_auto_resize(true)
                .build(|| {
                    let _wrap_token = ui.push_text_wrap_pos_with_pos(300.0);
                    ui.text(err.to_string());
                    ui.separator();
                    if ui.button_with_size("OK", [120.0, 0.0]) {
                        ui.close_current_popup();
                    }
                });
        }
    }

    fn show_deepl_usage(&self, ui: &Ui) {
        if let Some(Translation::DeepL { deepl_usage, .. }) = self.rikai.translation() {
            ui.same_line();
            let fraction = deepl_usage.character_count as f32 / deepl_usage.character_limit as f32;
            ProgressBar::new(fraction)
                .overlay_text(format!(
                    "DeepL API usage: {}/{} ({:.2}%)",
                    deepl_usage.character_count,
                    deepl_usage.character_limit,
                    fraction * 100.0
                ))
                .size([350.0, 0.0])
                .build(ui);
        }
    }

    pub fn ui(&mut self, env: &mut Env, ui: &mut Ui, run: &mut bool) {
        let io = ui.io();
        let mut niinii = ui
            .window("niinii")
            .opened(run)
            .menu_bar(true)
            .draw_background(!self.settings().transparent);
        if !self.settings().overlay_mode && !env.flags().contains(EnvFlags::SHARED_RENDER_CONTEXT) {
            niinii = niinii
                .position([0.0, 0.0], Condition::Always)
                .size(io.display_size, Condition::Always)
                .no_decoration()
        };
        niinii.build(|| {
            self.show_main_menu(env, ui);

            let disabled = matches!(self.state, State::Processing);
            if self.settings().show_manual_input {
                let _disable_input = ui.begin_disabled(disabled);
                if ui
                    .input_text_multiline("##", &mut self.input_text, [0.0, 50.0])
                    .enter_returns_true(true)
                    .build()
                {
                    self.request_gloss_text = Some(self.input_text.clone());
                }
                if ui.button_with_size("Gloss", [120.0, 0.0]) {
                    self.request_gloss_text = Some(self.input_text.clone());
                }
                ui.same_line();

                let enable_tl = self.rikai.gloss().map_or_else(|| false, |x| x.translatable);
                {
                    let mut _disable_tl =
                        ui.begin_disabled(!enable_tl || self.rikai.translation().is_some());
                    if ui.button_with_size("Translate", [120.0, 0.0]) {
                        self.transition(ui, State::Processing);
                        if let Some(gloss) = self.rikai.gloss() {
                            self.request_translation(&gloss.original_text.clone());
                        }
                    }
                }
                if !enable_tl
                    && ui.is_item_hovered_with_flags(ItemHoveredFlags::ALLOW_WHEN_DISABLED)
                {
                    ui.tooltip(|| ui.text("Text does not require translation"));
                }
                self.show_deepl_usage(ui);
            }

            self.rikai.ui(env, ui, &self.settings, &mut self.show_raw);

            if let State::Processing = &self.state {
                ui.set_mouse_cursor(Some(MouseCursor::NotAllowed));
            }
            self.show_error_modal(env, ui);
            self.poll(ui, env);

            ui.new_line();
            if env.font_atlas_dirty() {
                ui.text_disabled("(rebuilding font atlas...)")
            }
        });

        if self.show_imgui_demo {
            ui.show_demo_window(&mut self.show_imgui_demo);
        }

        if self.show_settings {
            self.show_settings(env, ui);
        }
        if self.show_metrics_window {
            ui.show_metrics_window(&mut self.show_metrics_window);
        }
        if self.show_style_editor {
            self.show_style_editor(ui);
        }
        if self.show_inject {
            self.show_inject(env, ui);
        }
    }

    fn show_settings(&mut self, env: &mut Env, ui: &mut Ui) {
        if let Some(_token) = ui.window("Settings").always_auto_resize(true).begin() {
            self.settings.ui(env, ui);
            ui.separator();
            if ui.button_with_size("OK", [120.0, 0.0]) {
                self.show_settings = false;
            }
            ui.same_line();
            ui.text("* Restart to apply these changes");
        }
    }

    fn show_inject(&mut self, env: &mut Env, ui: &mut Ui) {
        if let Some(_token) = ui.window("Inject").always_auto_resize(true).begin() {
            self.inject.ui(env, ui, &mut self.settings);
            ui.separator();
            if ui.button_with_size("OK", [120.0, 0.0]) {
                self.show_inject = false;
            }
        }
    }

    fn show_style_editor(&mut self, ui: &Ui) {
        let mut show_style_editor = self.show_style_editor;
        ui.window("Style Editor")
                .opened(&mut show_style_editor)
                .menu_bar(true)
                .build(|| {
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

    pub fn settings(&self) -> &Settings {
        &self.settings
    }
    pub fn settings_mut(&mut self) -> &mut Settings {
        &mut self.settings
    }
}
