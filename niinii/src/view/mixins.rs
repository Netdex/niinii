use imgui::{DrawListMut, MouseCursor, StyleColor, Ui};
use strum::IntoEnumIterator;

use crate::renderer::context::{Context, TextStyle};

pub fn help_marker(ui: &Ui, text: &str) {
    ui.text_colored(ui.style_color(StyleColor::TextDisabled), "[?]");
    if ui.is_item_hovered() {
        ui.tooltip_text(text);
    }
}

pub fn wrap_bullet(ui: &Ui, text: &str) {
    ui.bullet();
    ui.text_wrapped(text);
}

pub enum UnderlineMode {
    Normal,
    Pad,
    None,
}

pub enum RubyTextMode<'a> {
    Text(&'a str),
    Pad,
    None,
}

pub struct StrokeStyle {
    thick: f32,
    fore: StyleColor,
    back: StyleColor,
}
pub fn stroke_token_with_offsets(
    ui: &Ui,
    draw_list: &DrawListMut,
    text: &str,
    pos: [f32; 2],
    offsets: &[[f32; 2]],
    style: StrokeStyle,
) {
    let StrokeStyle { thick, fore, back } = style;
    for off in offsets {
        draw_list.add_text(
            [pos[0] + off[0] * thick, pos[1] + off[1] * thick],
            ui.style_color(back),
            text,
        );
    }
    draw_list.add_text(pos, ui.style_color(fore), text);
}

pub fn stroke_token_with_color(
    ui: &Ui,
    draw_list: &DrawListMut,
    text: &str,
    pos: [f32; 2],
    style: StrokeStyle,
) {
    let offsets = [
        [-1.0, -1.0],
        [-1.0, 1.0],
        [1.0, -1.0],
        [1.0, 1.0],
        [-1.0, 0.0],
        [1.0, 0.0],
        [0.0, -1.0],
        [0.0, 1.0],
    ];
    stroke_token_with_offsets(ui, draw_list, text, pos, &offsets, style);
}

pub fn stroke_text_with_highlight(
    ui: &Ui,
    draw_list: &DrawListMut,
    text: &str,
    thick: f32,
    highlight: Option<StyleColor>,
) {
    ui.new_line();
    let tokens = text.split_inclusive(char::is_whitespace);
    for token in tokens {
        let sz = ui.calc_text_size(token);
        wrap_line_with_spacing(ui, sz[0], 0.0);
        let p = ui.cursor_screen_pos();
        if let Some(highlight) = highlight {
            draw_list
                .add_rect(p, [p[0] + sz[0], p[1] + sz[1]], ui.style_color(highlight))
                .filled(true)
                .build();
        }
        stroke_token_with_color(
            ui,
            draw_list,
            token,
            ui.cursor_screen_pos(),
            StrokeStyle {
                thick,
                fore: StyleColor::Text,
                back: StyleColor::TitleBg,
            },
        );
        ui.dummy(sz)
    }
}

