use std::io::Read;

use dear_imgui_rs::{Context, FontConfig, FontSource, Ui};
use flate2::bufread::GzDecoder;

const SARASA_MONO_J: &[u8] = include_bytes!("../../res/sarasa-mono-j-regular.ttf.gz");

/// A text size before DPI and user scaling. There is a single font; glyphs
/// are rasterized at whatever size text is drawn, on first use.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontSize(f32);

impl FontSize {
    pub const BODY: FontSize = FontSize(18.0);
    pub const KANJI: FontSize = FontSize(38.0);

    pub const fn scaled(self, scale: f32) -> FontSize {
        FontSize(self.0 * scale)
    }

    /// Runs `f` with text drawn at this size.
    pub fn with<R>(self, ui: &Ui, f: impl FnOnce() -> R) -> R {
        ui.with_font_and_size(None, self.0, f)
    }
}

/// Converts a length written for 100% scale with body text into pixels at the
/// current scale, which follows the monitor's DPI and the user's text scaling.
pub fn px(ui: &Ui, length: f32) -> f32 {
    length * ui.current_font_size() / FontSize::BODY.0
}

/// [`px`] for both components of a size.
pub fn px2(ui: &Ui, [w, h]: [f32; 2]) -> [f32; 2] {
    [px(ui, w), px(ui, h)]
}

/// Installs the CJK font as the default font.
pub(super) fn load(imgui: &mut Context) {
    let mut ttf = Vec::new();
    GzDecoder::new(SARASA_MONO_J)
        .read_to_end(&mut ttf)
        .expect("embedded font is valid gzip");
    let config = FontConfig::new().name("Sarasa Mono J");
    // SAFETY: `ttf` is a complete TrueType font and outlives `add_font`, which
    // copies it into the atlas.
    let source =
        unsafe { FontSource::ttf_data_with_size(&ttf, FontSize::BODY.0) }.with_config(config);
    imgui.font_atlas().add_font(&[source]);
}
