use dear_imgui_rs::*;

use crate::{
    gui::{capture_theme, Frame},
    settings::Settings,
};

use super::{
    mixins::help_marker,
    panel::{Panel, Sizing, TextSize},
};

pub const SIZING: Sizing = Sizing {
    default: None,
    min: TextSize::new(44.0, 14.0),
};

pub fn show(panel: &mut Panel, ui: &Ui, frame: &mut Frame, settings: &mut Settings) {
    panel.show(ui, WindowFlags::MENU_BAR, || {
        ui.menu_bar(|| {
            if ui.menu_item("Save") {
                settings.theme = Some(capture_theme(&ui.clone_style(), frame.style_scale()));
                frame.theme_saved(settings.theme.clone());
            }
            if ui.menu_item("Reset") {
                settings.theme = None;
                frame.theme_saved(None);
            }
            if settings.theme.is_some() {
                ui.menu_with_enabled("Style saved", false, || {});
                help_marker(
                    ui,
                    "Saved style will be restored on start-up. Reset will clear the stored style.",
                );
            }
        });
        ui.show_default_style_editor();
    });
}
