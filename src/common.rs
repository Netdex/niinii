use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use imgui::*;

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub enum TextStyle {
    Kanji,
    Body,
}

#[derive(Default)]
pub struct Env {
    fonts: HashMap<TextStyle, FontId>,
}
impl Env {
    pub fn get_font(&self, style: TextStyle) -> FontId {
        *self.fonts.get(&style).unwrap()
    }
}

#[derive(Deserialize, Serialize)]
#[serde(remote = "ImString")]
pub struct ImStringDef(#[serde(getter = "ImString::to_string")] String);
impl From<ImStringDef> for ImString {
    fn from(str: ImStringDef) -> Self {
        ImString::new(str.0)
    }
}

static SARASA_MONO_J_REGULAR: &'static [u8] = include_bytes!("../res/sarasa-mono-j-regular.ttf");

pub fn init_fonts(env: &mut Env, imgui: &mut Context, hidpi_factor: f64) {
    let mut add_font = |style: TextStyle, font_data: &[u8], size_pt: f64, config: &[FontConfig]| {
        // let font_data = &fs::read(path).unwrap();
        let font_sources: Vec<_> = config
            .iter()
            .map(|config| FontSource::TtfData {
                data: font_data,
                size_pixels: (size_pt * hidpi_factor) as f32,
                config: Some(FontConfig {
                    name: Some(format!("{:?}", style)),
                    ..config.clone()
                }),
            })
            .collect();
        env.fonts
            .insert(style, imgui.fonts().add_font(font_sources.as_slice()));
    };
    let jp_font_config = [
        // japanese
        FontConfig {
            rasterizer_multiply: 1.75,
            glyph_ranges: FontGlyphRanges::japanese(),
            oversample_h: 2,
            ..Default::default()
        },
        // latin extended-a
        FontConfig {
            rasterizer_multiply: 1.75,
            glyph_ranges: FontGlyphRanges::from_slice(&[0x0100, 0x017F, 0x0]),
            oversample_h: 2,
            ..Default::default()
        },
    ];
    add_font(
        TextStyle::Body,
        SARASA_MONO_J_REGULAR,
        16.0,
        &jp_font_config,
    );
    add_font(
        TextStyle::Kanji,
        SARASA_MONO_J_REGULAR,
        40.0,
        &jp_font_config,
    );
    imgui.io_mut().font_global_scale = (1.0 / hidpi_factor) as f32;
}
