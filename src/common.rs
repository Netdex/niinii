use flate2::read::GzDecoder;
use imgui_winit_support::WinitPlatform;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, io::Read, path::PathBuf};

use crate::clipboard;
use imgui::*;

static SARASA_MONO_J_REGULAR: &'static [u8] = include_bytes!("../res/sarasa-mono-j-regular.ttf.gz");

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

fn decompress_gzip_font(font_data: &[u8]) -> Vec<u8> {
    let mut decoder = GzDecoder::new(font_data);
    let mut font_buf = vec![];
    decoder.read_to_end(&mut font_buf).unwrap();
    font_buf
}

pub fn init_fonts(env: &mut Env, imgui: &mut Context, hidpi_factor: f64) {
    let mut add_font = |style: TextStyle, font_data: &[u8], size_pt: f64, config: &[FontConfig]| {
        let font_buf = decompress_gzip_font(font_data);

        let font_sources: Vec<_> = config
            .iter()
            .map(|config| FontSource::TtfData {
                data: &font_buf,
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
    let ext_font_config = [
        FontConfig {
            rasterizer_multiply: 1.75,
            glyph_ranges: FontGlyphRanges::japanese(),
            oversample_h: 2,
            ..Default::default()
        },
        FontConfig {
            rasterizer_multiply: 1.75,
            glyph_ranges: FontGlyphRanges::from_slice(&[
                0x0100, 0x017F, // Latin Extended-A
                0x2000, 0x206F, // General Punctuation
                0x0,
            ]),
            oversample_h: 2,
            ..Default::default()
        },
    ];
    let jp_font_config = [FontConfig {
        rasterizer_multiply: 1.75,
        glyph_ranges: FontGlyphRanges::japanese(),
        oversample_h: 2,
        ..Default::default()
    }];
    add_font(
        TextStyle::Body,
        SARASA_MONO_J_REGULAR,
        16.0,
        &ext_font_config,
    );
    add_font(
        TextStyle::Kanji,
        SARASA_MONO_J_REGULAR,
        40.0,
        &jp_font_config,
    );
    imgui.io_mut().font_global_scale = (1.0 / hidpi_factor) as f32;
}

pub fn imgui_init(window: &winit::window::Window) -> (WinitPlatform, imgui::Context, Env) {
    let mut imgui = imgui::Context::create();
    imgui.set_ini_filename(Some(PathBuf::from("imgui.ini")));

    let io = imgui.io_mut();
    io.font_allow_user_scaling = true;

    if let Some(backend) = clipboard::init() {
        imgui.set_clipboard_backend(backend);
    } else {
        panic!("failed to initialize clipboard");
    }

    let mut platform = WinitPlatform::init(&mut imgui);
    platform.attach_window(
        imgui.io_mut(),
        window,
        imgui_winit_support::HiDpiMode::Default,
    );

    let mut env = Env::default();
    let hidpi_factor = platform.hidpi_factor();
    init_fonts(&mut env, &mut imgui, hidpi_factor);

    (platform, imgui, env)
}
