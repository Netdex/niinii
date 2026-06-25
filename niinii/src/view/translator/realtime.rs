use imgui::*;
use openai::chat::Role;

use crate::{
    settings::{Settings, TruncationMode},
    translator::{RealtimeHandle, Response},
    view::mixins::{
        checkbox_option, checkbox_option_with_default, combo_enum, combo_list, drag_handle,
        ellipses, help_marker,
    },
};

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

    if ui.collapsing_header("Tuning", TreeNodeFlags::DEFAULT_OPEN) {
        let align = 12.0;
        if let Some(_token) = ui.begin_table("##", 2) {
            ui.table_next_column();
            ui.set_next_item_width(ui.current_font_size() * -align);
            combo_list(ui, "Model", &state.models, &mut settings.openai_model);
            ui.same_line();
            help_marker(ui, "Requires a realtime-capable model (e.g. gpt-realtime)");
            ui.table_next_column();
            checkbox_option(ui, &mut rt.max_tokens, |ui, max_tokens| {
                ui.set_next_item_width(ui.current_font_size() * -align);
                ui.input_scalar("Max output tokens", max_tokens).build();
            });
            ui.table_next_column();
            checkbox_option_with_default(
                ui,
                &mut rt.reasoning_effort,
                openai::ReasoningEffort::Low,
                |ui, reasoning_effort| {
                    ui.set_next_item_width(ui.current_font_size() * -align);
                    combo_enum(ui, "Reasoning effort", reasoning_effort);
                },
            );
            ui.same_line();
            help_marker(
                ui,
                "Only for reasoning-capable realtime models (e.g. gpt-realtime-2); \
                 leave off for gpt-realtime. Lower effort = lower latency.",
            );
            ui.table_next_column();
            ui.checkbox("Chain conversation", &mut rt.chain);
            ui.same_line();
            help_marker(
                ui,
                "Keep the server-side conversation across turns. Off = reconnect \
                 per line and translate independently; no cross-line memory.",
            );
            ui.table_next_column();
            ui.set_next_item_width(ui.current_font_size() * -align);
            combo_enum(ui, "Truncation", &mut rt.truncation);
            ui.same_line();
            help_marker(
                ui,
                "When the conversation exceeds the model's input-token limit:\n\
                 - Auto: drop the oldest messages\n\
                 - Disabled: never truncate (the server errors instead)\n\
                 - Retention ratio: truncate early to keep a fraction of\n\
                 \x20 the max context (fewer truncations, better cache rate)\n\
                 Only matters when chaining.",
            );
            // The retention-ratio knobs stay visible at all times and are only
            // disabled when another mode is selected, so the layout never shifts.
            let is_retention = rt.truncation == TruncationMode::RetentionRatio;
            ui.table_next_column();
            ui.disabled(!is_retention, || {
                ui.set_next_item_width(ui.current_font_size() * -align);
                ui.slider_config("Retention ratio", 0.0f32, 1.0f32)
                    .display_format("%.2f")
                    .flags(SliderFlags::ALWAYS_CLAMP)
                    .build(&mut rt.truncation_retention_ratio);
            });
            ui.table_next_column();
            ui.disabled(!is_retention, || {
                checkbox_option_with_default(
                    ui,
                    &mut rt.truncation_post_instructions_token_limit,
                    4096,
                    |ui, limit| {
                        ui.set_next_item_width(ui.current_font_size() * -align);
                        ui.input_scalar("Token limit", limit).build();
                    },
                );
            });
            ui.same_line();
            help_marker(
                ui,
                "token_limits.post_instructions: cap on tokens kept\n\
                 after the instructions.\n\
                 Off = model default. Lower = truncate sooner but\n\
                 amortize message drops for a better cache rate.",
            );
        }
    }

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

    // Mirror the responses backend's read-only table layout (drag / delete /
    // Lock / Role / Message) for visual parity: the instructions row, the VNDB
    // addendum row, and one row per conversation turn. The Realtime API keeps
    // conversation state server-side, so the controls are present-but-disabled.
    ui.child_window("conversation_window").build(|| {
        let Some(_t) = ui.begin_table_header_with_flags(
            "conversation",
            [
                TableColumnSetup::new(""),
                TableColumnSetup::new(""),
                TableColumnSetup::new("Lock"),
                TableColumnSetup::new("Role"),
                TableColumnSetup::new("Message"),
            ],
            TableFlags::SIZING_STRETCH_PROP,
        ) else {
            return;
        };

        // Static row helper: disabled drag/delete/lock + a disabled role combo.
        // Leaves the Message column ready for the caller to fill.
        let static_lead = |role: Role, id: &str| {
            ui.table_next_column();
            ui.disabled(true, || drag_handle(ui));
            ui.table_next_column();
            ui.table_next_column();
            ui.disabled(true, || {
                let mut dummy = true;
                ui.checkbox(format!("##lock{id}"), &mut dummy);
            });
            ui.table_next_column();
            ui.disabled(true, || {
                ui.set_next_item_width(ui.current_font_size() * 6.0);
                let mut role = role;
                combo_enum(ui, format!("##role{id}"), &mut role);
            });
            ui.table_next_column();
        };

        // Instructions (system prompt). Sent in `session.update` each turn.
        static_lead(Role::System, "instr");
        ui.input_text_multiline(
            "##system_prompt",
            &mut rt.system_prompt,
            [ui.content_region_avail()[0], 140.0],
        )
        .build();
        ui.text_disabled(format!("~{sys_tokens} tokens"));

        // VNDB-derived addendum, read-only.
        if let Some(addendum) = state.system_addendum.as_deref() {
            static_lead(Role::System, "addendum");
            ui.child_window("##addendum")
                .size([ui.content_region_avail()[0], 200.0])
                .border(true)
                .build(|| {
                    let _wrap = ui.push_text_wrap_pos_with_pos(0.0);
                    ui.text_disabled(addendum);
                });
            ui.text_disabled(format!("~{addendum_tokens} tokens"));
        }

        // One read-only row per conversation turn.
        for (idx, ex) in state.exchanges.iter().enumerate() {
            let _id = ui.push_id_usize(idx);
            static_lead(Role::Assistant, "turn");
            let _wrap = ui.push_text_wrap_pos_with_pos(0.0);
            match &ex.response {
                Response::Streaming { content, .. } => {
                    ui.text_wrapped(format!("{}{}", content, ellipses(ui)));
                }
                Response::Completed { content, .. } => ui.text_wrapped(content),
                Response::Errored(err) => ui.text_disabled(format!("(error: {})", err)),
                Response::Cancelled => ui.text_disabled("(cancelled)"),
            }
            if let Some(u) = &ex.usage {
                ui.text_disabled(format!(
                    "{}: {} in ({} cached) + {} out = {}",
                    ex.model.as_ref(),
                    u.input_tokens,
                    u.cached_tokens,
                    u.output_tokens,
                    u.total_tokens,
                ));
            }
        }
    });
}
