use std::{collections::HashMap, io::Read, path::PathBuf};

use flate2::read::GzDecoder;
use glutin::{platform::windows::WindowBuilderExtWindows, window};
use imgui::{FontConfig, FontGlyphRanges, FontId, FontSource};
use imgui_winit_support::WinitPlatform;

use crate::clipboard;
use crate::{app::App, view::settings::SettingsView};

static SARASA_MONO_J_REGULAR: &[u8] = include_bytes!("../../res/sarasa-mono-j-regular.ttf.gz");

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

fn decompress_gzip_font(font_data: &[u8]) -> Vec<u8> {
    let mut decoder = GzDecoder::new(font_data);
    let mut font_buf = vec![];
    decoder.read_to_end(&mut font_buf).unwrap();
    font_buf
}

pub trait Renderer {
    fn new(settings: &SettingsView) -> Self;
    fn main_loop(&mut self, app: &mut App);

    fn create_window_builder(settings: &SettingsView) -> window::WindowBuilder {
        let transparent = settings.transparent || settings.overlay_mode;
        let on_top = settings.on_top || settings.overlay_mode;
        let maximized = settings.overlay_mode;
        let decorations = !settings.overlay_mode;

        window::WindowBuilder::new()
            .with_title("niinii")
            .with_inner_size(glutin::dpi::LogicalSize::new(768, 768))
            .with_transparent(transparent)
            .with_drag_and_drop(false)
            .with_maximized(maximized)
            .with_decorations(decorations)
            .with_always_on_top(on_top)
    }

    fn create_imgui() -> imgui::Context {
        let mut imgui = imgui::Context::create();
        imgui.set_ini_filename(Some(PathBuf::from("imgui.ini")));

        let io = imgui.io_mut();
        io.font_allow_user_scaling = true;

        if let Some(backend) = clipboard::init() {
            imgui.set_clipboard_backend(backend);
        } else {
            panic!("failed to initialize clipboard");
        }
        imgui
    }

    fn create_platform(
        imgui: &mut imgui::Context,
        window: &winit::window::Window,
    ) -> WinitPlatform {
        let mut platform = WinitPlatform::init(imgui);
        platform.attach_window(
            imgui.io_mut(),
            window,
            imgui_winit_support::HiDpiMode::Default,
        );
        platform
    }

    fn create_fonts(imgui: &mut imgui::Context, env: &mut Env, platform: &WinitPlatform) {
        let hidpi_factor = platform.hidpi_factor();
        imgui.io_mut().font_global_scale = (1.0 / hidpi_factor) as f32;

        let mut add_font =
            |style: TextStyle, font_data: &[u8], size_pt: f64, config: &[FontConfig]| {
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
                oversample_h: 1,
                ..Default::default()
            },
        ];

        add_font(
            TextStyle::Body,
            SARASA_MONO_J_REGULAR,
            18.0,
            &ext_font_config,
        );
        add_font(
            TextStyle::Kanji,
            SARASA_MONO_J_REGULAR,
            40.0,
            &ext_font_config,
        );
    }
}
