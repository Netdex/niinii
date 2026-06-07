use imgui::*;

use std::sync::Arc;

use ichiran::prelude::Ichiran;

use crate::{
    settings::Settings,
    translator::chat::ChatHandle,
    vndb::{self, clean_description, SearchParams, SortKey, VndbHandle, VnSummary},
};

const SORT_OPTIONS: &[(SortKey, &str)] = &[
    (SortKey::SearchRank, "search rank"),
    (SortKey::Title, "title"),
    (SortKey::Released, "released"),
    (SortKey::Rating, "rating"),
    (SortKey::VoteCount, "vote count"),
    (SortKey::Popularity, "popularity"),
];

const LENGTH_OPTIONS: &[(u8, &str)] = &[
    (1, "very short"),
    (2, "short"),
    (3, "medium"),
    (4, "long"),
    (5, "very long"),
];

const LANG_OPTIONS: &[&str] = &[
    "en", "ja", "zh-Hans", "zh-Hant", "ko", "fr", "de", "es", "it", "ru", "pt-br", "pt-pt", "nl",
    "pl", "sv", "fi", "tr", "vi", "th", "id", "ar", "cs", "hu", "uk",
];

const PLATFORM_OPTIONS: &[&str] = &[
    "win", "lin", "mac", "web", "ios", "and", "swi", "ps5", "ps4", "ps3", "ps2", "ps1", "psp",
    "psv", "n3d", "nds", "xb1", "xbo", "x36", "xbx", "dvd", "drc", "dos", "fmt", "mob", "oth",
];

pub struct VndbView {
    pub open: bool,
    params: SearchParams,
    vndb: VndbHandle,
}

impl VndbView {
    /// Create the view and spawn the VNDB writer task.
    ///
    /// `chat_handle` receives the active VN's prompt fragment via
    /// `set_system_addendum`. `ichiran` is used to push character
    /// names into the dictionary whenever the active VN changes, so
    /// the segmenter recognises them as proper nouns.
    pub fn new(chat_handle: ChatHandle, ichiran: Ichiran, settings: &Settings) -> Self {
        let chat = chat_handle.clone();
        let vndb = vndb::spawn(
            move |prompt: Option<Arc<str>>| {
                chat.set_system_addendum(prompt);
            },
            ichiran,
        );
        if let Some(id) = settings.vndb_active_id.clone() {
            vndb.set_active_by_id(id);
        }
        Self {
            open: false,
            params: SearchParams::default(),
            vndb,
        }
    }

    pub fn show_menu_item(&mut self, ui: &Ui) {
        if ui.menu_item("VNDB") {
            self.open = true;
        }
    }

