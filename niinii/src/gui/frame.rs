use std::{collections::HashSet, sync::Arc};

use dear_imgui_rs::{Id, Theme, Ui};

/// What the UI declares about its native windows for one frame. Nothing
/// carries over: each frame states the full policy, and the gui brings the
/// native windows in line with it afterwards.
pub struct Frame {
    style_scale: f32,
    adapter: Arc<str>,
    text_scale: f32,
    saved_theme: Option<Option<Theme>>,
    on_top: bool,
    passthrough: HashSet<Id>,
    exit: bool,
}

impl Frame {
    pub(super) fn new(style_scale: f32, adapter: Arc<str>) -> Self {
        Self {
            style_scale,
            saved_theme: None,
            adapter,
            text_scale: 1.0,
            on_top: false,
            passthrough: HashSet::new(),
            exit: false,
        }
    }

    /// Scale of the monitor the style was sized for at startup.
    pub fn style_scale(&self) -> f32 {
        self.style_scale
    }

    /// The theme the style is rebuilt from when the scale changes, after the
    /// user saved or reset it.
    pub fn theme_saved(&mut self, theme: Option<Theme>) {
        self.saved_theme = Some(theme);
    }

    /// The GPU adapter rendering the windows: name, backend and device type.
    pub fn adapter(&self) -> &str {
        &self.adapter
    }

    /// Multiplies every text size, and with it the lengths written with
    /// `gui::px`. Takes effect from the next frame.
    pub fn set_text_scale(&mut self, scale: f32) {
        self.text_scale = scale;
    }

    /// Keeps every native window above other applications' windows.
    pub fn keep_on_top(&mut self, on_top: bool) {
        self.on_top = on_top;
    }

    /// Lets pointer input on the current window's native window reach other
    /// applications below it.
    pub fn pass_input_through(&mut self, ui: &Ui) {
        self.passthrough.insert(ui.window_viewport().id());
    }

    pub fn exit(&mut self) {
        self.exit = true;
    }

    pub(super) fn take_saved_theme(&mut self) -> Option<Option<Theme>> {
        self.saved_theme.take()
    }

    pub(super) fn text_scale(&self) -> f32 {
        self.text_scale
    }

    pub(super) fn on_top(&self) -> bool {
        self.on_top
    }

    pub(super) fn passes_input_through(&self, viewport: Id) -> bool {
        self.passthrough.contains(&viewport)
    }

    pub(super) fn exit_requested(&self) -> bool {
        self.exit
    }
}
