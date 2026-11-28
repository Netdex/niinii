use dear_imgui_rs::*;

/// Commands with a keyboard shortcut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Translate,
    Speak,
    CycleRubyText,
    PreviousLine,
    NextLine,
    LatestLine,
}

impl Action {
    const ALL: [Action; 6] = [
        Action::Translate,
        Action::Speak,
        Action::CycleRubyText,
        Action::PreviousLine,
        Action::NextLine,
        Action::LatestLine,
    ];

    /// The actions this build supports.
    pub fn all() -> impl Iterator<Item = Action> {
        Self::ALL
            .into_iter()
            .filter(|action| *action != Action::Speak || cfg!(feature = "voicevox"))
    }

    fn key(self) -> Key {
        match self {
            Action::Translate => Key::T,
            Action::Speak => Key::S,
            Action::CycleRubyText => Key::R,
            Action::PreviousLine => Key::LeftArrow,
            Action::NextLine => Key::RightArrow,
            Action::LatestLine => Key::End,
        }
    }

    pub fn key_name(self, ui: &Ui) -> &str {
        ui.get_key_name(self.key())
    }

    pub fn description(self) -> &'static str {
        match self {
            Action::Translate => "Translate the shown line",
            Action::Speak => "Speak the shown line",
            Action::CycleRubyText => "Cycle ruby text: none, furigana, romaji",
            Action::PreviousLine => "Previous line",
            Action::NextLine => "Next line",
            Action::LatestLine => "Latest line",
        }
    }

    /// Actions whose shortcut was pressed this frame. Keys act while any
    /// niinii window has focus; ImGui holds back plain keys while a text
    /// field is taking input.
    pub fn pressed(ui: &Ui) -> Vec<Action> {
        Self::all()
            .filter(|action| {
                let mut flags = ShortcutFlags::NONE;
                // Held arrows step through lines.
                if matches!(action, Action::PreviousLine | Action::NextLine) {
                    flags |= ShortcutFlags::REPEAT;
                }
                ui.shortcut_with_flags(
                    KeyChord::new(action.key()),
                    ShortcutOptions::new()
                        .flags(flags)
                        .route(ShortcutRoute::Global(ShortcutGlobalRouteFlags::NONE)),
                )
            })
            .collect()
    }

    /// A menu item that runs this action, labelled with its shortcut.
    pub fn menu_item(self, ui: &Ui, label: &str) -> bool {
        ui.menu_item_with_shortcut(label, self.key_name(ui))
    }
}
