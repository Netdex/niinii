use std::ffi::CStr;

use dear_imgui_rs::*;

use crate::{
    gui::{px2, Frame},
    settings::Settings,
};

use super::{mixins::help_marker, panel::Panel};

pub fn show(panel: &mut Panel, ui: &Ui, frame: &Frame, settings: &mut Settings) {
    panel.show(
        ui,
        |w| {
            w.size_constraints(px2(ui, [360.0, 300.0]), [f32::MAX, f32::MAX])
                .flags(WindowFlags::MENU_BAR)
        },
        || {
                ui.menu_bar(|| {
                    if ui.menu_item("Save") {
                        settings.theme =
                            Some(capture_theme(&ui.clone_style(), frame.dpi_scale()));
                    }
                    if ui.menu_item("Reset") {
                        settings.theme = None;
                    }
                    if settings.theme.is_some() {
                        ui.menu_with_enabled("Style saved", false, || {});
                        help_marker(
                            ui,
                            "Saved style will be restored on start-up. Reset will clear the stored style.",
                        );
                    }
                });
                ui.show_default_style_editor();
        },
    );
}

/// Every color, and the sizes a [`Theme`] can hold divided by `dpi_scale`
/// (the live style is scaled to the monitor).
fn capture_theme(style: &Style, dpi_scale: f32) -> Theme {
    let px = |v: f32| Some(v / dpi_scale);
    let px2 = |[x, y]: [f32; 2]| Some([x / dpi_scale, y / dpi_scale]);
    Theme {
        preset: ThemePreset::None,
        colors: style_colors()
            .map(|id| ColorOverride {
                id,
                rgba: style.color(id),
            })
            .collect(),
        style: StyleTweaks {
            window_rounding: px(style.window_rounding()),
            frame_rounding: px(style.frame_rounding()),
            tab_rounding: px(style.tab_rounding()),
            window_padding: px2(style.window_padding()),
            frame_padding: px2(style.frame_padding()),
            cell_padding: px2(style.cell_padding()),
            item_spacing: px2(style.item_spacing()),
            item_inner_spacing: px2(style.item_inner_spacing()),
            scrollbar_size: px(style.scrollbar_size()),
            grab_min_size: px(style.grab_min_size()),
            indent_spacing: px(style.indent_spacing()),
            separator_size: px(style.separator_size()),
            scrollbar_rounding: px(style.scrollbar_rounding()),
            grab_rounding: px(style.grab_rounding()),
            window_border_size: px(style.window_border_size()),
            child_border_size: px(style.child_border_size()),
            popup_border_size: px(style.popup_border_size()),
            frame_border_size: px(style.frame_border_size()),
            tab_border_size: px(style.tab_border_size()),
            child_rounding: px(style.child_rounding()),
            popup_rounding: px(style.popup_rounding()),
            anti_aliased_lines: Some(style.anti_aliased_lines()),
            anti_aliased_fill: Some(style.anti_aliased_fill()),
        },
        ..Default::default()
    }
}

/// All style colors, identified through their ImGui names since the binding
/// has no way to enumerate [`StyleColor`].
fn style_colors() -> impl Iterator<Item = StyleColor> {
    (0..StyleColor::COUNT).filter_map(|idx| {
        // SAFETY: ImGui returns a static, NUL-terminated name for every index
        // below ImGuiCol_COUNT.
        let name = unsafe { CStr::from_ptr(sys::igGetStyleColorName(idx as sys::ImGuiCol)) };
        let name = name.to_str().ok()?;
        serde_json::from_value(serde_json::Value::String(name.to_owned())).ok()
    })
}
