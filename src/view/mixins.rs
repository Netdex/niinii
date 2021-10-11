use imgui::Ui;

pub fn help_marker(ui: &Ui, text: &str) {
    ui.text_colored([0.7, 0.7, 0.7, 1.0], "[?]");
    if ui.is_item_hovered() {
        ui.tooltip_text(text);
    }
}