    pub fn ui(&mut self, ui: &Ui, settings: &mut Settings) {
        if !self.open {
            return;
        }
        let Some(_window) = ui
            .window("VNDB")
            .size_constraints([720.0, 400.0], [1600.0, 1600.0])
            .opened(&mut self.open)
            .begin()
        else {
            return;
        };
        let vndb = &self.vndb;
        let state = vndb.state();

        // Search row.
        ui.set_next_item_width(-160.0);
        let submitted = ui
            .input_text("##query", &mut self.params.query)
            .enter_returns_true(true)
            .hint("Search visual novel...")
            .build();
        ui.same_line();
        let clicked = ui.button_with_size("Search", [140.0, 0.0]);
        if (submitted || clicked) && !self.params.query.trim().is_empty() {
            vndb.search(self.params.clone());
        }

        // Filter row -- explicit pixel widths so the multi-selects don't
        // get squished by the layout engine.
        multi_select_str(ui, "lang", LANG_OPTIONS, &mut self.params.langs, 180.0);
        ui.same_line();
        multi_select_str(
            ui,
            "platform",
            PLATFORM_OPTIONS,
            &mut self.params.platforms,
            220.0,
        );
        ui.same_line();
        multi_select_length(ui, &mut self.params.lengths, 160.0);
        ui.same_line();
        ui.set_next_item_width(140.0);
        sort_combo(ui, &mut self.params.sort);
        ui.same_line();
        ui.checkbox("reverse", &mut self.params.reverse);
        if state.searching {
            ui.same_line();
            ui.text_disabled("searching...");
        }

        // Active VN block.
        if let Some(active) = &state.active {
            ui.separator();
            ui.text_colored([0.6, 0.9, 0.6, 1.0], &active.summary.title);
            ui.same_line();
            ui.text_disabled(format!("[{}]", active.summary.id));
            ui.same_line();
            if ui.small_button("Clear") {
                vndb.clear_active();
                settings.vndb_active_id = None;
            }
            draw_vn_details(ui, &active.summary);
            if state.loading_active {
                ui.text_disabled("loading characters...");
            } else {
                ui.text_disabled(format!("{} characters", active.characters.len()));
            }
        }

        if let Some(err) = &state.last_error {
            ui.text_colored([1.0, 0.4, 0.4, 1.0], format!("error: {}", err));
        }

        ui.separator();

        let table_flags = TableFlags::BORDERS_INNER_H
            | TableFlags::ROW_BG
            | TableFlags::SCROLL_Y
            | TableFlags::SIZING_STRETCH_PROP;
        let columns = [
            TableColumnSetup {
                name: "id",
                flags: TableColumnFlags::WIDTH_FIXED,
                init_width_or_weight: 56.0,
                user_id: Id::default(),
            },
            TableColumnSetup {
                name: "title",
                flags: TableColumnFlags::WIDTH_STRETCH,
                init_width_or_weight: 3.0,
                user_id: Id::default(),
            },
            TableColumnSetup {
                name: "released",
                flags: TableColumnFlags::WIDTH_FIXED,
                init_width_or_weight: 80.0,
                user_id: Id::default(),
            },
            TableColumnSetup {
                name: "length",
                flags: TableColumnFlags::WIDTH_FIXED,
                init_width_or_weight: 70.0,
                user_id: Id::default(),
            },
            TableColumnSetup {
                name: "rating",
                flags: TableColumnFlags::WIDTH_FIXED,
                init_width_or_weight: 90.0,
                user_id: Id::default(),
            },
            TableColumnSetup {
                name: "platforms",
                flags: TableColumnFlags::WIDTH_STRETCH,
                init_width_or_weight: 1.0,
                user_id: Id::default(),
            },
            TableColumnSetup {
                name: "developer",
                flags: TableColumnFlags::WIDTH_STRETCH,
                init_width_or_weight: 1.0,
                user_id: Id::default(),
            },
        ];
        if let Some(_t) = ui.begin_table_header_with_flags("results", columns, table_flags) {
            for vn in state.search_results.iter() {
                let _id = ui.push_id(&vn.id);
                let active = state
                    .active
                    .as_ref()
                    .map(|a| a.summary.id == vn.id)
                    .unwrap_or(false);
                ui.table_next_column();
                // Whole-row selectable -- click the row to set it active.
                let label = format!("{}##row", vn.id);
                if ui
                    .selectable_config(label)
                    .span_all_columns(true)
                    .selected(active)
                    .build()
                {
                    settings.vndb_active_id = Some(vn.id.clone());
                    vndb.set_active(vn.clone());
                }
                ui.table_next_column();
                ui.text_wrapped(&vn.title);
                if let Some(alt) = vn.alttitle.as_ref().filter(|s| !s.is_empty()) {
                    ui.text_disabled(alt);
                }
                if let Some(desc) = vn.description.as_ref().filter(|s| !s.is_empty()) {
                    if ui.is_item_hovered() {
                        let cleaned = clean_description(desc);
                        if !cleaned.is_empty() {
                            ui.tooltip(|| {
                                let _w = ui.push_text_wrap_pos_with_pos(400.0);
                                ui.text_wrapped(&cleaned);
                            });
                        }
                    }
                }
                ui.table_next_column();
                ui.text(vn.released.as_deref().unwrap_or(""));
                ui.table_next_column();
                ui.text(vn.length_label().unwrap_or(""));
                ui.table_next_column();
                match (vn.rating, vn.votecount) {
                    (Some(r), Some(v)) => ui.text(format!("{:.1} ({})", r / 10.0, v)),
                    (Some(r), None) => ui.text(format!("{:.1}", r / 10.0)),
                    _ => ui.text(""),
                }
                ui.table_next_column();
                ui.text_wrapped(&vn.platforms.join(", "));
                ui.table_next_column();
                let devs: Vec<&str> = vn.developers.iter().map(|d| d.name.as_str()).collect();
                ui.text_wrapped(&devs.join(", "));
            }
        }
    }
}