pub struct KanjiStyle {
    pub highlight: bool,
    /// Override for the highlight rectangle color. `None` falls back to
    /// `StyleColor::TextSelectedBg`. Ignored when `highlight` is false.
    pub highlight_color: Option<[f32; 4]>,
    pub stroke: bool,
    pub preview: bool,
    pub underline: UnderlineMode,
}
pub fn draw_kanji_text(
    ui: &Ui,
    ctx: &Context,
    text: &str,
    ruby_text: RubyTextMode,
    style: KanjiStyle,
) -> bool {
    let KanjiStyle {
        highlight,
        highlight_color,
        stroke,
        preview,
        underline,
    } = style;
    let ruby_sz = match ruby_text {
        RubyTextMode::Text(text) => ui.calc_text_size(text),
        RubyTextMode::Pad => [0.0, ui.text_line_height()],
        RubyTextMode::None => [0.0, 0.0],
    };

    let _kanji_font_token = ui.push_font(ctx.get_font(TextStyle::Kanji));
    let kanji_sz = ui.calc_text_size(text);
    drop(_kanji_font_token);

    let vpad = match ruby_text {
        RubyTextMode::None => 0.0,
        _ => 8.0,
    };
    let w = f32::max(kanji_sz[0], ruby_sz[0]);
    let h = kanji_sz[1] + ruby_sz[1] + vpad;

    wrap_line(ui, w);

    let x = ui.cursor_screen_pos()[0];
    let mut y = ui.cursor_screen_pos()[1] + vpad;

    let draw_list = ui.get_window_draw_list();

    let maybe_stroke_text = |text: &str, pos: [f32; 2], thick: f32| {
        if stroke {
            stroke_token_with_color(
                ui,
                &draw_list,
                text,
                pos,
                StrokeStyle {
                    thick,
                    fore: StyleColor::Text,
                    back: StyleColor::TitleBg,
                },
            );
        } else {
            draw_list.add_text(pos, ui.style_color(StyleColor::Text), text);
        }
    };

    if let RubyTextMode::Text(text) = ruby_text {
        let cx = x + w / 2.0 - ruby_sz[0] / 2.0;
        maybe_stroke_text(text, [cx, y], 1.0);
    }

    let cx = x + w / 2.0 - kanji_sz[0] / 2.0;
    y += ruby_sz[1];

    if highlight {
        let color = highlight_color.unwrap_or_else(|| ui.style_color(StyleColor::TextSelectedBg));
        draw_list
            .add_rect([cx, y], [cx + kanji_sz[0], y + kanji_sz[1]], color)
            .rounding(5.0)
            .filled(true)
            .build();
    }

    let item_spacing_x = unsafe { ui.style().item_spacing[0] };
    let ul_thick = 4.0;
    let ul0 = [x, y + kanji_sz[1] + ul_thick / 2.0];
    let ul1 = match underline {
        UnderlineMode::Normal => [x + w, y + kanji_sz[1] + ul_thick / 2.0],
        UnderlineMode::Pad => [x + w + item_spacing_x, y + kanji_sz[1] + ul_thick / 2.0],
        UnderlineMode::None => ul0,
    };
    draw_list
        .add_line(ul0, ul1, ui.style_color(StyleColor::Text))
        .thickness(ul_thick)
        .build();

    let _kanji_font_token = ui.push_font(ctx.get_font(TextStyle::Kanji));
    if preview {
        stroke_token_with_offsets(
            ui,
            &draw_list,
            text,
            [cx, y],
            &[[1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            StrokeStyle {
                thick: 2.0,
                fore: StyleColor::TextDisabled,
                back: StyleColor::MenuBarBg,
            },
        )
    } else {
        maybe_stroke_text(text, [cx, y], 1.5);
    }
    drop(_kanji_font_token);

    ui.dummy([w, h]);

    ui.is_window_focused()
        && ui.is_mouse_hovering_rect(
            [ul0[0], ul0[1] - ul_thick / 2.0],
            [ul1[0], ul1[1] + ul_thick / 2.0],
        )
}

pub fn wrap_line_with_spacing(ui: &Ui, expected_width: f32, spacing: f32) -> bool {
    let max_width = ui.window_content_region_max()[0];
    let visible_x = ui.window_pos()[0] + max_width;
    let last_x = ui.item_rect_max()[0];
    let item_spacing_x = unsafe { ui.style().item_spacing[0] };
    let next_x = last_x + item_spacing_x + expected_width;
    // don't wrap if it will fit on the current line, or if it won't even fit on an empty line
    if next_x < visible_x || expected_width >= max_width {
        ui.same_line_with_spacing(0.0, spacing);
        false
    } else {
        true
    }
}

pub fn wrap_line(ui: &Ui, expected_width: f32) -> bool {
    let spacing = unsafe { ui.style().item_spacing[0] };
    wrap_line_with_spacing(ui, expected_width, spacing)
}

pub fn checkbox_option<T: Default, U>(
    ui: &Ui,
    val: &mut Option<T>,
    f: impl FnOnce(&Ui, &mut T) -> U,
) -> U {
    checkbox_option_with_default(ui, val, T::default(), f)
}

pub fn checkbox_option_with_default<T, U>(
    ui: &Ui,
    val: &mut Option<T>,
    default: T,
    f: impl FnOnce(&Ui, &mut T) -> U,
) -> U {
    let mut default = Some(default);
    let mut chk_value = val.is_some();
    let _id = ui.push_id_ptr(val);
    if ui.checkbox("##", &mut chk_value) {
        *val = if chk_value { default.take() } else { None }
    }
    ui.same_line();
    if let Some(val) = val {
        f(ui, val)
    } else {
        let _token = ui.begin_disabled(true);
        let mut dummy = default.unwrap();
        f(ui, &mut dummy)
    }
}

pub fn combo_enum<T>(ui: &Ui, label: impl AsRef<str>, val: &mut T)
where
    T: IntoEnumIterator + PartialEq,
    for<'a> &'a T: Into<&'static str>,
{
    let val_name = <&T as Into<&'static str>>::into(val);
    if let Some(_token) = ui.begin_combo(label, val_name) {
        for e in T::iter() {
            let selected = *val == e;
            if selected {
                ui.set_item_default_focus();
            }
            if ui
                .selectable_config(<&T as Into<&'static str>>::into(&e))
                .selected(selected)
                .build()
            {
                *val = e;
            }
        }
    }
}

pub fn combo_list<T>(ui: &Ui, label: impl AsRef<str>, elems: &[T], val: &mut T)
where
    T: Clone + PartialEq + AsRef<str>,
{
    if let Some(_token) = ui.begin_combo(label, val.as_ref()) {
        for e in elems {
            let selected = val == e;
            if selected {
                ui.set_item_default_focus();
            }
            if ui.selectable_config(e).selected(selected).build() {
                *val = e.clone();
            }
        }
    }
}

pub fn ellipses(ui: &Ui) -> &str {
    let now = ui.time();
    let pattern = ["   ", ".  ", ".. ", "..."];
    let i = (now * 5_f64) as usize % pattern.len();
    pattern[i]
}

pub fn drag_handle(ui: &Ui) {
    let table_row_bg = ui.style_color(StyleColor::TableRowBg);
    let _t1 = ui.push_style_color(StyleColor::Button, table_row_bg);
    let _t2 = ui.push_style_color(StyleColor::ButtonHovered, table_row_bg);
    let _t3 = ui.push_style_color(StyleColor::ButtonActive, table_row_bg);
    let _t4 = ui.push_style_color(StyleColor::Text, ui.style_color(StyleColor::TextDisabled));
    ui.button("\u{250b}\u{250b}");
    if ui.is_item_hovered() {
        ui.set_mouse_cursor(Some(MouseCursor::Hand));
    }
}
