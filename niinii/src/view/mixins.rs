use dear_imgui_rs::{
    ColorStackToken, DrawListMut, MouseCursor, StyleColor, TableColumnFlags, TableColumnWidth,
    TableFlags, Ui,
};
use strum::IntoEnumIterator;

use crate::gui::{px, FontSize};

pub fn help_marker(ui: &Ui, text: &str) {
    ui.text_disabled("(?)");
    ui.set_item_tooltip(text);
}

/// Secondary text, wrapped at the window's edge.
pub fn text_disabled_wrapped(ui: &Ui, text: impl AsRef<str>) {
    let _color = ui.push_style_color(StyleColor::Text, ui.style_color(StyleColor::TextDisabled));
    ui.text_wrapped(text);
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
    let thick = px(ui, thick);
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

/// Lays items out left to right from the cursor, starting a new line when
/// the next one would not fit.
pub struct Flow {
    start_x: f32,
    end_x: f32,
    placed: bool,
}

impl Flow {
    pub fn new(ui: &Ui) -> Self {
        let start_x = ui.cursor_screen_pos()[0];
        Self {
            start_x,
            end_x: start_x + ui.content_region_avail()[0],
            placed: false,
        }
    }

    /// Moves the cursor to where the next item, `width` wide and `spacing`
    /// after the previous one, goes.
    pub fn place(&mut self, ui: &Ui, width: f32, spacing: f32) {
        if self.placed {
            let end = ui.item_rect_max()[0] + spacing + width;
            // An item too wide for any line stays on the current one.
            if end <= self.end_x || width > self.end_x - self.start_x {
                ui.same_line_with_spacing(0.0, spacing);
            }
        }
        self.placed = true;
    }

    /// Outlined text, wrapping between words.
    pub fn stroke_text(
        &mut self,
        ui: &Ui,
        draw_list: &DrawListMut,
        text: &str,
        thick: f32,
        highlight: Option<StyleColor>,
    ) {
        for token in text.split_inclusive(char::is_whitespace) {
            let sz = ui.calc_text_size(token);
            self.place(ui, sz[0], 0.0);
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
                p,
                StrokeStyle {
                    thick,
                    fore: StyleColor::Text,
                    back: StyleColor::TitleBg,
                },
            );
            ui.dummy(sz);
        }
    }
}

/// Outlined text on its own lines.
pub fn stroke_text_with_highlight(
    ui: &Ui,
    draw_list: &DrawListMut,
    text: &str,
    thick: f32,
    highlight: Option<StyleColor>,
) {
    Flow::new(ui).stroke_text(ui, draw_list, text, thick, highlight);
}

/// Height of a word-wrapped multiline field filling the available width with
/// `text`: one line at least, `max_height` at most (it scrolls beyond that).
pub fn wrapped_field_height(ui: &Ui, text: &str, max_height: f32) -> f32 {
    let style = unsafe { ui.style() };
    let [pad_x, pad_y] = style.frame_padding();
    // Room for the scrollbar keeps the measured wrap from running wider than
    // the field's own.
    let width = ui.content_region_avail()[0] - 2.0 * pad_x - style.scrollbar_size();
    let text_height =
        ui.calc_text_size_with_opts(text, false, width.max(1.0))[1].max(ui.text_line_height());
    (text_height + 2.0 * pad_y).min(max_height)
}

/// A problem the user should see, drawn as a highlighted note after the
/// content it is about.
pub fn error_note(ui: &Ui, text: &str) {
    let draw_list = ui.get_window_draw_list();
    stroke_text_with_highlight(
        ui,
        &draw_list,
        text,
        1.0,
        Some(StyleColor::PlotLinesHovered),
    );
}

/// Highlight for text from an earlier line: the usual highlight in light gray.
pub fn earlier_line_highlight(ui: &Ui) -> [f32; 4] {
    let alpha = ui.style_color(StyleColor::TextSelectedBg)[3];
    [0.75, 0.75, 0.75, alpha]
}

/// Marks text from an earlier line by drawing its highlight in light gray
/// while the returned token lives.
pub fn push_earlier_line_highlight(ui: &Ui) -> ColorStackToken<'_> {
    ui.push_style_color(StyleColor::TextSelectedBg, earlier_line_highlight(ui))
}

/// Gap between glossed words, as a fraction of the style's item spacing.
const GLOSS_SPACING: f32 = 0.5;

/// How glossed text is inked.
#[derive(Clone, Copy, PartialEq)]
pub enum Ink {
    Plain,
    /// Outlined, for reading over busy backgrounds.
    Stroked,
    /// Dimmed with a drop shadow: text not glossed yet.
    Preview,
}

impl Ink {
    /// `Stroked` when the stroke setting is on, `Plain` otherwise.
    pub fn text(stroke: bool) -> Self {
        if stroke {
            Ink::Stroked
        } else {
            Ink::Plain
        }
    }
}

