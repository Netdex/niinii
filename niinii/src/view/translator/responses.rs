use dear_imgui_reflect::{ImGuiReflectExt, ReflectSession};
use dear_imgui_rs::*;

use crate::{settings::Settings, translator::ResponsesHandle, view::mixins::combo_list};

use super::history::history;

/// Render the responses backend's window body: latency-oriented tuning knobs,
/// and the server-side conversation chain with its editable instructions.
pub fn window(
    ui: &Ui,
    reflect: &ReflectSession,
    settings: &mut Settings,
    handle: &ResponsesHandle,
) {
    let state = handle.state();
    let resp = &mut settings.responses;

    ui.menu_bar(|| {
        if ui.menu_item("Reset conversation") {
            handle.reset_chain();
        }
    });

    combo_list(
        ui,
        "Model",
        &state.models,
        &mut settings.translation.openai_model,
    );
    ui.inspector(reflect).input("Tuning", resp);

    let chain = state
        .previous_response_id
        .as_deref()
        .unwrap_or("(new conversation)");
    ui.text_disabled(format!("chain: {}", chain));

    history(ui, "chain", state.last_error.as_deref(), |rows| {
        // Resent every turn: the Responses API does not inherit instructions
        // across `previous_response_id`.
        rows.instructions(&mut resp.system_prompt, None);
        if let Some(addendum) = state.system_addendum.as_deref() {
            rows.addendum(addendum, None);
        }
        for ex in &state.exchanges {
            rows.turn(ex);
        }
    });
}
