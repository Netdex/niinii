use imgui::*;

use crate::{
    renderer::context::{Context, ContextFlags},
    settings::Settings,
    support::{docking::UiDocking, regex::CachedRegex},
    tts::{self, TtsEngine},
    view::{
        gloss::{GlossEvent, GlossInputAction, GlossView},
        inject::InjectView,
        mixins::{ellipses, stroke_text_with_highlight},
        settings::SettingsView,
        style_editor::StyleEditor,
        translator::TranslatorWindow,
    },
};

const ERROR_MODAL_ID: &str = "Error";

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

    settings: Settings,
    error: Option<Error>,
    tts: TtsEngine,
    gloss: GlossView,
    translator_window: TranslatorWindow,
    settings_view: SettingsView,
    inject_view: InjectView,
    style_editor: StyleEditor,

    auto_tts_regex: CachedRegex,
}

impl App {
    pub async fn new(settings: Settings) -> Self {
        let tts = TtsEngine::new(&settings);
        let gloss = GlossView::new(&settings).await;
        let translator_window = TranslatorWindow::new(&settings);
        App {
            show_metrics_window: false,
            no_inputs: false,
            settings,
            error: None,
            tts,
            gloss,
            translator_window,
            settings_view: SettingsView::new(),
            inject_view: InjectView::new(),
            style_editor: StyleEditor::new(),
            auto_tts_regex: CachedRegex::default(),
        }
    }

    fn request_gloss(&mut self, ui: &Ui, text: &str) {
        let processed = match self.gloss.request(text, &self.settings) {
            Ok(Some(p)) => p,
            Ok(None) => return,
            Err(err) => return self.error(ui, Error::Gloss(err)),
        };
        // Auto-translate and auto-tts both run on the regex-processed input
        // text and don't need the parsed AST, so kick them off here in
        // parallel with the still-running parse.
        if self.settings.auto_translate {
            self.translator_window
                .translate(&self.settings, processed.clone());
        } else {
            self.translator_window.clear_current();
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
                self.request_tts(ui, &tts_text);
            }
        }
    }

    fn request_tts(&mut self, ui: &Ui, text: &str) {
        let span = tracing::debug_span!("tts");
        let _enter = span.enter();
        if let Err(err) = self.tts.request_tts(text) {
            self.error(ui, err.into());
        }
    }

    fn error(&mut self, ui: &Ui, err: Error) {
        tracing::error!(%err);
        self.error = Some(err);
        ui.open_popup(ERROR_MODAL_ID);
    }

    fn poll(&mut self, ui: &Ui, ctx: &mut Context) {
        while let Some(event) = self.gloss.poll(ui, ctx, &self.settings) {
            match event {
                GlossEvent::ClipboardReceived(text) => self.request_gloss(ui, &text),
                GlossEvent::Failed(err) => self.error(ui, err.into()),
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
                self.style_editor.show_menu_item(ui);
                self.translator_window.show_menu_item(ui);
                ui.separator();
                self.settings_view.show_menu_item(ui);
                ui.separator();
                ui.menu_item_config("Disable interaction")
                    .build_with_ref(&mut self.no_inputs);
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
                {
                    self.inject_view.show_menu_item(ui);
                }
            }
            ui.separator();
            let disable_state = ui.begin_disabled(self.gloss.is_processing());
            if ui.menu_item("Translate") {
                if let Some(gloss) = self.gloss.ast() {
                    let text = gloss.original_text.clone();
                    self.translator_window.translate(&self.settings, text);
                }
            }
            if cfg!(feature = "voicevox") && ui.menu_item("Speak") {
                if let Some(gloss) = self.gloss.ast() {
                    self.request_tts(ui, &gloss.original_text.clone());
                }
            }
            drop(disable_state);
        }
    }

    fn show_error_modal(&mut self, _ctx: &mut Context, ui: &Ui) {
        ui.modal_popup_config(ERROR_MODAL_ID)
            .always_auto_resize(true)
            .build(|| {
                let _wrap_token = ui.push_text_wrap_pos_with_pos(300.0);
                if let Some(err) = &self.error {
                    ui.text(err.to_string());
                }
                ui.separator();
                if ui.button_with_size("OK", [120.0, 0.0]) {
                    ui.close_current_popup();
                }
            });
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
                if ui.button_with_size("Enable interaction", [180.0, 0.0]) {
                    self.no_inputs = false;
                }
                hovered = ui.is_window_hovered();
            });
        hovered
    }

    pub fn ui(&mut self, ctx: &mut Context, ui: &mut Ui, run: &mut bool) {
        if self.settings().overlay_mode
            && !ctx.flags().contains(ContextFlags::SHARED_RENDER_CONTEXT)
        {
            ui.dockspace_over_viewport();
        };

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
            self.show_error_modal(ctx, ui);

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

            if self.settings().show_manual_input {
                let action = self.gloss.show_input(ui);
                self.translator_window.draw_current_usage(ui);
                if let Some(action) = action {
                    match action {
                        GlossInputAction::Gloss(text) => self.request_gloss(ui, &text),
                        GlossInputAction::Translate(text) => {
                            self.translator_window.translate(&self.settings, text);
                        }
                    }
                }
            }

            self.gloss.ui(ctx, ui, &self.settings);
            self.translator_window.draw_current_exchange(ui);

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

        self.settings_view.ui(ctx, ui, &mut self.settings);
        self.inject_view.ui(ui, &mut self.settings);
        self.style_editor.ui(ui, &mut self.settings);
        self.translator_window.ui(ui, &mut self.settings);
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
    pub fn settings_mut(&mut self) -> &mut Settings {
        &mut self.settings
    }
}
