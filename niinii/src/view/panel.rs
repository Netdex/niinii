use dear_imgui_rs::{Ui, Window};

/// A window the user opens from a menu and closes from its title bar.
pub struct Panel {
    title: &'static str,
    open: bool,
}

impl Panel {
    pub const fn new(title: &'static str) -> Self {
        Self { title, open: false }
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

    /// Draws the window while it is open. `configure` sets up the window
    /// (size, flags) before it begins.
    pub fn show<'ui>(
        &'ui mut self,
        ui: &'ui Ui,
        configure: impl FnOnce(Window<'ui>) -> Window<'ui>,
        contents: impl FnOnce(),
    ) {
        if self.open {
            configure(ui.window(self.title))
                .opened(&mut self.open)
                .build(contents);
        }
    }
}
