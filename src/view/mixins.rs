use imgui::{DrawListMut, StyleColor, Ui};

use crate::backend::env::{Env, TextStyle};

pub fn help_marker(ui: &Ui, text: &str) {
    ui.text_colored(ui.style_color(StyleColor::TextDisabled), "[?]");
    if ui.is_item_hovered() {
        ui.tooltip_text(text);
    }
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

pub fn stroke_text_with_offsets(
    ui: &Ui,
    draw_list: &DrawListMut,
    text: &str,
    pos: [f32; 2],
    thick: f32,
    fore: StyleColor,
    back: StyleColor,
    offsets: &[[f32; 2]],
) {
    for off in offsets {
        draw_list.add_text(
            [pos[0] + off[0] * thick, pos[1] + off[1] * thick],
            ui.style_color(back),
            text,
        );
    }
    draw_list.add_text(pos, ui.style_color(fore), text);
}
pub fn stroke_text_with_color(
    ui: &Ui,
    draw_list: &DrawListMut,
    text: &str,
    pos: [f32; 2],
    thick: f32,
    fore: StyleColor,
    back: StyleColor,
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
    stroke_text_with_offsets(ui, draw_list, text, pos, thick, fore, back, &offsets);
}
pub fn stroke_text(ui: &Ui, draw_list: &DrawListMut, text: &str, pos: [f32; 2], thick: f32) {
    stroke_text_with_color(
        ui,
        draw_list,
        text,
        pos,
        thick,
        StyleColor::Text,
        StyleColor::TitleBg,
    )
}

pub fn draw_kanji_text(
    ui: &Ui,
    env: &Env,
    text: &str,
    highlight: bool,
    stroke: bool,
    preview: bool,
    underline: UnderlineMode,
    ruby_text: RubyTextMode,
) -> bool {
    let ruby_sz = match ruby_text {
        RubyTextMode::Text(text) => ui.calc_text_size(text),
        RubyTextMode::Pad => [0.0, ui.text_line_height()],
        RubyTextMode::None => [0.0, 0.0],
    };

    let _kanji_font_token = ui.push_font(env.get_font(TextStyle::Kanji));
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
            stroke_text_with_color(
                ui,
                &draw_list,
                text,
                pos,
                thick,
                StyleColor::Text,
                StyleColor::TitleBg,
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

    let style = ui.clone_style();
    let ul_thick = 4.0;
    let ul0 = [x, y + kanji_sz[1] + ul_thick / 2.0];
    let ul1 = match underline {
        UnderlineMode::Normal => [x + w, y + kanji_sz[1] + ul_thick / 2.0],
        UnderlineMode::Pad => [
            x + w + style.item_spacing[0],
            y + kanji_sz[1] + ul_thick / 2.0,
        ],
        UnderlineMode::None => ul0,
    };
    draw_list
        .add_line(ul0, ul1, ui.style_color(StyleColor::Text))
        .thickness(ul_thick)
        .build();

    let _kanji_font_token = ui.push_font(env.get_font(TextStyle::Kanji));
    if preview {
        stroke_text_with_offsets(
            ui,
            &draw_list,
            text,
            [cx, y],
            2.0,
            StyleColor::TextDisabled,
            StyleColor::MenuBarBg,
            &[[1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
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

pub fn wrap_line(ui: &Ui, expected_width: f32) -> bool {
    let max_width = ui.window_content_region_max()[0];
    let visible_x = ui.window_pos()[0] + max_width;
    let last_x = ui.item_rect_max()[0];
    let style = ui.clone_style();
    let next_x = last_x + style.item_spacing[0] + expected_width;
    // don't wrap if it will fit on the current line, or if it won't even fit on an empty line
    if next_x < visible_x || expected_width >= max_width {
        ui.same_line();
        false
    } else {
        true
    }
}
