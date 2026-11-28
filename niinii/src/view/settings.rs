use dear_file_browser::{Backend, DialogMode, FileDialog, FileDialogError};
use dear_imgui_reflect::{ImGuiReflectExt, ImGuiValue, Inspector, ReflectSession};
use dear_imgui_rs::*;

use crate::settings::{ContextLimit, FilePath, FolderPath, Secret, Settings};

use super::{
    mixins::error_note,
    panel::{Panel, Sizing, TextSize},
};

pub const SIZING: Sizing = Sizing {
    default: Some(TextSize::new(62.0, 24.0)),
    min: TextSize::new(48.0, 12.0),
};

/// The editor configuration all reflected settings views share.
pub fn reflect_session() -> ReflectSession {
    let mut session = ReflectSession::default();
    session.settings_mut().options_mut().inline = true;
    session
}

pub fn show(panel: &mut Panel, ui: &Ui, reflect: &ReflectSession, settings: &mut Settings) {
    panel.show(ui, WindowFlags::empty(), || {
        let mut inspector = ui.inspector(reflect);
        inspector.input("Ichiran", &mut settings.ichiran);
        inspector.input("Text", &mut settings.text);
        inspector.input("Interface", &mut settings.interface);
        inspector.input("Translation", &mut settings.translation);
        if cfg!(feature = "voicevox") {
            inspector.input("Text-to-speech", &mut settings.tts);
        }
        inspector.input("Windows", &mut settings.window);
        ui.separator();
        ui.text_disabled("* Restart to apply these changes");
    });
}

impl ImGuiValue for FilePath {
    fn imgui_value(inspector: &mut Inspector<'_, '_>, label: &str, value: &mut Self) -> bool {
        path_field(
            inspector.ui(),
            label,
            &mut value.path,
            &mut value.pick_error,
            DialogMode::OpenFile,
        )
    }
}

impl ImGuiValue for FolderPath {
    fn imgui_value(inspector: &mut Inspector<'_, '_>, label: &str, value: &mut Self) -> bool {
        path_field(
            inspector.ui(),
            label,
            &mut value.path,
            &mut value.pick_error,
            DialogMode::PickFolder,
        )
    }
}

impl ImGuiValue for ContextLimit {
    fn imgui_value(inspector: &mut Inspector<'_, '_>, label: &str, value: &mut Self) -> bool {
        inspector.ui().input_scalar_n(label, &mut value.0).build()
    }
}

impl ImGuiValue for Secret {
    fn imgui_value(inspector: &mut Inspector<'_, '_>, label: &str, value: &mut Self) -> bool {
        inspector
            .ui()
            .input_text(label, &mut value.0)
            .password(true)
            .build()
    }
}

/// A path typed in, or picked with the system dialog for `mode`. Shows its
/// end when too long, since that is the part that differs.
fn path_field(
    ui: &Ui,
    label: &str,
    path: &mut String,
    pick_error: &mut Option<String>,
    mode: DialogMode,
) -> bool {
    let _id = ui.push_id(label);
    let browse = "Browse...";
    let style = unsafe { ui.style() };
    let browse_width = ui.calc_text_size(browse)[0] + 2.0 * style.frame_padding()[0];
    let field_width = ui.calc_item_width() - browse_width - style.item_inner_spacing()[0];

    ui.set_next_item_width(field_width);
    let mut changed = ui
        .input_text("##path", path)
        .flags(InputTextFlags::ELIDE_LEFT)
        .build();
    ui.same_line_with_spacing(0.0, style.item_inner_spacing()[0]);
    if ui.button(browse) {
        let mut dialog = FileDialog::new(mode).backend(Backend::Native);
        if let Some(dir) = std::path::Path::new(path.as_str()).parent() {
            dialog = dialog.directory(dir);
        }
        match dialog.open_blocking() {
            Ok(selection) => {
                if let Some(picked) = selection.paths().first() {
                    *path = picked.to_string_lossy().into_owned();
                    *pick_error = None;
                    changed = true;
                }
            }
            Err(FileDialogError::Cancelled) => {}
            Err(err) => *pick_error = Some(format!("could not open the dialog: {err}")),
        }
    }
    let visible = label.split("##").next().unwrap_or(label);
    if !visible.is_empty() {
        ui.same_line_with_spacing(0.0, style.item_inner_spacing()[0]);
        ui.text(visible);
    }
    if let Some(err) = pick_error {
        error_note(ui, err);
    }
    changed
}
