use imgui::*;

use super::mixins::wrap_bullet;

/// Keep in sync with `App::handle_shortcuts`.
const SHORTCUTS: &[(&str, &str)] = &[
    ("T", "Translate the shown line"),
    ("S", "Speak the shown line (VOICEVOX builds)"),
    ("R", "Cycle ruby text: none, furigana, romaji"),
    ("Left / Right", "Previous / next line"),
    ("End", "Latest line"),
    (
        "ScrLk",
        "Toggle interaction (works while the game has focus)",
    ),
];

/// Usage notes, shortcuts and credits.
pub struct HelpView {
    pub open: bool,
}

impl HelpView {
    pub fn new() -> Self {
        Self { open: false }
    }

    pub fn show_menu_item(&mut self, ui: &Ui) {
        if ui.menu_item("About niinii") {
            self.open = true;
        }
    }

    pub fn ui(&mut self, ui: &Ui) {
        if !self.open {
            return;
        }
        ui.window("About niinii")
            .size([460.0, 420.0], Condition::FirstUseEver)
            .opened(&mut self.open)
            .build(|| {
                ui.text(concat!("niinii ", env!("CARGO_PKG_VERSION")));
                ui.text_disabled("Glossing and translation for reading Japanese text.");
                ui.text(concat!("By ", env!("CARGO_PKG_AUTHORS")));
                ui.text_disabled("github.com/Netdex/niinii");
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
                        ui.table_setup_column_with(TableColumnSetup {
                            // Fixed with no width given: imgui fits it to the keys.
                            flags: TableColumnFlags::WIDTH_FIXED,
                            ..TableColumnSetup::new("key")
                        });
                        ui.table_setup_column_with(TableColumnSetup {
                            flags: TableColumnFlags::WIDTH_STRETCH,
                            ..TableColumnSetup::new("action")
                        });
                        for (key, action) in SHORTCUTS {
                            ui.table_next_column();
                            ui.text(key);
                            ui.table_next_column();
                            ui.text_wrapped(action);
                        }
                    }
                }

                if ui.collapsing_header("Credits", TreeNodeFlags::DEFAULT_OPEN) {
                    wrap_bullet(ui, "Segmentation by Ichiran (github.com/tshatrov/ichiran).");
                    wrap_bullet(ui, "JMdict and KANJIDIC2 dictionary files are the property of the Electronic Dictionary Research and Development Group (edrdg.org), used in conformance with the Group's licence.");
                    wrap_bullet(ui, "Character data from VNDB (vndb.org).");
                    wrap_bullet(ui, "Third-party licenses are listed in NOTICE.");
                }
            });
    }
}