fn sort_combo(ui: &Ui, current: &mut SortKey) {
    let label = SORT_OPTIONS
        .iter()
        .find(|(k, _)| k == current)
        .map(|(_, l)| *l)
        .unwrap_or("");
    if let Some(_t) = ui.begin_combo("##sort", label) {
        for (k, l) in SORT_OPTIONS {
            let selected = k == current;
            if selected {
                ui.set_item_default_focus();
            }
            if ui.selectable_config(*l).selected(selected).build() {
                *current = *k;
            }
        }
    }
}

fn multi_select_str(
    ui: &Ui,
    label: &str,
    options: &[&'static str],
    selected: &mut Vec<String>,
    width: f32,
) {
    let summary = if selected.is_empty() {
        format!("{}: any", label)
    } else {
        format!("{}: {}", label, selected.join(", "))
    };
    let _id = ui.push_id(label);
    let popup_id = format!("{}_popup", label);
    if ui.button_with_size(format!("{}##btn", summary), [width, 0.0]) {
        ui.open_popup(&popup_id);
    }
    ui.popup(&popup_id, || {
        if let Some(_t) = ui.begin_table_with_flags("opts", 4, TableFlags::SIZING_FIXED_FIT) {
            for opt in options {
                ui.table_next_column();
                let mut checked = selected.iter().any(|s| s == opt);
                if ui.checkbox(*opt, &mut checked) {
                    if checked {
                        if !selected.iter().any(|s| s == opt) {
                            selected.push((*opt).to_string());
                        }
                    } else {
                        selected.retain(|s| s != opt);
                    }
                }
            }
        }
        ui.separator();
        if ui.button("Clear") {
            selected.clear();
        }
    });
}

fn multi_select_length(ui: &Ui, selected: &mut Vec<u8>, width: f32) {
    let summary = if selected.is_empty() {
        "length: any".to_string()
    } else {
        let names: Vec<&str> = selected
            .iter()
            .filter_map(|n| LENGTH_OPTIONS.iter().find(|(v, _)| v == n).map(|(_, l)| *l))
            .collect();
        format!("length: {}", names.join(", "))
    };
    let _id = ui.push_id("length");
    let popup_id = "length_popup";
    if ui.button_with_size(format!("{}##btn", summary), [width, 0.0]) {
        ui.open_popup(popup_id);
    }
    ui.popup(popup_id, || {
        for (v, l) in LENGTH_OPTIONS {
            let mut checked = selected.contains(v);
            if ui.checkbox(*l, &mut checked) {
                if checked {
                    if !selected.contains(v) {
                        selected.push(*v);
                    }
                } else {
                    selected.retain(|x| x != v);
                }
            }
        }
        ui.separator();
        if ui.button("Clear") {
            selected.clear();
        }
    });
}

/// Render the active VN's metadata as a single bullet-separated line.
fn draw_vn_details(ui: &Ui, vn: &VnSummary) {
    if let Some(alt) = vn.alttitle.as_ref().filter(|s| !s.is_empty()) {
        ui.text_disabled(alt);
    }
    let mut parts: Vec<String> = Vec::new();
    if let Some(rel) = &vn.released {
        parts.push(format!("released {}", rel));
    }
    if let Some(len) = vn.length_label() {
        parts.push(format!("length {}", len));
    }
    if let Some(r) = vn.rating {
        let votes = vn.votecount.unwrap_or(0);
        parts.push(format!("rating {:.1} ({} votes)", r / 10.0, votes));
    }
    if !vn.platforms.is_empty() {
        parts.push(format!("platforms {}", vn.platforms.join(", ")));
    }
    if !vn.developers.is_empty() {
        let devs: Vec<&str> = vn.developers.iter().map(|d| d.name.as_str()).collect();
        parts.push(format!("dev {}", devs.join(", ")));
    }
    if !parts.is_empty() {
        ui.text_disabled(parts.join("  -  "));
    }
}
