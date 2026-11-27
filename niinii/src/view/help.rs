use crate::gui::px2;
use dear_imgui_rs::*;

use super::{mixins::wrap_bullet, panel::Panel, shortcuts::Action};

/// Usage notes, shortcuts and credits.
pub fn show(panel: &mut Panel, ui: &Ui) {
    panel.show(
        ui,
        |w| {
            w.size(px2(ui, [460.0, 420.0]), Condition::FirstUseEver)
                .size_constraints(px2(ui, [300.0, 200.0]), [f32::MAX, f32::MAX])
        },
        || {
                ui.text(concat!("niinii ", env!("CARGO_PKG_VERSION")));
                ui.text(concat!("By ", env!("CARGO_PKG_AUTHORS")));
                ui.text_link_open_url("github.com/Netdex/niinii", "https://github.com/Netdex/niinii");
                ui.text_disabled("Glossing and translation for reading Japanese text.");
                ui.separator();

                if ui.collapsing_header("Usage", TreeNodeFlags::DEFAULT_OPEN) {
                    wrap_bullet(ui, "With Options > Watch clipboard on, copied text (e.g. from a text hooker) is glossed. Options > Show input lets you type text instead.");
                    wrap_bullet(ui, "Hover a word for its dictionary entry; click it to keep the entry open in its own window.");
                    wrap_bullet(ui, "Translate sends the line to the translator configured under Options > Translator. Auto-translate translates every new line.");
                    wrap_bullet(ui, "The last lines are kept; step through them with the arrows in the menu bar, or open Options > Transcript to see them with their translations.");
                }

                if ui.collapsing_header("Keyboard shortcuts", TreeNodeFlags::DEFAULT_OPEN) {
                    ui.text_disabled("While a niinii window has focus and no text field is active.");
                    if let Some(_table) =
                        ui.begin_table_with_flags("shortcuts", 2, TableFlags::NO_SAVED_SETTINGS)
                    {
                        // Fixed with no width given: imgui fits it to the keys.
                        ui.table_setup_column(
                            "key",
                            TableColumnFlags::NONE,
                            Some(TableColumnWidth::Fixed(0.0)),
                        );
                        ui.table_setup_column(
                            "action",
                            TableColumnFlags::NONE,
                            Some(TableColumnWidth::Stretch(1.0)),
                        );
                        for action in Action::all() {
                            ui.table_next_column();
                            ui.text(action.key_name(ui));
                            ui.table_next_column();
                            ui.text_wrapped(action.description());
                        }
                        // Polled from the OS, so it works while the game has focus.
                        ui.table_next_column();
                        ui.text("ScrLk");
                        ui.table_next_column();
                        ui.text_wrapped("Toggle interaction (works while the game has focus)");
                    }
                }

                if ui.collapsing_header("Credits", TreeNodeFlags::DEFAULT_OPEN) {
                    wrap_bullet(ui, "Segmentation by Ichiran (github.com/tshatrov/ichiran).");
                    wrap_bullet(ui, "JMdict and KANJIDIC2 dictionary files are the property of the Electronic Dictionary Research and Development Group (edrdg.org), used in conformance with the Group's licence.");
                    wrap_bullet(ui, "Character data from VNDB (vndb.org).");
                    wrap_bullet(ui, "Third-party licenses are listed in NOTICE.");
                }
        },
    );
}
