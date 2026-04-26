use std::f32::consts::PI;

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

#[derive(Clone, Copy)]
pub enum BottomTextMode {
    /// Reserve `h` of vertical space and the standard bottom vpad without
    /// drawing any text. Used when a translation is drawn separately across
    /// one or more basic-split segments.
    Pad(f32),
    None,
}

// Layout model for kanji-block flows
// ----------------------------------
//
// A "kanji block" is a unit drawn by `draw_kanji_text`: kanji centered, with
// optional ruby above and an optional fixed-height bottom slot. Its width
// is `max(kanji_w, ruby_w)`, returned by `measure_kanji_w`. Block height
// includes any reserved bottom space (`BottomTextMode::Pad(h)`).
//
// A sequence of blocks placed via imgui's same-line/wrap flow forms one or
// more visual rows. `simulate_flow` mirrors imgui's wrap_line decisions
// against given widths to predict those rows ahead of drawing -- this is
// how callers size per-block reservations they need before any draw
// happens.
//
// `distribute_lines` greedily packs words across a row-width sequence: each
// row gets one packed line, the last row absorbs leftover as additional
// stacked lines at its width. `draw_translation_lines_colored` emits those
// lines via the window draw-list at each row's y + `bottom_text_y_offset`.

/// One visual row of a kanji-block flow. `y` is the row's top in screen
/// coordinates (above the ruby band); `left`/`right` bracket the row's
/// horizontal extent.
#[derive(Clone, Copy)]
pub struct RowExtent {
    pub y: f32,
    pub left: f32,
    pub right: f32,
}

/// Width of the kanji block that `draw_kanji_text` would produce for these
/// inputs: `max(kanji_w, ruby_w)`. Used by `simulate_flow` to match the
/// actual draw geometry exactly.
pub fn measure_kanji_w(ui: &Ui, ctx: &Context, text: &str, ruby: &RubyTextMode) -> f32 {
    let _t = ui.push_font(ctx.get_font(TextStyle::Kanji));
    let kanji_w = ui.calc_text_size(text)[0];
    drop(_t);
    let ruby_w = match ruby {
        RubyTextMode::Text(t) => ui.calc_text_size(t)[0],
        RubyTextMode::Pad | RubyTextMode::None => 0.0,
    };
    kanji_w.max(ruby_w)
}

/// Greedy fit: take as many words from `words` as fit in `max_w`. Returns
/// the joined line and how many words were consumed. The first word is
/// always emitted even if oversize, so progress is guaranteed.
pub fn take_words_fitting(ui: &Ui, words: &[&str], max_w: f32) -> (String, usize) {
    let mut line = String::new();
    let mut consumed = 0usize;
    for w in words {
        let candidate = if line.is_empty() {
            w.to_string()
        } else {
            format!("{} {}", line, w)
        };
        if !line.is_empty() && ui.calc_text_size(&candidate)[0] > max_w {
            break;
        }
        line = candidate;
        consumed += 1;
    }
    (line, consumed)
}

/// Distribute `words` across rows of the given widths via repeated
/// `take_words_fitting`. Each row gets one greedy line; the last row
/// absorbs any leftover as additional stacked lines at its width.
pub fn distribute_lines(ui: &Ui, words: &[&str], row_widths: &[f32]) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = row_widths.iter().map(|_| Vec::new()).collect();
    if words.is_empty() || row_widths.is_empty() {
        return rows;
    }
    let last = row_widths.len() - 1;
    let mut cursor = 0usize;
    for (i, &w) in row_widths.iter().enumerate() {
        if cursor < words.len() {
            let (line, c) = take_words_fitting(ui, &words[cursor..], w);
            rows[i].push(line);
            cursor += c;
        }
        if i == last {
            while cursor < words.len() {
                let (line, c) = take_words_fitting(ui, &words[cursor..], w);
                if c == 0 {
                    break;
                }
                rows[i].push(line);
                cursor += c;
            }
        }
    }
    rows
}

