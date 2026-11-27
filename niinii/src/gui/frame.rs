use std::collections::HashSet;

use dear_imgui_rs::{Id, Ui};

/// What the UI declares about its native windows for one frame. Nothing
/// carries over: each frame states the full policy, and the gui brings the
/// native windows in line with it afterwards.
pub struct Frame {
    dpi_scale: f32,
    text_scale: f32,
    on_top: bool,
    passthrough: HashSet<Id>,
    exit: bool,
}

impl Frame {
    pub(super) fn new(dpi_scale: f32) -> Self {
        Self {
            dpi_scale,
            text_scale: 1.0,
            on_top: false,
            passthrough: HashSet::new(),
            exit: false,
        }
    }

    /// Scale of the monitor the style was sized for at startup.
    pub fn dpi_scale(&self) -> f32 {
        self.dpi_scale
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
