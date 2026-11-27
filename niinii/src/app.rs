use imgui::*;

use crate::{
    renderer::context::Context,
    settings::{RubyTextType, Settings},
    support::{platform::ScrollLockSync, regex::CachedRegex},
    tts::{self, TtsEngine},
    view::{
        gloss::{GlossEvent, GlossInputAction, GlossView},
        help::HelpView,
        mixins::{ellipses, stroke_text_with_highlight},
        settings::SettingsView,
        style_editor::StyleEditor,
        transcript::TranscriptView,
        translator::TranslatorWindow,
        vndb::VndbView,
    },
};

#[derive(thiserror::Error, Debug)]
enum Error {
    #[error(transparent)]
    Gloss(#[from] crate::parser::Error),
    #[error(transparent)]
    TextToSpeech(#[from] tts::Error),
}

pub struct App {
    show_metrics_window: bool,
    no_inputs: bool,
    scroll_lock: ScrollLockSync,

    settings: Settings,
    /// Shown under the transcript until the next line.
    error: Option<Error>,
    tts: TtsEngine,
    gloss: GlossView,
    translator_window: TranslatorWindow,
    settings_view: SettingsView,
    style_editor: StyleEditor,
    vndb_view: VndbView,
    help_view: HelpView,
    transcript: TranscriptView,

    auto_tts_regex: CachedRegex,
}

impl App {
    pub async fn new(settings: Settings) -> Self {
        let tts = TtsEngine::new(&settings);
        let gloss = GlossView::new(&settings).await;
        let translator_window = TranslatorWindow::new(&settings);
        let vndb_view = VndbView::new(
            translator_window.backend(),
            gloss.parser().ichiran(),
            &settings,
        );
        App {
            show_metrics_window: false,
            no_inputs: false,
            scroll_lock: ScrollLockSync::new(false),
            settings,
            error: None,
            tts,
            gloss,
            translator_window,
            settings_view: SettingsView::new(),
            style_editor: StyleEditor::new(),
            vndb_view,
            help_view: HelpView::new(),
            transcript: TranscriptView::new(),
            auto_tts_regex: CachedRegex::default(),
        }
    }

    fn request_gloss(&mut self, text: &str) {
        let processed = match self.gloss.request(text, &self.settings) {
            Ok(Some(p)) => p,
            Ok(None) => return,
            Err(err) => return self.error(Error::Gloss(err)),
        };
        self.error = None;
        // Auto-translate and auto-tts both run on the regex-processed input
        // text and don't need the parsed AST, so kick them off here in
        // parallel with the still-running parse.
        if self.settings.auto_translate {
            self.translate(processed.clone());
        }
        if let Some(pattern) = self.settings.auto_tts_regex.clone() {
            let tts_text = self.auto_tts_regex.get(&pattern).ok().and_then(|regex| {
                regex.captures(&processed).unwrap().map(|captures| {
                    captures
                        .get(1)
                        .map(|cap| cap.as_str().to_owned())
                        .unwrap_or_else(|| processed.clone())
                })
            });
            if let Some(tts_text) = tts_text {
                self.request_tts(&tts_text);
            }
        }
    }

    /// Translate `text` as the line being shown.
    fn translate(&mut self, text: String) {
        let id = self.translator_window.translate(&self.settings, text);
        self.gloss.set_translation(id);
    }

    fn translate_current(&mut self) {
        if let Some(text) = self.gloss.ast().map(|ast| ast.original_text.clone()) {
            self.translate(text);
        }
    }

    fn speak_current(&mut self) {
        if let Some(text) = self.gloss.ast().map(|ast| ast.original_text.clone()) {
            self.request_tts(&text);
        }
    }

    fn request_tts(&mut self, text: &str) {
        let span = tracing::debug_span!("tts");
        let _enter = span.enter();
        if let Err(err) = self.tts.request_tts(text) {
            self.error(err.into());
        }
    }

    fn error(&mut self, err: Error) {
        tracing::error!(%err);
        self.error = Some(err);
    }

    fn poll(&mut self, ui: &Ui, ctx: &mut Context) {
        while let Some(event) = self.gloss.poll(ui, ctx, &self.settings) {
            match event {
                GlossEvent::ClipboardReceived(text) => self.request_gloss(&text),
                GlossEvent::Failed(err) => self.error(err.into()),
            }
        }
    }

    fn cycle_ruby_text(&mut self) {
        self.settings.ruby_text_type = match self.settings.ruby_text_type {
            RubyTextType::None => RubyTextType::Furigana,
            RubyTextType::Furigana => RubyTextType::Romaji,
            RubyTextType::Romaji => RubyTextType::None,
        };
    }

    /// Keys act while any niinii window has focus, except when typing. Listed
    /// for the user in `view::help::SHORTCUTS`.
    fn handle_shortcuts(&mut self, ui: &Ui) {
        if ui.io().want_text_input {
            return;
        }
        let processing = self.gloss.is_processing();
        if ui.is_key_pressed_no_repeat(Key::T) && !processing {
            self.translate_current();
        }
        if cfg!(feature = "voicevox") && ui.is_key_pressed_no_repeat(Key::S) && !processing {
            self.speak_current();
        }
        if ui.is_key_pressed_no_repeat(Key::R) {
            self.cycle_ruby_text();
        }
        if ui.is_key_pressed(Key::LeftArrow) {
            self.gloss.history_back();
        }
        if ui.is_key_pressed(Key::RightArrow) {
            self.gloss.history_forward();
        }
        if ui.is_key_pressed_no_repeat(Key::End) {
            self.gloss.history_latest();
        }
    }

    fn show_menu(&mut self, ctx: &mut Context, ui: &Ui) {
        if let Some(_token) = ui.begin_menu_bar() {
            if let Some(_menu) = ui.begin_menu("Options") {
                ui.menu_item_config("Watch clipboard")
                    .build_with_ref(&mut self.settings.watch_clipboard);
                ui.menu_item_config("Show input")
                    .build_with_ref(&mut self.settings.show_manual_input);
                ui.menu_item_config("Auto-translate")
                    .build_with_ref(&mut self.settings.auto_translate);
                if let Some(_menu) = ui.begin_menu("Ruby text") {
                    for (label, kind) in [
                        ("None", RubyTextType::None),
                        ("Furigana", RubyTextType::Furigana),
                        ("Romaji", RubyTextType::Romaji),
                    ] {
                        if ui
                            .menu_item_config(label)
                            .selected(self.settings.ruby_text_type == kind)
                            .build()
                        {
                            self.settings.ruby_text_type = kind;
                        }
                    }
                    ui.separator();
                    if ui.menu_item_config("Cycle").shortcut("R").build() {
                        self.cycle_ruby_text();
                    }
                }
                ui.separator();
                self.style_editor.show_menu_item(ui);
                self.translator_window.show_menu_item(ui);
                self.transcript.show_menu_item(ui);
                self.vndb_view.show_menu_item(ui);
                ui.separator();
                self.settings_view.show_menu_item(ui);
                ui.separator();
                let mut no_inputs = self.no_inputs;
                if ui
                    .menu_item_config("Disable interaction")
                    .shortcut("ScrLk")
                    .build_with_ref(&mut no_inputs)
                {
                    self.set_no_inputs(no_inputs);
                }
            }
            if let Some(_menu) = ui.begin_menu("Gloss") {
                self.gloss.show_menu(ctx, ui);
            }
            if let Some(_menu) = ui.begin_menu("Help") {
                self.help_view.show_menu_item(ui);
            }
            if let Some(_menu) = ui.begin_menu("Debug") {
                if ui.menu_item("Debugger") {
                    self.show_metrics_window = true;
                }
            }
            ui.separator();
            let disable_state = ui.begin_disabled(self.gloss.is_processing());
            if ui.menu_item_config("Translate").shortcut("T").build() {
                self.translate_current();
            }
            if cfg!(feature = "voicevox") && ui.menu_item_config("Speak").shortcut("S").build() {
                self.speak_current();
            }
            drop(disable_state);

            ui.separator();
            let (pos, len) = self.gloss.history_position();
            {
                let _disabled = ui.begin_disabled(pos <= 1);
                if ui.menu_item_config("<").shortcut("Left").build() {
                    self.gloss.history_back();
                }
            }
            {
                let _disabled = ui.begin_disabled(pos >= len);
                if ui.menu_item_config(">").shortcut("Right").build() {
                    self.gloss.history_forward();
                }
            }
            if len > 0 {
                ui.text_disabled(format!("{pos}/{len}"));
            }

            ui.separator();
            self.vndb_view.show_status(ui);
        }
    }

    /// Errors for the line being shown, under the transcript.
    fn show_notes(&self, ui: &Ui) {
        let mut notes: Vec<String> = self.error.iter().map(|e| e.to_string()).collect();
        notes.extend(self.translator_window.notes(self.gloss.translation()));
        if notes.is_empty() {
            return;
        }
        let draw_list = ui.get_window_draw_list();
        let _wrap = ui.push_text_wrap_pos_with_pos(0.0);
        for note in notes {
            stroke_text_with_highlight(
                ui,
                &draw_list,
                &note,
                1.0,
                Some(StyleColor::PlotLinesHovered),
            );
        }
    }

    fn set_no_inputs(&mut self, no_inputs: bool) {
        self.no_inputs = no_inputs;
        self.scroll_lock.set(no_inputs);
    }

    fn show_input_toggle(&mut self, ui: &Ui) -> bool {
        let mut hovered = false;
        ui.window("Interaction")
            .always_auto_resize(true)
            .movable(false)
            .collapsible(false)
            .resizable(false)
            .title_bar(false)
            .position([12.0, 12.0], Condition::Always)
            .bg_alpha(0.75)
            .build(|| {
                ui.text("Interaction disabled");
                ui.text_disabled("(ScrLk to toggle)");
                if ui.button_with_size("Enable interaction", [180.0, 0.0]) {
                    self.set_no_inputs(false);
                }
                hovered = ui.is_window_hovered();
            });
        hovered
    }

    pub fn ui(&mut self, ctx: &mut Context, ui: &mut Ui, run: &mut bool) {
        if let Some(scroll_lock) = self.scroll_lock.poll() {
            self.no_inputs = scroll_lock;
        }

        let no_inputs = self.no_inputs;
        let mut toggle_hovered = false;
        if no_inputs {
            toggle_hovered = self.show_input_toggle(ui);
        }

        let mut niinii = ui
            .window("niinii")
            .opened(run)
            .menu_bar(true)
            .draw_background(!self.settings().transparent);
        if no_inputs {
            niinii = niinii.no_inputs().draw_background(false);
        }
        niinii.build(|| {
            self.show_menu(ctx, ui);

            if no_inputs {
                stroke_text_with_highlight(
                    ui,
                    &ui.get_window_draw_list(),
                    "(interaction disabled)",
                    1.0,
                    Some(StyleColor::PlotLinesHovered),
                );
                return;
            }
            self.poll(ui, ctx);
            self.handle_shortcuts(ui);

            if self.settings().show_manual_input {
                let action = self.gloss.show_input(ui);
                if let Some(id) = self.gloss.translation() {
                    self.translator_window.draw_usage(ui, id);
                }
                if let Some(action) = action {
                    match action {
                        GlossInputAction::Gloss(text) => self.request_gloss(&text),
                        GlossInputAction::Translate(text) => self.translate(text),
                    }
                }
            }

            self.gloss.ui(ctx, ui, &self.settings);
            if let Some(id) = self.gloss.translation() {
                self.translator_window.draw_exchange(
                    ui,
                    ctx,
                    id,
                    self.gloss.showing_earlier_line(),
                );
            }
            self.show_notes(ui);

            if ctx.font_atlas_dirty() {
                ui.new_line();
                ui.text_disabled("(rebuilding font atlas");
                ui.same_line_with_spacing(0.0, 0.0);
                ui.text_disabled(ellipses(ui));
                ui.same_line_with_spacing(0.0, 0.0);
                ui.text_disabled(")");
            }
            if self.gloss.is_processing() {
                ui.set_mouse_cursor(Some(MouseCursor::NotAllowed));
            }
        });

        self.settings_view.ui(ui, &mut self.settings);
        self.style_editor.ui(ui, &mut self.settings);
        self.translator_window.ui(ui, &mut self.settings);
        self.vndb_view.ui(ui, &mut self.settings);
        self.help_view.ui(ui);
        self.transcript
            .ui(ui, &mut self.gloss, &self.translator_window);
        if self.show_metrics_window {
            ui.show_metrics_window(&mut self.show_metrics_window);
        }

        if no_inputs && !toggle_hovered {
            unsafe { sys::igSetNextFrameWantCaptureMouse(false) }
        }
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }
}
