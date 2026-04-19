use imgui::*;

use crate::settings::Settings;

use super::mixins::help_marker;

#[derive(Default)]
pub struct StyleEditor {
    pub open: bool,
}

impl StyleEditor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn show_menu_item(&mut self, ui: &Ui) {
        if ui.menu_item("Style Editor") {
            self.open = true;
        }
    }

    pub fn ui(&mut self, ui: &Ui, settings: &mut Settings) {
        if !self.open {
            return;
        }
        let Some(_window) = ui
            .window("Style Editor")
            .opened(&mut self.open)
            .menu_bar(true)
            .begin()
        else {
            return;
        };
        ui.menu_bar(|| {
            if ui.menu_item("Save") {
                settings.set_style(Some(&ui.clone_style()));
            }
            if ui.menu_item("Reset") {
                settings.set_style(None);
            }
            if settings.style.is_some() {
                ui.menu_with_enabled("Style saved", false, || {});
                help_marker(
                    ui,
                    "Saved style will be restored on start-up. Reset will clear the stored style.",
                );
            }
        });
        ui.show_default_style_editor();
    }
}
