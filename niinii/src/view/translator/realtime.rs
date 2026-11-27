use dear_imgui_reflect::{ImGuiReflectExt, ReflectSession};
use dear_imgui_rs::*;

use crate::{
    settings::Settings,
    translator::RealtimeHandle,
    view::mixins::{combo_list, help_marker},
};

use super::history::history;

/// Memoized token counts for the instruction blocks (system prompt + VNDB
/// addendum). Counting runs only when the text changes, not every frame.
#[derive(Default)]
pub struct InstrTokens {
    system_prompt: (String, u32),
    addendum: (String, u32),
}

impl InstrTokens {
    /// Token count for `text`, recomputed only when `text` differs from the last
    /// value stored in `slot`.
    fn count(slot: &mut (String, u32), text: &str) -> u32 {
        if slot.0 != text {
            slot.0 = text.to_owned();
            slot.1 = openai::estimate_text_tokens(text);
        }
        slot.1
    }

    fn system_prompt(&mut self, text: &str) -> u32 {
        Self::count(&mut self.system_prompt, text)
    }

    fn addendum(&mut self, text: &str) -> u32 {
        Self::count(&mut self.addendum, text)
    }
}

/// Render the realtime backend's window body: the slim session knobs, the
/// instructions editor, and a read-only readout of the server-side conversation.
/// The Realtime API is a stateful, always-streaming WebSocket session, so there
/// is no editable context buffer and no stream/service-tier/reasoning knobs.
pub fn window(
    ui: &Ui,
    reflect: &ReflectSession,
    settings: &mut Settings,
    handle: &RealtimeHandle,
    instr_tokens: &mut InstrTokens,
) {
    let state = handle.state();
    let rt = &mut settings.realtime;

    ui.menu_bar(|| {
        if ui.menu_item("Reset conversation") {
            handle.reset_conversation();
        }
    });

    combo_list(
        ui,
        "Model",
        &state.models,
        &mut settings.translation.openai_model,
    );
    ui.same_line();
    help_marker(ui, "Requires a realtime-capable model (e.g. gpt-realtime)");
    ui.inspector(reflect).input("Tuning", rt);

    // Instruction token counts. `post_instructions` caps only the conversation
    // *after* the instructions, so the system prompt + VNDB addendum ride on top
    // of that limit -- surfacing their size explains why total input exceeds the
    // configured limit.
    let sys_tokens = instr_tokens.system_prompt(&rt.system_prompt);
    let addendum_tokens = state
        .system_addendum
        .as_deref()
        .map(|a| instr_tokens.addendum(a))
        .unwrap_or(0);
    let instr_tokens_total = sys_tokens + addendum_tokens;

    let status = if state.connected {
        "connected"
    } else {
        "disconnected"
    };
    ui.text_disabled(format!(
        "session: {}   |   instructions: ~{} tokens (added on top of the post-instructions limit)",
        status, instr_tokens_total
    ));

    history(ui, "conversation", state.last_error.as_deref(), |rows| {
        // Sent in `session.update` each turn.
        rows.instructions(&mut rt.system_prompt, Some(sys_tokens));
        if let Some(addendum) = state.system_addendum.as_deref() {
            rows.addendum(addendum, Some(addendum_tokens));
        }
        for ex in &state.exchanges {
            rows.turn(ex);
        }
    });
}
