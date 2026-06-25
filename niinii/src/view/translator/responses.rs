use imgui::*;
use openai::chat::Role;

use crate::{
    settings::Settings,
    translator::{Response, ResponsesHandle},
    view::mixins::{
        checkbox_option, checkbox_option_with_default, combo_enum, combo_list, drag_handle,
        ellipses, help_marker,
    },
};

/// Render the responses backend's window body: latency-oriented tuning knobs,
/// the instructions editor, and a read-only readout of the server-side
/// conversation chain. There is no editable context buffer -- state lives on the
/// server via `previous_response_id`.
pub fn window(ui: &Ui, settings: &mut Settings, handle: &ResponsesHandle) {
    let state = handle.state();
    let resp = &mut settings.responses;

    ui.menu_bar(|| {
        if ui.menu_item("Reset conversation") {
            handle.reset_chain();
        }
    });

    if ui.collapsing_header("Tuning", TreeNodeFlags::DEFAULT_OPEN) {
        let align = 12.0;
        if let Some(_token) = ui.begin_table("##", 2) {
            ui.table_next_column();
            ui.set_next_item_width(ui.current_font_size() * -align);
            combo_list(ui, "Model", &state.models, &mut settings.openai_model);
            ui.table_next_column();
            ui.checkbox("Stream", &mut resp.stream);
            ui.same_line();
            help_marker(ui, "Stream output for lower time-to-first-token");
            ui.table_next_column();
            checkbox_option(ui, &mut resp.max_tokens, |ui, max_tokens| {
                ui.set_next_item_width(ui.current_font_size() * -align);
                ui.input_scalar("Max output tokens", max_tokens).build();
            });
            ui.table_next_column();
            checkbox_option_with_default(ui, &mut resp.temperature, 1.0, |ui, temperature| {
                ui.set_next_item_width(ui.current_font_size() * -align);
                ui.slider_config("Temperature", 0.0f32, 2.0f32)
                    .display_format("%.2f")
                    .flags(SliderFlags::ALWAYS_CLAMP)
                    .build(temperature);
            });
            ui.table_next_column();
            checkbox_option_with_default(ui, &mut resp.top_p, 1.0, |ui, top_p| {
                ui.set_next_item_width(ui.current_font_size() * -align);
                ui.slider_config("Top P", 0.0f32, 1.0f32)
                    .display_format("%.2f")
                    .flags(SliderFlags::ALWAYS_CLAMP)
                    .build(top_p);
            });
            ui.table_next_column();
            checkbox_option_with_default(
                ui,
                &mut resp.service_tier,
                openai::ServiceTier::Auto,
                |ui, service_tier| {
                    ui.set_next_item_width(ui.current_font_size() * -align);
                    combo_enum(ui, "Service tier", service_tier);
                },
            );
            ui.same_line();
            help_marker(ui, "Priority gives the lowest-latency serving tier");
            ui.table_next_column();
            checkbox_option_with_default(
                ui,
                &mut resp.reasoning_effort,
                openai::ReasoningEffort::Medium,
                |ui, reasoning_effort| {
                    ui.set_next_item_width(ui.current_font_size() * -align);
                    combo_enum(ui, "Reasoning effort", reasoning_effort);
                },
            );
            ui.same_line();
            help_marker(
                ui,
                "Low is efficient for most tasks; None for lowest latency (classification-style)",
            );
            ui.table_next_column();
            checkbox_option_with_default(
                ui,
                &mut resp.verbosity,
                openai::Verbosity::Medium,
                |ui, verbosity| {
                    ui.set_next_item_width(ui.current_font_size() * -align);
                    combo_enum(ui, "Verbosity", verbosity);
                },
            );
            ui.table_next_column();
            ui.checkbox("Chain conversation", &mut resp.chain);
            ui.same_line();
            help_marker(
                ui,
                "Keep server-side context across turns (previous_response_id). \
                 Off = translate each line independently; lowest latency/cost, no \
                 cross-line memory.",
            );
        }
    }

    let chain = state
        .previous_response_id
        .as_deref()
        .unwrap_or("(new conversation)");
    ui.text_disabled(format!("chain: {}", chain));

    // Mirror the chat backend's table layout (drag / delete / Lock / Role /
    // Message), but read-only: the instructions row, the VNDB addendum row, and
    // one row per server-side chain turn. There is no editable context buffer
    // (state lives server-side via `previous_response_id`), so the drag/delete/
    // lock controls are present-but-disabled for visual parity.
    ui.child_window("chain_window").build(|| {
        let Some(_t) = ui.begin_table_header_with_flags(
            "chain",
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

        // Instructions (system prompt). Resent every turn since the Responses
        // API does not inherit instructions across `previous_response_id`.
        static_lead(Role::System, "instr");
        ui.input_text_multiline(
            "##system_prompt",
            &mut resp.system_prompt,
            [ui.content_region_avail()[0], 140.0],
        )
        .build();

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
        }

        // One read-only row per chain turn.
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
                    "{}: {} in ({} cached) + {} out ({} reasoning) = {}",
                    ex.model.as_ref(),
                    u.input_tokens,
                    u.cached_tokens,
                    u.output_tokens,
                    u.reasoning_tokens,
                    u.total_tokens,
                ));
            }
        }
    });
}
