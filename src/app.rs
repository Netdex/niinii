use enclose::enclose;
use fancy_regex::Regex;
use imgui::*;
use tokio::sync::mpsc;

use crate::{
    parser::{self, Parser, SyntaxTree},
    renderer::context::{Context, ContextFlags},
    settings::Settings,
    support::{self},
    translator::{self, Translate, Translation, Translator},
    tts::{self, TtsEngine},
    view::{
        gloss::GlossView,
        inject::InjectView,
        mixins::help_marker,
        settings::SettingsView,
        translator::{TranslationUsageView, TranslatorView},
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

#[derive(Debug)]
enum Message {
    Gloss(Result<SyntaxTree, parser::Error>),
    Translation(Result<Translation, translator::Error>),
}

#[derive(Debug)]
enum State {
    Error(Error),
    Processing,
    Completed,
}

pub struct App {
    runtime: tokio::runtime::Runtime,
    channel_tx: mpsc::UnboundedSender<Message>,
    channel_rx: mpsc::UnboundedReceiver<Message>,

    input_text: String,
    last_clipboard: String,
    request_gloss_text: Option<String>,

    show_imgui_demo: bool,
    show_settings: bool,
    show_metrics_window: bool,
    show_style_editor: bool,
    show_inject: bool,
    show_translator: bool,

    settings: Settings,
    state: State,
    glossator: Parser,
    translator: Translator,
    tts: TtsEngine,
    gloss: GlossView,
}

impl App {
    pub fn new(settings: Settings) -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        let (channel_tx, channel_rx) = tokio::sync::mpsc::unbounded_channel();
        let glossator = runtime.block_on(Parser::new(&settings));
        let translator = Translator::new(&settings);
        let tts = TtsEngine::new(&settings);

        App {
            runtime,
            channel_tx,
            channel_rx,
            input_text: "".into(),
            last_clipboard: "".into(),
            request_gloss_text: None,
            show_imgui_demo: false,
            show_settings: false,
            show_metrics_window: false,
            show_style_editor: false,
            show_inject: false,
            show_translator: false,
            settings,
            state: State::Completed,
            glossator,
            translator,
            tts,
            gloss: GlossView::new(),
        }
    }

    fn request_gloss(&mut self, ui: &Ui, text: &str) {
        let Self {
            channel_tx,
            glossator,
            ..
        } = self;
        let regex = Regex::new(&self.settings.regex_match);
        match regex {
            Ok(regex) => {
                let text = regex
                    .replace(text, &self.settings.regex_replace)
                    .into_owned();
                let variants = if self.settings.more_variants { 5 } else { 1 };

                self.gloss.set_text(text.clone());

                self.runtime
                    .spawn(enclose! { (channel_tx, glossator) async move {
                        let gloss = glossator.parse(&text, variants).await;
                        let _ = channel_tx.send(Message::Gloss(gloss));
                    }});
            }
            Err(err) => self.transition(ui, State::Error(Error::Gloss(err.into()))),
        }
    }

    fn request_translation(&mut self, text: impl Into<String>) {
        let Self {
            translator,
            settings,
            channel_tx,
            ..
        } = self;

        self.gloss.set_translation_pending(true);

        let text = text.into();

        self.runtime.spawn(
            enclose! { (mut translator, mut settings, channel_tx) async move {
                let translation = translator.translate(&settings, text).await;
                let _ = channel_tx.send(Message::Translation(translation));
            }},
        );
    }

    fn request_tts(&mut self, ui: &Ui, text: &str) {
        if let Err(err) = self.tts.request_tts(text) {
            self.transition(ui, State::Error(err.into()));
        }
    }

    fn transition(&mut self, ui: &Ui, state: State) {
        if let State::Error(err) = &state {
            log::error!("{}", err);
            ui.open_popup(ERROR_MODAL_TITLE);
        }
        self.state = state;
    }

    fn poll(&mut self, ui: &Ui, ctx: &mut Context) {
        while let Ok(message) = self.channel_rx.try_recv() {
            match message {
                Message::Gloss(Ok(ast)) => {
                    if ctx.flags().contains(ContextFlags::SUPPORTS_ATLAS_UPDATE) {
                        ctx.add_unknown_glyphs_from_root(&ast.root);
                    }
                    let should_translate = self.settings.auto_translate && ast.translatable;
                    let text = ast.original_text.clone();
                    self.gloss.set_ast(ast);
                    if should_translate {
                        self.request_translation(&text);
                    } else {
                        self.transition(ui, State::Completed);
                        self.gloss.set_translation(None);
                    }
                }
                Message::Translation(Ok(translation)) => {
                    self.gloss.set_translation(Some(translation));
                    self.transition(ui, State::Completed)
                }
                Message::Gloss(Err(err)) => {
                    self.transition(ui, State::Error(err.into()));
                }
                Message::Translation(Err(err)) => {
                    self.transition(ui, State::Error(err.into()));
                }
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

    fn show_menu(&mut self, ctx: &mut Context, ui: &Ui) {
        if let Some(_token) = ui.begin_menu_bar() {
            if let Some(_menu) = ui.begin_menu("Options") {
                if ui
                    .menu_item_config("Watch clipboard")
                    .selected(self.settings.watch_clipboard)
                    .build()
                {
                    self.settings.watch_clipboard = !self.settings.watch_clipboard;
                }
                if ui
                    .menu_item_config("Show input")
                    .selected(self.settings.show_manual_input)
                    .build()
                {
                    self.settings.show_manual_input = !self.settings.show_manual_input;
                }
                ui.separator();
                if ui.menu_item("Style Editor") {
                    self.show_style_editor = true;
                }
                if ui.menu_item("Debugger") {
                    self.show_metrics_window = true;
                }
                if ui.menu_item("ImGui Demo") {
                    self.show_imgui_demo = true;
                }
                if !ctx.flags().contains(ContextFlags::SHARED_RENDER_CONTEXT)
                    && ui.menu_item("Inject")
                {
                    self.show_inject = true;
                }
                if ui.menu_item("Translator") {
                    self.show_translator = true;
                }
                ui.separator();
                if ui.menu_item("Settings") {
                    self.show_settings = true;
                }
            }
            if let Some(_menu) = ui.begin_menu("View") {
                self.gloss.show_menu(ctx, ui);
            }
            ui.separator();
            let disabled = matches!(self.state, State::Processing);
            let _disable_input = ui.begin_disabled(disabled);
            if ui.menu_item("Translate") {
                self.transition(ui, State::Processing);
                if let Some(gloss) = self.gloss.ast() {
                    self.request_translation(&gloss.original_text.clone());
                }
            }
            if ui.menu_item("Speak") {
                if let Some(gloss) = self.gloss.ast() {
                    self.request_tts(ui, &gloss.original_text.clone());
                }
            }
        }
    }

    fn show_error_modal(&mut self, _ctx: &mut Context, ui: &Ui) {
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

    pub fn ui(&mut self, ctx: &mut Context, ui: &mut Ui, run: &mut bool) {
        let io = ui.io();

        let ui_d = support::docking::UiDocking {};
        let _space = ui_d.dockspace_over_viewport();

        let mut niinii = ui
            .window("niinii")
            .opened(run)
            .menu_bar(true)
            .draw_background(!self.settings().transparent);
        if !self.settings().overlay_mode
            && !ctx.flags().contains(ContextFlags::SHARED_RENDER_CONTEXT)
        {
            niinii = niinii
                .position([0.0, 0.0], Condition::Always)
                .size(io.display_size, Condition::Always)
                .bring_to_front_on_focus(false)
                .no_decoration()
        };
        niinii.build(|| {
            self.show_menu(ctx, ui);

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

                let enable_tl = self.gloss.ast().map_or_else(|| false, |x| x.translatable);
                {
                    let mut _disable_tl =
                        ui.begin_disabled(!enable_tl || self.gloss.translation().is_some());
                    if ui.button_with_size("Translate", [120.0, 0.0]) {
                        self.transition(ui, State::Processing);
                        if let Some(gloss) = self.gloss.ast() {
                            self.request_translation(&gloss.original_text.clone());
                        }
                    }
                }
                if !enable_tl
                    && ui.is_item_hovered_with_flags(ItemHoveredFlags::ALLOW_WHEN_DISABLED)
                {
                    ui.tooltip(|| ui.text("Text does not require translation"));
                }
                if let Some(translation) = self.gloss.translation() {
                    TranslationUsageView(translation).ui(ui);
                }
            }

            self.gloss.ui(ctx, ui, &self.settings);

            if let State::Processing = &self.state {
                ui.set_mouse_cursor(Some(MouseCursor::NotAllowed));
            }
            self.show_error_modal(ctx, ui);
            self.poll(ui, ctx);

            ui.new_line();
            if ctx.font_atlas_dirty() {
                ui.text_disabled("(rebuilding font atlas...)")
            }
        });

        if self.show_imgui_demo {
            ui.show_demo_window(&mut self.show_imgui_demo);
        }
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
            .size_constraints([200.0, 100.0], [1000.0, 1000.0])
            .opened(&mut self.show_translator)
            .begin()
        {
            TranslatorView(&self.translator, &mut self.settings).ui(ui);
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
