use dear_imgui_rs::*;

use super::{
    panel::{Panel, Sizing, TextSize},
    translator::TranslatorWindow,
};
use crate::history::History;

pub const SIZING: Sizing = Sizing {
    default: Some(TextSize::new(71.0, 18.0)),
    min: TextSize::new(44.0, 8.0),
};

/// Recent lines with their translations, oldest first. Clicking a row shows
/// that line in the main view.
pub struct TranscriptView {
    latest_line: Option<u64>,
}

impl Default for TranscriptView {
    fn default() -> Self {
        Self::new()
    }
}

impl TranscriptView {
    pub fn new() -> Self {
        Self { latest_line: None }
    }

    pub fn show(
        &mut self,
        panel: &mut Panel,
        ui: &Ui,
        history: &mut History,
        translator: &TranslatorWindow,
    ) {
        panel.show(ui, WindowFlags::empty(), || {
            self.table(ui, history, translator)
        });
    }

    fn table(&mut self, ui: &Ui, history: &mut History, translator: &TranslatorWindow) {
        let flags = TableOptions::default()
            .flags(
                TableFlags::RESIZABLE
                    | TableFlags::ROW_BG
                    | TableFlags::BORDERS_INNER_V
                    | TableFlags::SCROLL_Y,
            )
            .sizing_policy(TableSizingPolicy::StretchProp);
        // -FLT_MIN height fills the rest of the window.
        let Some(_table) =
            ui.begin_table_with_sizing("transcript", 3, flags, [0.0, -f32::MIN_POSITIVE], 0.0)
        else {
            return;
        };
        ui.table_setup_scroll_freeze(0, 1);
        ui.table_setup_column(
            "#",
            TableColumnFlags::NONE,
            Some(TableColumnWidth::Fixed(0.0)),
        );
        ui.table_setup_column("Japanese", TableColumnFlags::NONE, None);
        ui.table_setup_column("English", TableColumnFlags::NONE, None);
        ui.table_headers_row();

        let (pos, _) = history.position();
        let shown = pos.saturating_sub(1);
        let latest = history.latest_line_id();
        let new_line = latest != self.latest_line;
        self.latest_line = latest;
        // Follow new lines only while scrolled to the bottom, so reading back
        // isn't interrupted.
        let at_bottom = ui.scroll_y() >= ui.scroll_max_y() - 1.0;

        let mut jump = None;
        for (idx, line) in history.lines().enumerate() {
            let _id = ui.push_id(idx);
            ui.table_next_row();
            ui.table_next_column();
            let top = ui.cursor_screen_pos();
            ui.text_disabled(format!("{}", idx + 1));
            let mut bottom = ui.item_rect_max()[1];

            ui.table_next_column();
            ui.text_wrapped(&line.text);
            bottom = bottom.max(ui.item_rect_max()[1]);

            ui.table_next_column();
            if let Some(t) = line
                .translation
                .and_then(|id| translator.translation_text(id))
            {
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
                .flags(SelectableFlags::SPAN_ALL_COLUMNS | SelectableFlags::ALLOW_OVERLAP)
                .size([0.0, bottom - top[1]])
                .build()
            {
                jump = Some(idx);
            }
        }
        if new_line && at_bottom {
            ui.set_scroll_here_y(1.0);
        }
        if let Some(idx) = jump {
            history.show(idx);
        }
    }
}
