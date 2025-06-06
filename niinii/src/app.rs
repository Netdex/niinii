use std::sync::Arc;

use enclose::enclose;
use fancy_regex::Regex;
use imgui::*;
use tokio::sync::mpsc;
use tracing::Instrument;

use crate::{
    parser::{self, Parser, SyntaxTree},
    renderer::context::{Context, ContextFlags},
    settings::{Settings, TranslatorType},
    support::docking::UiDocking,
    translator::{
        self, chat::ChatTranslator, deepl::DeepLTranslator, realtime::RealtimeTranslator,
        Translation, Translator,
    },
    tts::{self, TtsEngine},
    view::{
        gloss::GlossView,
        inject::InjectView,
        mixins::{ellipses, help_marker},
        settings::SettingsView,
    },
};

const ERROR_MODAL_TITLE: &str = "Error";

#[derive(thiserror::Error, Debug)]
enum Error {
    #[error(transparent)]
    Gloss(#[from] parser::Error),
    #[error(transparent)]
    Translation(#[from] translator::Error),
    #[error(transparent)]
    TextToSpeech(#[from] tts::Error),
}

enum Message {
    Gloss(Result<SyntaxTree, parser::Error>),
    Translation(Result<Box<dyn Translation>, translator::Error>),
}

#[derive(Debug)]
enum State {
    Error(Error),
    Processing,
    Completed,
}

pub struct App {
    channel_tx: mpsc::UnboundedSender<Message>,
    channel_rx: mpsc::UnboundedReceiver<Message>,

    input_text: String,
    last_clipboard: String,
    request_gloss_text: Option<String>,

    show_settings: bool,
    show_metrics_window: bool,
    show_style_editor: bool,
    show_inject: bool,
    show_translator: bool,

    settings: Settings,
    state: State,
    error: Option<Error>,
    parser: Parser,
    translator: Arc<dyn Translator>,
    tts: TtsEngine,
    gloss: GlossView,
}

impl App {
    pub async fn new(settings: Settings) -> Self {
        let (channel_tx, channel_rx) = tokio::sync::mpsc::unbounded_channel();
        let parser = Parser::new(&settings).await;
        let translator: Arc<dyn Translator> = match settings.translator_type {
            TranslatorType::DeepL => Arc::new(DeepLTranslator),
            TranslatorType::Chat => Arc::new(ChatTranslator::new(&settings)),
            TranslatorType::Realtime => Arc::new(RealtimeTranslator::new(&settings)),
        };
        let tts = TtsEngine::new(&settings);

        App {
            channel_tx,
            channel_rx,
            input_text: "".into(),
            last_clipboard: "".into(),
            request_gloss_text: None,
            show_settings: false,
            show_metrics_window: false,
            show_style_editor: false,
            show_inject: false,
            show_translator: false,
            settings,
            state: State::Completed,
            error: None,
            parser,
            translator,
            tts,
            gloss: GlossView::new(),
        }
    }

    fn request_parse(&mut self, ui: &Ui, text: &str) {
        let regex = Regex::new(&self.settings.regex_match);
        match regex {
            Ok(regex) => {
                let text = regex
                    .replace(text, &self.settings.regex_replace)
                    .into_owned();
                let text = text.trim().to_owned();
                if text.is_empty() {
                    return;
                }

                self.transition(ui, State::Processing);
                self.gloss.set_text(text.clone());

                if self.settings.auto_translate {
                    self.request_translation(ui, text.clone());
                } else {
                    self.gloss.set_translation(None);
                }

                let Self {
                    channel_tx, parser, ..
                } = self;
                let variants = if self.settings.more_variants { 5 } else { 1 };
                tokio::spawn(enclose! { (channel_tx, parser) async move {
                    let span = tracing::debug_span!("parse");
                    let ast = parser.parse(&text, variants).instrument(span).await;
                    let _ = channel_tx.send(Message::Gloss(ast));
                }});
            }
            Err(err) => self.transition(ui, State::Error(Error::Gloss(err.into()))),
        }
    }

    fn request_translation(&mut self, _ui: &Ui, text: impl Into<String>) {
        let Self {
            translator,
            settings,
            channel_tx,
            gloss,
            ..
        } = self;

        gloss.set_translation(None);
        gloss.set_translation_pending(true);

        let text = text.into();

        tokio::spawn(enclose! { (translator, settings, channel_tx) async move {
            let span = tracing::debug_span!("translation");
            let translation = translator.translate(&settings, text).instrument(span).await;
            let _ = channel_tx.send(Message::Translation(translation));
        }});
    }

    fn request_tts(&mut self, ui: &Ui, text: &str) {
        let span = tracing::debug_span!("tts");
        let _enter = span.enter();
        if let Err(err) = self.tts.request_tts(text) {
            self.transition(ui, State::Error(err.into()));
        }
    }

    fn transition(&mut self, ui: &Ui, state: State) {
        if let State::Error(err) = state {
            tracing::error!(%err);
            ui.open_popup(ERROR_MODAL_TITLE);
            self.error = Some(err);
        } else {
            self.state = state;
        }
    }

    fn poll(&mut self, ui: &Ui, ctx: &mut Context) {
        while let Ok(message) = self.channel_rx.try_recv() {
            match message {
                Message::Gloss(Ok(ast)) => {
                    if ctx.flags().contains(ContextFlags::SUPPORTS_ATLAS_UPDATE) {
                        ctx.add_unknown_glyphs_from_root(&ast.root);
                    }
                    let text = ast.original_text.clone();
                    self.gloss.set_ast(ast);
                    if let Some(auto_tts_regex) = &self.settings.auto_tts_regex {
                        let regex = Regex::new(auto_tts_regex).ok();
                        if let Some(regex) = regex {
                            let captures = regex.captures(&text).unwrap();
                            if let Some(captures) = captures {
                                if let Some(cap) = captures.get(1) {
                                    self.request_tts(ui, cap.as_str());
                                } else {
                                    self.request_tts(ui, &text);
                                }
                            }
                        }
                    }
                    self.transition(ui, State::Completed);
                }
                Message::Gloss(Err(err)) => {
                    self.transition(ui, State::Error(err.into()));
                }
                Message::Translation(Ok(translation)) => {
                    self.gloss.set_translation(Some(translation));
                }
                Message::Translation(Err(err)) => {
                    self.gloss.set_translation_pending(false);
                    self.transition(ui, State::Error(err.into()));
                }
            }
        }

        match &self.state {
            State::Error(_) | State::Completed => {
                if let Some(request_gloss_text) = self.request_gloss_text.clone() {
                    self.request_gloss_text = None;
                    self.request_parse(ui, &request_gloss_text);
                }
            }
            _ => (),
        };

        if self.settings.watch_clipboard {
            if let Some(clipboard) = ui.clipboard_text() {
                if clipboard != self.last_clipboard {
                    self.input_text.clone_from(&clipboard);
                    self.last_clipboard.clone_from(&clipboard);
                    self.request_gloss_text = Some(clipboard);
                }
            }
        }
    }

    fn show_menu(&mut self, ctx: &mut Context, ui: &Ui) {
        if let Some(_token) = ui.begin_menu_bar() {
            if let Some(_menu) = ui.begin_menu("Options") {
                ui.menu_item_config("Watch clipboard")
                    .build_with_ref(&mut self.settings.watch_clipboard);
                ui.menu_item_config("Show input")
                    .build_with_ref(&mut self.settings.show_manual_input);
                ui.separator();
                if ui.menu_item("Style Editor") {
                    self.show_style_editor = true;
                }
                if ui.menu_item("Translator") {
                    self.show_translator = true;
                }
                ui.separator();
                if ui.menu_item("Settings") {
                    self.show_settings = true;
                }
            }
            if let Some(_menu) = ui.begin_menu("Gloss") {
                self.gloss.show_menu(ctx, ui);
            }
            if let Some(_menu) = ui.begin_menu("Debug") {
                if ui.menu_item("Debugger") {
                    self.show_metrics_window = true;
                }
                if cfg!(feature = "hook")
                    && !ctx.flags().contains(ContextFlags::SHARED_RENDER_CONTEXT)
                    && ui.menu_item("Inject")
                {
                    self.show_inject = true;
                }
            }
            ui.separator();
            let _disable_state = ui.begin_disabled(matches!(self.state, State::Processing));
            {
                let mut _disable_tl =
                    ui.begin_disabled(self.gloss.ast().is_none_or(|ast| ast.empty()));
                if ui.menu_item("Translate") {
                    if let Some(gloss) = self.gloss.ast() {
                        self.request_translation(ui, gloss.original_text.clone());
                    }
                }
            }
            if cfg!(feature = "voicevox") && ui.menu_item("Speak") {
                if let Some(gloss) = self.gloss.ast() {
                    self.request_tts(ui, &gloss.original_text.clone());
                }
            }
        }
    }

    fn show_error_modal(&mut self, _ctx: &mut Context, ui: &Ui) {
        if let Some(err) = &self.error {
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

    pub fn ui(&mut self, ctx: &mut Context, ui: &mut Ui, run: &mut bool) {
        if self.settings().overlay_mode
            && !ctx.flags().contains(ContextFlags::SHARED_RENDER_CONTEXT)
        {
            ui.dockspace_over_viewport();
        };

        let niinii = ui
            .window("niinii")
            .opened(run)
            .menu_bar(true)
            .draw_background(!self.settings().transparent);
        niinii.build(|| {
            self.show_menu(ctx, ui);
            self.show_error_modal(ctx, ui);
            self.poll(ui, ctx);

            let disabled = matches!(self.state, State::Processing);
            if self.settings().show_manual_input {
                let disable_input = ui.begin_disabled(disabled);
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
                drop(disable_input);
                ui.same_line();

                let enable_tl = self.gloss.ast().is_some_and(|ast| !ast.empty());
                let disable_tl = ui.begin_disabled(!enable_tl);
                if ui.button_with_size("Translate", [120.0, 0.0]) {
                    if let Some(gloss) = self.gloss.ast() {
                        self.request_translation(ui, gloss.original_text.clone());
                    }
                }
                drop(disable_tl);
                if !enable_tl
                    && ui.is_item_hovered_with_flags(ItemHoveredFlags::ALLOW_WHEN_DISABLED)
                {
                    ui.tooltip(|| ui.text("Text does not require translation"));
                }
                if let Some(translation) = self.gloss.translation() {
                    translation.view_usage().ui(ui);
                }
            }

            self.gloss.ui(ctx, ui, &self.settings);

            if ctx.font_atlas_dirty() {
                ui.new_line();
                ui.text_disabled("(rebuilding font atlas");
                ui.same_line_with_spacing(0.0, 0.0);
                ui.text_disabled(ellipses(ui));
                ui.same_line_with_spacing(0.0, 0.0);
                ui.text_disabled(")");
            }
            if let State::Processing = &self.state {
                ui.set_mouse_cursor(Some(MouseCursor::NotAllowed));
            }
        });

        if self.show_settings {
            self.show_settings(ctx, ui);
        }
        if self.show_metrics_window {
            ui.show_metrics_window(&mut self.show_metrics_window);
        }
        if self.show_style_editor {
            self.show_style_editor(ui);
        }
        if self.show_inject {
            self.show_inject(ctx, ui);
        }
        if self.show_translator {
            self.show_translator(ctx, ui);
        }
    }

    fn show_settings(&mut self, ctx: &mut Context, ui: &mut Ui) {
        if let Some(_token) = ui.window("Settings").always_auto_resize(true).begin() {
            SettingsView(&mut self.settings).ui(ctx, ui);
            ui.separator();
            if ui.button_with_size("OK", [120.0, 0.0]) {
                self.show_settings = false;
            }
            ui.same_line();
            ui.text("* Restart to apply these changes");
        }
    }

    fn show_inject(&mut self, ctx: &mut Context, ui: &mut Ui) {
        if let Some(_token) = ui.window("Inject").always_auto_resize(true).begin() {
            InjectView.ui(ctx, ui, &mut self.settings);
            ui.separator();
            if ui.button_with_size("OK", [120.0, 0.0]) {
                self.show_inject = false;
            }
        }
    }

    fn show_translator(&mut self, _ctx: &mut Context, ui: &mut Ui) {
        if let Some(_token) = ui
            .window("Translator")
            .size_constraints([600.0, 300.0], [1200.0, 1200.0])
            .opened(&mut self.show_translator)
            .menu_bar(true)
            .begin()
        {
            self.translator.view(&mut self.settings).ui(ui);
        }
    }

    fn show_style_editor(&mut self, ui: &Ui) {
        let mut show_style_editor = self.show_style_editor;
        ui.window("Style Editor")
                .opened(&mut show_style_editor)
                .menu_bar(true)
                .build(|| {
                    ui.menu_bar(|| {
                        if ui.menu_item("Save") {
                            self.settings_mut().set_style(Some(&ui.clone_style()));
                        }
                        if ui.menu_item("Reset") {
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