/// Simulate imgui flow over a sequence of width-groups starting at the
/// current cursor. Each entry of `groups` is one segment's item widths.
/// Returns each group's rows as `RowExtent` with `y = row_index` so
/// same-row identification (by equality) works without pixel y values.
///
/// **Preconditions** (callers must hold these or the simulation lies):
/// - Called *before* any draw for these groups.
/// - `ui.cursor_screen_pos()` reflects the next-line position;
///   `ui.item_rect_max()` is the previous segment's last item.
/// - No intervening cursor manipulation between this call and the draws.
pub fn simulate_global_flow(ui: &Ui, groups: &[Vec<f32>]) -> Vec<Vec<RowExtent>> {
    let visible_x = ui.window_pos()[0] + ui.window_content_region_max()[0];
    let new_row_x = ui.cursor_screen_pos()[0];
    let item_sp = unsafe { ui.style().item_spacing[0] };
    let max_w = ui.window_content_region_max()[0];

    let mut last_x = ui.item_rect_max()[0];
    let mut row_index: i32 = 0;
    let mut group_rows: Vec<Vec<RowExtent>> = vec![Vec::new(); groups.len()];

    for (g_idx, widths) in groups.iter().enumerate() {
        for &w in widths {
            let next_x = last_x + item_sp + w;
            let item_left = if next_x < visible_x || w >= max_w {
                last_x + item_sp
            } else {
                row_index += 1;
                new_row_x
            };
            let item_right = item_left + w;
            last_x = item_right;
            let key = row_index as f32;
            let rows = &mut group_rows[g_idx];
            match rows.last_mut() {
                Some(r) if (r.y - key).abs() < 0.5 => r.right = r.right.max(item_right),
                _ => rows.push(RowExtent {
                    y: key,
                    left: item_left,
                    right: item_right,
                }),
            }
        }
    }
    group_rows
}

/// For each `(rows, words)` group, compute the per-term Pad height the
/// caller must apply so every row of the group has bottom space tall enough
/// for the greedy line distribution. This deliberately does not borrow
/// horizontal space from neighboring groups; translation layout is local
/// to the same basic-split groups used by the translator.
pub fn plan_translation_reservations(
    ui: &Ui,
    groups: &[(Vec<RowExtent>, &[String])],
    line_h: f32,
) -> Vec<f32> {
    groups
        .iter()
        .map(|(rows, words)| {
            if rows.is_empty() {
                return 0.0;
            }
            let widths: Vec<f32> = rows.iter().map(|r| r.right - r.left).collect();
            let words_ref: Vec<&str> = words.iter().map(String::as_str).collect();
            let lines = distribute_lines(ui, &words_ref, &widths);
            let max_lines = lines.iter().map(|r| r.len()).max().unwrap_or(0);
            (max_lines as f32) * line_h
        })
        .collect()
}

/// Place precomputed translation `lines` at `rows`. Lines are left-aligned
/// in their own basic-split row extent and stacked vertically within the
/// bottom slot the caller reserved via `BottomTextMode::Pad`. Drawn with
/// the same style as furigana so the translation visually belongs to the
/// kanji block above.
pub fn draw_translation_lines_colored(
    ui: &Ui,
    ctx: &Context,
    ruby_present: bool,
    stroke: bool,
    fore: StyleColor,
    rows: &[RowExtent],
    lines: &[Vec<String>],
) {
    if rows.is_empty() || lines.iter().all(|l| l.is_empty()) {
        return;
    }
    let line_h = ui.text_line_height();
    let y_off = bottom_text_y_offset(ui, ctx, ruby_present);
    let draw_list = ui.get_window_draw_list();
    for (row, row_lines) in rows.iter().zip(lines.iter()) {
        if row_lines.is_empty() {
            continue;
        }
        let clip_min = [row.left, row.y + y_off];
        let clip_max = [row.right, row.y + y_off + (row_lines.len() as f32) * line_h];
        draw_list.with_clip_rect_intersect(clip_min, clip_max, || {
            for (j, line) in row_lines.iter().enumerate() {
                let x = row.left;
                let y = row.y + y_off + (j as f32) * line_h;
                draw_styled_text_with_color(ui, &draw_list, line, [x, y], stroke, fore);
            }
        });
    }
}

/// Draw `text` at `pos` matching the style used for ruby/kanji bodies, with
/// a 1px stroke against `StyleColor::TitleBg` when `stroke` is set.
fn draw_styled_text_with_color(
    ui: &Ui,
    draw_list: &DrawListMut,
    text: &str,
    pos: [f32; 2],
    stroke: bool,
    fore: StyleColor,
) {
    if stroke {
        stroke_token_with_color(
            ui,
            draw_list,
            text,
            pos,
            StrokeStyle {
                thick: 1.0,
                fore,
                back: StyleColor::TitleBg,
            },
        );
    } else {
        draw_list.add_text(pos, ui.style_color(fore), text);
    }
}