pub struct KanjiStyle {
    /// Color of the box behind the text, if any.
    pub highlight: Option<[f32; 4]>,
    pub ink: Ink,
    pub underline: UnderlineMode,
}
pub fn draw_kanji_text(
    ui: &Ui,
    flow: &mut Flow,
    text: &str,
    ruby_text: RubyTextMode,
    style: KanjiStyle,
) -> bool {
    let KanjiStyle {
        highlight,
        ink,
        underline,
    } = style;
    let ruby_sz = match ruby_text {
        RubyTextMode::Text(text) => ui.calc_text_size(text),
        RubyTextMode::Pad => [0.0, ui.text_line_height()],
        RubyTextMode::None => [0.0, 0.0],
    };

    let kanji_sz = FontSize::KANJI.with(ui, || ui.calc_text_size(text));

    let vpad = match ruby_text {
        RubyTextMode::None => 0.0,
        _ => px(ui, 8.0),
    };
    let w = f32::max(kanji_sz[0], ruby_sz[0]);
    let h = kanji_sz[1] + ruby_sz[1] + vpad;

    let spacing = unsafe { ui.style() }.item_spacing()[0] * GLOSS_SPACING;
    flow.place(ui, w, spacing);

    let x = ui.cursor_screen_pos()[0];
    let mut y = ui.cursor_screen_pos()[1] + vpad;

    let draw_list = ui.get_window_draw_list();

    let maybe_stroke_text = |text: &str, pos: [f32; 2], thick: f32| {
        if ink == Ink::Stroked {
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

    if let Some(color) = highlight {
        draw_list
            .add_rect([cx, y], [cx + kanji_sz[0], y + kanji_sz[1]], color)
            .rounding(px(ui, 5.0))
            .filled(true)
            .build();
    }

    let ul_thick = px(ui, 4.0);
    let ul0 = [x, y + kanji_sz[1] + ul_thick / 2.0];
    let ul1 = match underline {
        UnderlineMode::Normal => [x + w, y + kanji_sz[1] + ul_thick / 2.0],
        UnderlineMode::Pad => [x + w + spacing, y + kanji_sz[1] + ul_thick / 2.0],
        UnderlineMode::None => ul0,
    };
    draw_list
        .add_line(ul0, ul1, ui.style_color(StyleColor::Text))
        .thickness(ul_thick)
        .build();

    FontSize::KANJI.with(ui, || {
        if ink == Ink::Preview {
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
    });

    ui.dummy([w, h]);

    ui.is_window_focused()
        && ui.is_mouse_hovering_rect(
            [ul0[0], ul0[1] - ul_thick / 2.0],
            [ul1[0], ul1[1] + ul_thick / 2.0],
        )
}

/// A form laid out like the reflected settings grid: `pairs` label/widget
/// pairs per row, each label in a column sized to its longest label and each
/// widget filling the column after it. Fill it with [`form_cell`].
pub fn form_grid(ui: &Ui, id: &str, pairs: usize, cells: impl FnOnce()) {
    if let Some(_table) = ui.begin_table_with_flags(id, pairs * 2, TableFlags::NO_SAVED_SETTINGS) {
        for _ in 0..pairs {
            ui.table_setup_column(
                "##label",
                TableColumnFlags::NONE,
                Some(TableColumnWidth::Fixed(0.0)),
            );
            ui.table_setup_column(
                "##value",
                TableColumnFlags::NONE,
                Some(TableColumnWidth::Stretch(1.0)),
            );
        }
        cells();
    }
}

/// One label/widget pair of a [`form_grid`]; `widget` fills its column.
pub fn form_cell(ui: &Ui, label: &str, widget: impl FnOnce()) {
    let _id = ui.push_id(label);
    ui.table_next_column();
    ui.align_text_to_frame_padding();
    ui.text(label);
    ui.table_next_column();
    let _width = ui.push_item_width(-f32::MIN_POSITIVE);
    widget();
}

/// A single [`form_grid`] row.
pub fn form_row(ui: &Ui, label: &str, widget: impl FnOnce()) {
    form_grid(ui, label, 1, || form_cell(ui, label, widget));
}

/// A combo choosing an enum value, as wide as its widest value so a column
/// of them lines up.
pub fn combo_enum_fit<T>(ui: &Ui, label: impl AsRef<str>, val: &mut T)
where
    T: IntoEnumIterator + PartialEq,
    for<'a> &'a T: Into<&'static str>,
{
    let widest = T::iter()
        .map(|e| ui.calc_text_size(<&T as Into<&'static str>>::into(&e))[0])
        .fold(0.0, f32::max);
    let padding = unsafe { ui.style() }.frame_padding()[0];
    ui.set_next_item_width(widest + 2.0 * padding + ui.frame_height());
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
