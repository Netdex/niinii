use imgui::*;

use super::{gloss::GlossView, translator::TranslatorWindow};

/// Recent lines with their translations, oldest first. Clicking a row shows
/// that line in the main view.
pub struct TranscriptView {
    pub open: bool,
    latest_line: Option<u64>,
}

impl TranscriptView {
    pub fn new() -> Self {
        Self {
            open: false,
            latest_line: None,
        }
    }

    pub fn show_menu_item(&mut self, ui: &Ui) {
        if ui.menu_item("Transcript") {
            self.open = true;
        }
    }

    pub fn ui(&mut self, ui: &Ui, gloss: &mut GlossView, translator: &TranslatorWindow) {
        if !self.open {
            return;
        }
        let Some(_window) = ui
            .window("Transcript")
            .size([640.0, 400.0], Condition::FirstUseEver)
            .opened(&mut self.open)
            .begin()
        else {
            return;
        };

        let flags = TableFlags::RESIZABLE
            | TableFlags::ROW_BG
            | TableFlags::BORDERS_INNER_V
            | TableFlags::SCROLL_Y
            | TableFlags::SIZING_STRETCH_PROP;
        // -FLT_MIN height fills the rest of the window.
        let Some(_table) =
            ui.begin_table_with_sizing("transcript", 3, flags, [0.0, -f32::MIN_POSITIVE], 0.0)
        else {
            return;
        };
        ui.table_setup_scroll_freeze(0, 1);
        ui.table_setup_column_with(TableColumnSetup {
            flags: TableColumnFlags::WIDTH_FIXED,
            ..TableColumnSetup::new("#")
        });
        ui.table_setup_column("Japanese");
        ui.table_setup_column("English");
        ui.table_headers_row();

        let (pos, _) = gloss.history_position();
        let shown = pos.saturating_sub(1);
        let latest = gloss.latest_line_id();
        let new_line = latest != self.latest_line;
        self.latest_line = latest;
        // Follow new lines only while scrolled to the bottom, so reading back
        // isn't interrupted.
        let at_bottom = ui.scroll_y() >= ui.scroll_max_y() - 1.0;

        let mut jump = None;
        for (idx, text, translation) in gloss.lines() {
            let _id = ui.push_id_usize(idx);
            ui.table_next_row();
            ui.table_next_column();
            let top = ui.cursor_screen_pos();
            ui.text_disabled(format!("{}", idx + 1));
            let mut bottom = ui.item_rect_max()[1];

            ui.table_next_column();
            ui.text_wrapped(text);
            bottom = bottom.max(ui.item_rect_max()[1]);

            ui.table_next_column();
            if let Some(t) = translation.and_then(|id| translator.translation_text(id)) {
                ui.text_wrapped(t);
                bottom = bottom.max(ui.item_rect_max()[1]);
            }

            // A selectable is one line tall by default; the cells wrap, so it
            // is laid over the row afterwards at the tallest cell's height.
            // Spanning all columns, imgui draws its highlight behind the cells.
            ui.table_set_column_index(0);
            ui.set_cursor_screen_pos(top);
            if ui
                .selectable_config("##row")
                .selected(idx == shown)
                .flags(SelectableFlags::SPAN_ALL_COLUMNS | SelectableFlags::ALLOW_ITEM_OVERLAP)
                .size([0.0, bottom - top[1]])
                .build()
            {
                jump = Some(idx);
            }
        }
        if new_line && at_bottom {
            ui.set_scroll_here_y_with_ratio(1.0);
        }
        if let Some(idx) = jump {
            gloss.show_line(idx);
        }
    }
}
