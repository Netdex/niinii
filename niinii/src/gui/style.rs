use std::ffi::CStr;

use dear_imgui_rs::{
    sys, ColorOverride, Context, Style, StyleColor, StyleTweaks, Theme, ThemePreset,
};

/// The style built from the saved theme with its sizes scaled by the
/// monitor's DPI times the user's text scale, so padding and spacing grow
/// with the text.
pub(super) struct StyleScale {
    /// The style before any theme or scaling, to rebuild from.
    defaults: Style,
    /// The saved theme, in unscaled sizes.
    theme: Option<Theme>,
    /// Scale the live style's sizes carry.
    applied: f32,
}

impl StyleScale {
    pub fn new(imgui: &mut Context, theme: Option<Theme>, scale: f32) -> Self {
        let style = Self {
            defaults: imgui.style().clone(),
            theme,
            applied: scale,
        };
        style.rebuild(imgui);
        style
    }

    pub fn applied(&self) -> f32 {
        self.applied
    }

    /// The theme a later rebuild starts from. The live style is left as is.
    pub fn set_theme(&mut self, theme: Option<Theme>) {
        self.theme = theme;
    }

    /// Rebuilds the style from the saved theme when `scale` changed. Unsaved
    /// Style Editor edits are discarded.
    pub fn update(&mut self, imgui: &mut Context, scale: f32) {
        if scale != self.applied {
            self.applied = scale;
            self.rebuild(imgui);
        }
    }

    fn rebuild(&self, imgui: &mut Context) {
        let style = imgui.style_mut();
        let (main, dpi) = (style.font_scale_main(), style.font_scale_dpi());
        *style = self.defaults.clone();
        if let Some(theme) = &self.theme {
            theme.apply_to_style(style);
        }
        style.scale_all_sizes(self.applied);
        style.set_font_scale_main(main);
        style.set_font_scale_dpi(dpi);
    }
}

/// Every color, and the sizes a [`Theme`] can hold divided by `scale` (the
/// scale the style's sizes carry).
pub fn capture_theme(style: &Style, scale: f32) -> Theme {
    let px = |v: f32| Some(v / scale);
    let px2 = |[x, y]: [f32; 2]| Some([x / scale, y / scale]);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rescales_from_the_theme() {
        let mut imgui = Context::create();
        imgui
            .set_ini_filename(None::<std::path::PathBuf>)
            .expect("disable imgui.ini");
        let original = imgui.style().window_padding();
        let mut style = StyleScale::new(&mut imgui, None, 1.0);

        style.update(&mut imgui, 2.0);
        assert_eq!(
            imgui.style().window_padding(),
            [original[0] * 2.0, original[1] * 2.0]
        );

        // Many small steps, as when dragging the text scale slider.
        for step in 0..100 {
            style.update(&mut imgui, 1.0 + step as f32 * 0.013);
        }
        style.update(&mut imgui, 1.0);
        assert_eq!(imgui.style().window_padding(), original);

        // A saved theme is what the next rescale starts from.
        let mut theme = capture_theme(imgui.style(), 1.0);
        theme.style.frame_padding = Some([10.0, 6.0]);
        style.set_theme(Some(theme));
        style.update(&mut imgui, 1.5);
        assert_eq!(imgui.style().frame_padding(), [15.0, 9.0]);
    }
}
