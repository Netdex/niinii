use crate::gui::px;
use dear_imgui_rs::*;

use std::sync::Arc;

use ichiran::prelude::Ichiran;

use crate::{
    settings::Settings,
    translator::Backend,
    view::{
        mixins::{error_note, form_cell, form_grid},
        panel::{Panel, Sizing, TextSize},
    },
    vndb::{self, clean_description, SearchParams, SortKey, VnSummary, VndbHandle},
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

pub const SIZING: Sizing = Sizing {
    default: None,
    min: TextSize::new(80.0, 18.0),
};

pub struct VndbView {
    params: SearchParams,
    vndb: VndbHandle,
}

impl VndbView {
    /// Create the view and spawn the VNDB writer task.
    ///
    /// `backend` receives the active VN's prompt fragment via
    /// `set_system_addendum`, regardless of which translator backend is active.
    /// `ichiran` is used to push character names into the dictionary whenever
    /// the active VN changes, so the segmenter recognises them as proper nouns.
    pub fn new(backend: Arc<dyn Backend>, ichiran: Ichiran, settings: &Settings) -> Self {
        let vndb = vndb::spawn(
            move |prompt: Option<Arc<str>>| {
                backend.set_system_addendum(prompt);
            },
            ichiran,
        );
        if let Some(id) = settings.vndb_active_id.clone() {
            vndb.set_active_by_id(id);
        }
        Self {
            params: SearchParams::default(),
            vndb,
        }
    }

    /// Menu-bar label naming the VN whose characters the translator uses.
    /// Opens the window when clicked.
    pub fn show_status(&self, panel: &mut Panel, ui: &Ui) {
        let state = self.vndb.state();
        let label = match (&state.active, state.loading_active) {
            (_, true) => "loading VN...",
            (Some(active), false) => active.summary.title.as_str(),
            (None, false) => "no VN",
        };
        let _dim = state.active.is_none().then(|| {
            ui.push_style_color(StyleColor::Text, ui.style_color(StyleColor::TextDisabled))
        });
        if ui.menu_item(format!("{label}##vndb_status")) {
            panel.open();
        }
    }

    pub fn show(&mut self, panel: &mut Panel, ui: &Ui, settings: &mut Settings) {
        panel.show(ui, WindowFlags::empty(), || self.contents(ui, settings));
    }

    fn contents(&mut self, ui: &Ui, settings: &mut Settings) {
        let vndb = &self.vndb;
        let state = vndb.state();

        let style = unsafe { ui.style() };
        let search = "Search";
        let button_width = ui.calc_text_size(search)[0] + 2.0 * style.frame_padding()[0];
        ui.set_next_item_width(-(button_width + style.item_spacing()[0]));
        let submitted = ui
            .input_text("##query", &mut self.params.query)
            .enter_returns_true(true)
            .hint("Search visual novel...")
            .build();
        ui.same_line();
        let clicked = ui.button(search);
        if (submitted || clicked) && !self.params.query.trim().is_empty() {
            vndb.search(self.params.clone());
        }

        let params = &mut self.params;
        form_grid(ui, "filters", 2, || {
            form_cell(ui, "Language", || {
                multi_select_str(ui, "##lang", LANG_OPTIONS, &mut params.langs)
            });
            form_cell(ui, "Platform", || {
                multi_select_str(ui, "##platform", PLATFORM_OPTIONS, &mut params.platforms)
            });
            form_cell(ui, "Length", || {
                multi_select_length(ui, &mut params.lengths)
            });
            form_cell(ui, "Sort", || {
                let reverse = "Reverse";
                let reverse_width = ui.frame_height()
                    + style.item_inner_spacing()[0]
                    + ui.calc_text_size(reverse)[0];
                ui.set_next_item_width(-(reverse_width + style.item_spacing()[0]));
                sort_combo(ui, &mut params.sort);
                ui.same_line();
                ui.checkbox(reverse, &mut params.reverse);
            });
        });
        if state.searching {
            ui.text_disabled("searching...");
        }

        // Active VN block.
        if let Some(active) = &state.active {
            ui.separator();
            ui.text_colored([0.6, 0.9, 0.6, 1.0], &active.summary.title);
            ui.same_line();
            let id = &active.summary.id;
            ui.text_link_open_url(id, format!("https://vndb.org/{id}"));
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
            error_note(ui, &format!("vndb: {err}"));
        }

        ui.separator();

        let table_flags = TableOptions::default()
            .flags(TableFlags::BORDERS_INNER_H | TableFlags::ROW_BG | TableFlags::SCROLL_Y)
            .sizing_policy(TableSizingPolicy::StretchProp);
        let columns = [
            TableColumnSetup::new("id").fixed_width(px(ui, 56.0)),
            TableColumnSetup::new("title").stretch_weight(3.0),
            TableColumnSetup::new("released").fixed_width(px(ui, 80.0)),
            TableColumnSetup::new("length").fixed_width(px(ui, 70.0)),
            TableColumnSetup::new("rating").fixed_width(px(ui, 90.0)),
            TableColumnSetup::new("platforms").stretch_weight(1.0),
            TableColumnSetup::new("developer").stretch_weight(1.0),
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
                    if ui.is_item_hovered_with_flags(ItemHoveredFlags::FOR_TOOLTIP) {
                        let cleaned = clean_description(desc);
                        if !cleaned.is_empty() {
                            ui.tooltip(|| {
                                let _w = ui.push_text_wrap_pos(px(ui, 400.0));
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
                ui.text_wrapped(vn.platforms.join(", "));
                ui.table_next_column();
                let devs: Vec<&str> = vn.developers.iter().map(|d| d.name.as_str()).collect();
                ui.text_wrapped(devs.join(", "));
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

fn multi_select_str(ui: &Ui, label: &str, options: &[&'static str], selected: &mut Vec<String>) {
    let preview = if selected.is_empty() {
        "any".to_owned()
    } else {
        selected.join(", ")
    };
    if let Some(_combo) = ui.begin_combo(label, preview) {
        if let Some(_t) = ui.begin_table_with_flags(
            "opts",
            4,
            TableOptions::default().sizing_policy(TableSizingPolicy::FixedFit),
        ) {
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
    }
}

fn multi_select_length(ui: &Ui, selected: &mut Vec<u8>) {
    let preview = if selected.is_empty() {
        "any".to_owned()
    } else {
        selected
            .iter()
            .filter_map(|n| LENGTH_OPTIONS.iter().find(|(v, _)| v == n).map(|(_, l)| *l))
            .collect::<Vec<_>>()
            .join(", ")
    };
    if let Some(_combo) = ui.begin_combo("##length", preview) {
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
    }
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
