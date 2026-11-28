use dear_imgui_reflect::ReflectSession;
use dear_imgui_rs::*;

use crate::{
    gui::{px, px2, Frame},
    history::{History, HistoryEvent},
    settings::{RubyTextType, Settings},
    support::{platform::ScrollLockSync, regex::CachedRegex},
    tts::{self, TtsEngine},
    view::{
        gloss::{GlossInputAction, GlossView},
        help,
        mixins::{error_note, stroke_text_with_highlight},
        panel::{Panel, TextSize},
        settings as settings_view,
        shortcuts::Action,
        style_editor,
        transcript::{self, TranscriptView},
        translator::{self as translator_view, TranslatorWindow},
        vndb::{self as vndb_view, VndbView},
    },
};

#[derive(thiserror::Error, Debug)]
enum Error {
    #[error(transparent)]
    Gloss(#[from] crate::parser::Error),
    #[error(transparent)]
    TextToSpeech(#[from] tts::Error),
}

/// The app's optional windows.
struct Panels {
    settings: Panel,
    style_editor: Panel,
    translator: Panel,
    transcript: Panel,
    vndb: Panel,
    help: Panel,
}

impl Panels {
    fn new() -> Self {
        Self {
            settings: Panel::new("Settings", settings_view::SIZING),
            style_editor: Panel::new("Style Editor", style_editor::SIZING),
            translator: Panel::new("Translator", translator_view::SIZING),
            transcript: Panel::new("Transcript", transcript::SIZING),
            vndb: Panel::new("VNDB", vndb_view::SIZING),
            help: Panel::new("About niinii", help::SIZING),
        }
    }
}

/// Dear ImGui's own inspection windows.
#[derive(Default)]
struct DebugTools {
    metrics: bool,
    log: bool,
    id_stack: bool,
}

impl DebugTools {
    fn show(&mut self, ui: &Ui) {
        if self.metrics {
            ui.show_metrics_window(&mut self.metrics);
        }
        if self.log {
            ui.show_debug_log_window(&mut self.log);
        }
        if self.id_stack {
            ui.show_id_stack_tool_window(&mut self.id_stack);
        }
    }
}

/// Longer clipboard text goes into the input box but is not glossed.
const MAX_CLIPBOARD_GLOSS: usize = 500;

pub struct App {
    debug: DebugTools,
    no_inputs: bool,
    scroll_lock: ScrollLockSync,

    settings: Settings,
    /// Shown under the transcript until the next line.
    error: Option<Error>,
    tts: TtsEngine,
    history: History,
    gloss: GlossView,
    translator_window: TranslatorWindow,
    vndb_view: VndbView,
    transcript: TranscriptView,
    panels: Panels,
    /// Editor state for the reflected settings views.
    reflect: ReflectSession,

    auto_tts_regex: CachedRegex,
}

impl App {
    pub async fn new(settings: Settings) -> Self {
        let tts = TtsEngine::new(&settings);
        let history = History::new(&settings).await;
        let translator_window = TranslatorWindow::new(&settings);
        let vndb_view = VndbView::new(
            translator_window.backend(),
            history.parser().ichiran(),
            &settings,
        );
        App {
            debug: DebugTools::default(),
            no_inputs: false,
            scroll_lock: ScrollLockSync::new(false),
            settings,
            error: None,
            tts,
            history,
            gloss: GlossView::new(),
            translator_window,
            vndb_view,
            transcript: TranscriptView::new(),
            panels: Panels::new(),
            reflect: settings_view::reflect_session(),
            auto_tts_regex: CachedRegex::default(),
        }
    }

    fn request_gloss(&mut self, text: &str) {
        let processed = match self.history.request(text, &self.settings) {
            Ok(Some(p)) => p,
            Ok(None) => return,
            Err(err) => return self.error(Error::Gloss(err)),
        };
        self.error = None;
        // Auto-translate and auto-tts both run on the regex-processed input
        // text and don't need the parsed AST, so kick them off here in
        // parallel with the still-running parse.
        if self.settings.translation.auto_translate {
            self.translate(processed.clone());
        }
        if let Some(pattern) = self.settings.tts.auto_tts_regex.clone() {
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
        self.history.set_translation(id);
    }

    fn translate_current(&mut self) {
        if let Some(text) = self.history.ast().map(|ast| ast.original_text.clone()) {
            self.translate(text);
        }
    }

    fn speak_current(&mut self) {
        if let Some(text) = self.history.ast().map(|ast| ast.original_text.clone()) {
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

    fn poll(&mut self) {
        while let Some(event) = self.history.poll(&self.settings) {
            match event {
                HistoryEvent::ClipboardReceived(text) => {
                    self.gloss.set_input(&text);
                    if text.len() <= MAX_CLIPBOARD_GLOSS {
                        self.request_gloss(&text);
                    }
                }
                HistoryEvent::Failed(err) => self.error(err.into()),
            }
        }
    }

    fn cycle_ruby_text(&mut self) {
        self.settings.interface.ruby_text_type = match self.settings.interface.ruby_text_type {
            RubyTextType::None => RubyTextType::Furigana,
            RubyTextType::Furigana => RubyTextType::Romaji,
            RubyTextType::Romaji => RubyTextType::None,
        };
    }

    fn run(&mut self, action: Action) {
        let processing = self.history.is_processing();
        match action {
            Action::Translate if !processing => self.translate_current(),
            Action::Speak if !processing => self.speak_current(),
            Action::Translate | Action::Speak => {}
            Action::CycleRubyText => self.cycle_ruby_text(),
            Action::PreviousLine => self.history.back(),
            Action::NextLine => self.history.forward(),
            Action::LatestLine => self.history.latest(),
        }
    }

    fn show_menu(&mut self, ui: &Ui) {
        if let Some(_token) = ui.begin_menu_bar() {
            if let Some(_menu) = ui.begin_menu("Options") {
                ui.menu_item_toggle_no_shortcut(
                    "Watch clipboard",
                    &mut self.settings.watch_clipboard,
                    true,
                );
                ui.menu_item_toggle_no_shortcut(
                    "Show input",
                    &mut self.settings.show_manual_input,
                    true,
                );
                ui.menu_item_toggle_no_shortcut(
                    "Auto-translate",
                    &mut self.settings.translation.auto_translate,
                    true,
                );
                if let Some(_menu) = ui.begin_menu("Ruby text") {
                    for (label, kind) in [
                        ("None", RubyTextType::None),
                        ("Furigana", RubyTextType::Furigana),
                        ("Romaji", RubyTextType::Romaji),
                    ] {
                        let selected = self.settings.interface.ruby_text_type == kind;
                        if ui.menu_item_enabled_selected_no_shortcut(label, selected, true) {
                            self.settings.interface.ruby_text_type = kind;
                        }
                    }
                    ui.separator();
                    if Action::CycleRubyText.menu_item(ui, "Cycle") {
                        self.run(Action::CycleRubyText);
                    }
                }
                ui.separator();
                self.panels.style_editor.menu_item(ui);
                self.panels.translator.menu_item(ui);
                self.panels.transcript.menu_item(ui);
                self.panels.vndb.menu_item(ui);
                ui.separator();
                self.panels.settings.menu_item(ui);
                ui.separator();
                let mut no_inputs = self.no_inputs;
                if ui.menu_item_toggle_with_shortcut(
                    "Disable interaction",
                    "ScrLk",
                    &mut no_inputs,
                    true,
                ) {
                    self.set_no_inputs(no_inputs);
                }
            }
            if let Some(_menu) = ui.begin_menu("Gloss") {
                self.gloss.show_menu(ui);
            }
            if let Some(_menu) = ui.begin_menu("Help") {
                self.panels.help.menu_item(ui);
            }
            if let Some(_menu) = ui.begin_menu("Debug") {
                ui.menu_item_toggle_no_shortcut("Metrics", &mut self.debug.metrics, true);
                ui.menu_item_toggle_no_shortcut("Debug log", &mut self.debug.log, true);
                ui.menu_item_toggle_no_shortcut("ID stack tool", &mut self.debug.id_stack, true);
            }
            ui.separator();
            let disable_state = ui.begin_disabled_with_cond(self.history.is_processing());
            if Action::Translate.menu_item(ui, "Translate") {
                self.run(Action::Translate);
            }
            if cfg!(feature = "voicevox") && Action::Speak.menu_item(ui, "Speak") {
                self.run(Action::Speak);
            }
            drop(disable_state);

            ui.separator();
            let (pos, len) = self.history.position();
            {
                let _disabled = ui.begin_disabled_with_cond(pos <= 1);
                if Action::PreviousLine.menu_item(ui, "<") {
                    self.run(Action::PreviousLine);
                }
            }
            {
                let _disabled = ui.begin_disabled_with_cond(pos >= len);
                if Action::NextLine.menu_item(ui, ">") {
                    self.run(Action::NextLine);
                }
            }
            if len > 0 {
                ui.text_disabled(format!("{pos}/{len}"));
            }

            ui.separator();
            self.vndb_view.show_status(&mut self.panels.vndb, ui);
        }
    }

    /// Errors for the line being shown, under the transcript.
    fn show_notes(&self, ui: &Ui) {
        let mut notes: Vec<String> = self.error.iter().map(|e| e.to_string()).collect();
        notes.extend(self.translator_window.notes(self.history.translation()));
        if notes.is_empty() {
            return;
        }
        for note in notes {
            error_note(ui, &note);
        }
    }

    fn set_no_inputs(&mut self, no_inputs: bool) {
        self.no_inputs = no_inputs;
        self.scroll_lock.set(no_inputs);
    }

    fn show_input_toggle(&mut self, ui: &Ui) {
        ui.window("Interaction")
            .flags(
                WindowFlags::ALWAYS_AUTO_RESIZE
                    | WindowFlags::NO_MOVE
                    | WindowFlags::NO_COLLAPSE
                    | WindowFlags::NO_RESIZE
                    | WindowFlags::NO_TITLE_BAR,
            )
            .position(px2(ui, [12.0, 12.0]), Condition::Always)
            .bg_alpha(0.75)
            .build(|| {
                ui.text("Interaction disabled");
                ui.text_disabled("(ScrLk to toggle)");
                if ui.button_with_size("Enable interaction", [px(ui, 180.0), 0.0]) {
                    self.set_no_inputs(false);
                }
            });
    }

    pub fn ui(&mut self, ui: &Ui, frame: &mut Frame) {
        if let Some(scroll_lock) = self.scroll_lock.poll() {
            self.no_inputs = scroll_lock;
        }
        frame.keep_on_top(self.settings.window.on_top);
        frame.set_text_scale(self.settings.window.text_scale);

        let no_inputs = self.no_inputs;
        if no_inputs {
            self.show_input_toggle(ui);
        }

        let mut flags = WindowFlags::MENU_BAR;
        flags.set(
            WindowFlags::NO_BACKGROUND,
            self.settings.window.transparent || no_inputs,
        );
        flags.set(WindowFlags::NO_INPUTS, no_inputs);
        // The app's one taskbar entry.
        ui.set_next_window_class(
            &WindowClass::default()
                .viewport_flags_override_clear(WindowClassViewportFlags::NO_TASK_BAR_ICON),
        );
        let mut open = true;
        ui.window("niinii")
            .size_constraints(TextSize::new(36.0, 5.0).px(ui), [f32::MAX, f32::MAX])
            .opened(&mut open)
            .flags(flags)
            .build(|| {
                if no_inputs {
                    frame.pass_input_through(ui);
                }
                self.show_menu(ui);

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
                self.poll();
                for action in Action::pressed(ui) {
                    self.run(action);
                }

                if self.settings().show_manual_input {
                    let action = self.gloss.show_input(ui, &self.history);
                    if let Some(id) = self.history.translation() {
                        self.translator_window.draw_usage(ui, id);
                    }
                    if let Some(action) = action {
                        match action {
                            GlossInputAction::Gloss(text) => self.request_gloss(&text),
                            GlossInputAction::Translate(text) => self.translate(text),
                        }
                    }
                }

                self.gloss.ui(ui, &self.settings, &mut self.history);
                if let Some(id) = self.history.translation() {
                    self.translator_window.draw_exchange(
                        ui,
                        id,
                        self.history.showing_earlier_line(),
                    );
                }
                self.show_notes(ui);

                if self.history.is_processing() {
                    ui.set_mouse_cursor(Some(MouseCursor::NotAllowed));
                }
            });

        let panels = &mut self.panels;
        settings_view::show(&mut panels.settings, ui, &self.reflect, &mut self.settings);
        style_editor::show(&mut panels.style_editor, ui, frame, &mut self.settings);
        self.translator_window.show(
            &mut panels.translator,
            ui,
            &self.reflect,
            &mut self.settings,
        );
        self.vndb_view
            .show(&mut panels.vndb, ui, &mut self.settings);
        help::show(&mut panels.help, ui, frame.adapter());
        self.transcript.show(
            &mut panels.transcript,
            ui,
            &mut self.history,
            &self.translator_window,
        );
        self.debug.show(ui);

        if !open {
            frame.exit();
        }
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }
}
