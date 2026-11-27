use dear_imgui_rs::*;
use openai::chat::Role;

use crate::{
    gui::px,
    translator::ExchangeView,
    view::mixins::{ellipses, error_note},
};

use super::{read_only_text, usage_text, wrapped_editor};

/// A backend's conversation as the server keeps it: what is sent each turn and
/// what came back. Two columns, Role and Message, in a child window filling the
/// rest of the window. `error` is the backend's last error, shown at the end.
pub(super) fn history(ui: &Ui, id: &str, error: Option<&str>, rows: impl FnOnce(&Rows)) {
    ui.child_window(id).build(ui, || {
        if let Some(_table) = ui.begin_table_header_with_flags(
            "history",
            [
                TableColumnSetup::new("Role"),
                TableColumnSetup::new("Message"),
            ],
            TableOptions::default().sizing_policy(TableSizingPolicy::StretchProp),
        ) {
            rows(&Rows { ui });
        }
        if let Some(err) = error {
            error_note(ui, &format!("translator: {err}"));
        }
    });
}

pub(super) struct Rows<'ui> {
    ui: &'ui Ui,
}

impl Rows<'_> {
    fn start(&self, role: Role) {
        let ui = self.ui;
        ui.table_next_row();
        ui.table_next_column();
        ui.text_disabled(<&str>::from(&role));
        ui.table_next_column();
    }

    /// The system prompt, editable.
    pub fn instructions(&self, prompt: &mut String, tokens: Option<u32>) {
        let ui = self.ui;
        self.start(Role::System);
        wrapped_editor(ui, "##instructions", prompt, px(ui, 140.0));
        if let Some(tokens) = tokens {
            ui.text_disabled(format!("~{tokens} tokens"));
        }
    }

    /// The VNDB addendum appended to the instructions.
    pub fn addendum(&self, text: &str, tokens: Option<u32>) {
        let ui = self.ui;
        self.start(Role::System);
        read_only_text(ui, "##addendum", text, px(ui, 200.0));
        if let Some(tokens) = tokens {
            ui.text_disabled(format!("~{tokens} tokens"));
        }
    }

    pub fn turn(&self, ex: &ExchangeView) {
        let ui = self.ui;
        let _id = ui.push_id(ex.id.0 as usize);
        self.start(Role::Assistant);
        let _wrap = ui.push_text_wrap_pos(0.0);
        let content = ex.response.content();
        if matches!(ex.response, crate::translator::Response::Streaming { .. }) {
            ui.text_wrapped(format!("{content}{}", ellipses(ui)));
        } else if !content.is_empty() {
            ui.text_wrapped(content);
        }
        if let Some(problem) = ex.response.problem() {
            error_note(ui, &problem);
        }
        if let Some(usage) = &ex.usage {
            ui.text_disabled(usage_text(&ex.model, usage));
        }
    }
}