/// Vertical offset from the top of a kanji block to where externally drawn
/// bottom text (translation) should be placed. Mirrors the layout used by
/// `draw_kanji_text` with `BottomTextMode::Pad`.
pub fn bottom_text_y_offset(ui: &Ui, ctx: &Context, ruby_present: bool) -> f32 {
    let ruby_h = if ruby_present {
        ui.text_line_height()
    } else {
        0.0
    };
    let vpad = if ruby_present { 8.0 } else { 0.0 };
    let _t = ui.push_font(ctx.get_font(TextStyle::Kanji));
    let kanji_h = ui.text_line_height();
    drop(_t);
    let vpad_b = 6.0;
    ruby_h + vpad + kanji_h + vpad_b
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

pub fn stroke_text(ui: &Ui, draw_list: &DrawListMut, text: &str, thick: f32) {
    stroke_text_with_highlight(ui, draw_list, text, thick, None)
}

pub struct KanjiStyle {
    pub highlight: bool,
    pub stroke: bool,
    pub preview: bool,
    pub underline: UnderlineMode,
}

/// Geometry of a drawn kanji block plus the underline-hover flag. Callers
/// laying out grouped blocks (e.g. clauses with a per-clause translation
/// hanging below) need the actual row position, which `cursor_screen_pos()`
/// can't give them after `dummy()` advances the cursor to the next line.
#[derive(Clone, Copy)]
pub struct KanjiDrawn {
    pub underline_hover: bool,
    /// Y of the top of the row the block was drawn on (after `wrap_line`).
    pub row_y: f32,
    pub x_left: f32,
    pub x_right: f32,
}
pub fn draw_kanji_text(
    ui: &Ui,
    ctx: &Context,
    text: &str,
    ruby_text: RubyTextMode,
    bottom_text: BottomTextMode,
    style: KanjiStyle,
) -> KanjiDrawn {
    let KanjiStyle {
        highlight,
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

    let bottom_h = match bottom_text {
        BottomTextMode::Pad(h) => h,
        BottomTextMode::None => 0.0,
    };

    let vpad = match ruby_text {
        RubyTextMode::None => 0.0,
        _ => 8.0,
    };
    let vpad_b = match bottom_text {
        BottomTextMode::None => 0.0,
        _ => 6.0,
    };
    let w = kanji_sz[0].max(ruby_sz[0]);
    let h = kanji_sz[1] + ruby_sz[1] + vpad + bottom_h + vpad_b;

    wrap_line(ui, w);

    let row_y = ui.cursor_screen_pos()[1];
    let x = ui.cursor_screen_pos()[0];
    let mut y = row_y + vpad;

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
        draw_list
            .add_rect(
                [cx, y],
                [cx + kanji_sz[0], y + kanji_sz[1]],
                ui.style_color(StyleColor::TextSelectedBg),
            )
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

    let underline_hover = ui.is_window_focused()
        && ui.is_mouse_hovering_rect(
            [ul0[0], ul0[1] - ul_thick / 2.0],
            [ul1[0], ul1[1] + ul_thick / 2.0],
        );
    KanjiDrawn {
        underline_hover,
        row_y,
        x_left: x,
        x_right: x + w,
    }
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

/// https://github.com/ocornut/imgui/issues/1901
pub fn spinner(ui: &Ui, radius: f32, thickness: f32, color: StyleColor) {
    let frame_padding_y = unsafe { ui.style().frame_padding[1] };
    let now = ui.time() as f32;
    let pos = ui.cursor_screen_pos();
    let size = [radius * 2.0, (radius + frame_padding_y) * 2.0];
    ui.dummy(size);

    let draw_list = ui.get_window_draw_list();
    let num_segments = 30;
    let start = f32::abs(f32::sin(now * 1.8) * ((num_segments - 5) as f32));

    let a_min = PI * 2.0 * start / (num_segments as f32);
    let a_max = PI * 2.0 * ((num_segments - 3) as f32) / (num_segments as f32);

    let center = [pos[0] + radius, pos[1] + radius + frame_padding_y];

    let mut points = Vec::with_capacity(num_segments);
    for i in 0..num_segments {
        let a = a_min + ((i as f32) / (num_segments as f32)) * (a_max - a_min);
        points.push([
            center[0] + f32::cos(a + now * 8.0) * radius,
            center[1] + f32::sin(a + now * 8.0) * radius,
        ]);
    }

    draw_list
        .add_polyline(points, ui.style_color(color))
        .filled(false)
        .thickness(thickness)
        .build();
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
