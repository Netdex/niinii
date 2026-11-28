use dear_imgui_reflect::{ImGuiReflectExt, ReflectSession};
use std::collections::HashMap;

use crate::gui::px;
use dear_imgui_rs::*;
use openai::chat::{Message, Role};

use crate::{
    settings::Settings,
    translator::{chat::LINE_PREFIX, ChatHandle, ContextEdit, MsgId},
    view::mixins::{
        combo_enum_fit, combo_list, drag_handle, error_note, form_row, wrapped_field_height,
    },
};

use super::{read_only_text, wrapped_editor};

/// Render the chat backend's window body: tuning knobs plus the editable context
/// buffer. The window framing is owned by the caller (`TranslatorWindow::ui`).
pub fn window(
    ui: &Ui,
    reflect: &ReflectSession,
    settings: &mut Settings,
    handle: &ChatHandle,
    buffers: &mut HashMap<MsgId, String>,
) {
    let state = handle.state();
    let chatgpt = &mut settings.chat;

    ui.menu_bar(|| {
        if ui.menu_item("Clear") {
            handle.clear_context();
        }
    });

    form_row(ui, "Model", || {
        combo_list(
            ui,
            "##model",
            &state.models,
            &mut settings.translation.openai_model,
        )
    });
    ui.inspector(reflect).input("Tuning", chatgpt);

    ui.child_window("context_window").build(ui, || {
        if let Some(_t) = ui.begin_table_header_with_flags(
            "context",
            [
                TableColumnSetup::new(""),
                TableColumnSetup::new(""),
                TableColumnSetup::new("Lock"),
                TableColumnSetup::new("Role"),
                TableColumnSetup::new("Message"),
            ],
            TableOptions::default().sizing_policy(TableSizingPolicy::StretchProp),
        ) {
            // Rows for the parts of the system message, in the order it is
            // assembled.
            {
                let _id = ui.push_id("system_prompt");
                fixed_row_lead(ui, Role::System);
                wrapped_editor(ui, "##text", &mut chatgpt.system_prompt, px(ui, 200.0));
            }
            if let Some(addendum) = state.system_addendum.as_deref() {
                static_row(ui, "addendum", Role::System, addendum, px(ui, 200.0));
            }
            // Prepended to every unlocked User row below and to the line
            // being translated.
            static_row(ui, "line_prefix", Role::User, LINE_PREFIX, px(ui, 200.0));

            // Sweep buffers whose messages are gone; keep the rest. Stable
            // MsgIds make this robust to reorder/insert/delete.
            let live: std::collections::HashSet<MsgId> =
                state.context.iter().map(|e| e.id).collect();
            buffers.retain(|id, _| live.contains(id));

            enum Interaction {
                Delete(usize),
                Swap(usize, usize),
                SetRole(usize, Role),
                SetName(usize, Option<String>),
                CommitContent(usize, String),
            }
            let mut interact: Option<Interaction> = None;

            for (idx, entry) in state.context.iter().enumerate() {
                let message = &entry.message;
                let _id = ui.push_id(idx);
                ui.table_next_column();
                drag_handle(ui);
                if let Some(_tooltip) = ui
                    .drag_drop_source_config("##dd_message")
                    .flags(DragDropSourceFlags::ALLOW_NULL_ID)
                    .begin_payload(idx)
                {
                    ui.text(format!("{:?}", message));
                }
                if let Some(target) = ui.drag_drop_target() {
                    if let Some(Ok(payload)) =
                        target.accept_payload::<usize, _>("##dd_message", DragDropTargetFlags::NONE)
                    {
                        interact = Some(Interaction::Swap(payload.data, idx));
                    }
                }
                ui.table_next_column();
                if ui.button_with_size("\u{00d7}", [ui.frame_height(), 0.0]) {
                    interact = Some(Interaction::Delete(idx));
                }
                ui.table_next_column();
                let mut lock = message.name.is_some();
                {
                    let _disabled = ui.begin_disabled_with_cond(message.role != Role::User);
                    if ui.checkbox("##lock", &mut lock) {
                        interact =
                            Some(Interaction::SetName(idx, lock.then(|| "info".to_string())));
                    }
                }
                ui.table_next_column();
                ui.group(|| {
                    let mut role = message.role.clone();
                    combo_enum_fit(ui, "##role", &mut role);
                    if role != message.role {
                        interact = Some(Interaction::SetRole(idx, role));
                    }
                });
                ui.table_next_column();
                let buf = buffers
                    .entry(entry.id)
                    .or_insert_with(|| message.content.clone().unwrap_or_default());
                let height = wrapped_field_height(ui, buf, px(ui, 160.0));
                wrapped_editor(ui, "##content", buf, height);
                if ui.is_item_deactivated_after_edit() {
                    interact = Some(Interaction::CommitContent(idx, buf.clone()));
                }
            }

            match interact {
                Some(Interaction::Delete(idx)) => {
                    handle.edit_context(ContextEdit::Delete(idx));
                }
                Some(Interaction::Swap(a, b)) => {
                    handle.edit_context(ContextEdit::Swap(a, b));
                }
                Some(Interaction::SetRole(idx, role)) => {
                    handle.edit_context(ContextEdit::SetRole { idx, role });
                }
                Some(Interaction::SetName(idx, name)) => {
                    handle.edit_context(ContextEdit::SetName { idx, name });
                }
                Some(Interaction::CommitContent(idx, content)) => {
                    handle.edit_context(ContextEdit::SetContent { idx, content });
                }
                None => {}
            }

            ui.table_next_column();
            ui.table_next_column();
            if ui.button_with_size("+", [ui.frame_height(), 0.0]) {
                handle.edit_context(ContextEdit::Insert {
                    idx: state.context.len(),
                    message: Message {
                        content: Some(String::new()),
                        ..Default::default()
                    },
                });
            }
            ui.table_next_column();
            ui.table_next_column();
            ui.table_next_column();
            ui.text_disabled("drag and drop by handle to reorder");
        }
        if let Some(err) = state.last_error.as_deref() {
            error_note(ui, &format!("translator: {err}"));
        }
    });
}

/// The leading cells of a fixed row: disabled drag handle, no delete button,
/// locked, and a fixed role. The caller fills the last cell.
fn fixed_row_lead(ui: &Ui, role: Role) {
    ui.table_next_column();
    {
        let _disabled = ui.begin_disabled_with_cond(true);
        drag_handle(ui);
    }
    ui.table_next_column();
    ui.table_next_column();
    {
        let _disabled = ui.begin_disabled_with_cond(true);
        let mut dummy = true;
        ui.checkbox("##lock", &mut dummy);
    }
    ui.table_next_column();
    {
        let _disabled = ui.begin_disabled_with_cond(true);
        let mut role = role;
        combo_enum_fit(ui, "##role", &mut role);
    }
    ui.table_next_column();
}

/// A read-only row in the context table for text the app adds itself.
fn static_row(ui: &Ui, id: &str, role: Role, text: &str, max_height: f32) {
    let _id = ui.push_id(id);
    fixed_row_lead(ui, role);
    read_only_text(ui, "##text", text, max_height);
}
