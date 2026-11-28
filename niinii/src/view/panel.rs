use dear_imgui_rs::{Condition, Ui, WindowFlags};

/// A window size in units of body text, so it follows text scale and DPI.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextSize {
    /// Characters of body text across.
    pub chars: f32,
    /// Lines of body text down.
    pub lines: f32,
}

impl TextSize {
    pub const fn new(chars: f32, lines: f32) -> Self {
        Self { chars, lines }
    }

    /// The size in pixels with the current font.
    pub fn px(self, ui: &Ui) -> [f32; 2] {
        // Sarasa Mono's Latin glyphs share one advance.
        let char_width = ui.calc_text_size("0")[0];
        [
            self.chars * char_width,
            self.lines * ui.text_line_height_with_spacing(),
        ]
    }
}

/// How big a window opens and how small it can be made. There is no maximum.
#[derive(Clone, Copy, Debug)]
pub struct Sizing {
    /// First-use size; `None` fits the window to its contents.
    pub default: Option<TextSize>,
    /// Smallest size the content still works at.
    pub min: TextSize,
}

/// A window the user opens from a menu and closes from its title bar.
pub struct Panel {
    title: &'static str,
    sizing: Sizing,
    open: bool,
}

impl Panel {
    pub const fn new(title: &'static str, sizing: Sizing) -> Self {
        Self {
            title,
            sizing,
            open: false,
        }
    }

    pub fn open(&mut self) {
        self.open = true;
    }

    /// Menu entry that opens the window.
    pub fn menu_item(&mut self, ui: &Ui) {
        if ui.menu_item(self.title) {
            self.open = true;
        }
    }

    /// Draws the window while it is open.
    pub fn show(&mut self, ui: &Ui, flags: WindowFlags, contents: impl FnOnce()) {
        if !self.open {
            return;
        }
        let min = self.sizing.min.px(ui);
        let mut window = ui
            .window(self.title)
            .size_constraints(min, [f32::MAX, f32::MAX]);
        if let Some(default) = self.sizing.default {
            window = window.size(default.px(ui), Condition::FirstUseEver);
        }
        window.flags(flags).opened(&mut self.open).build(contents);
    }
}
