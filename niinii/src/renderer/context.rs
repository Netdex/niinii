use std::{
    collections::{HashMap, HashSet},
    io::Read,
    mem, ptr,
    sync::Arc,
    time::Instant,
};

use bitflags::bitflags;
use flate2::bufread::GzDecoder;
use futures::FutureExt;
use ichiran::prelude::*;
use imgui::{internal::RawCast, *};
use tokio::task::JoinHandle;

use super::ranges::*;

fn decompress_gzip_font(font_data: &[u8]) -> Vec<u8> {
    let mut decoder = GzDecoder::new(font_data);
    let mut font_buf = vec![];
    decoder.read_to_end(&mut font_buf).unwrap();
    font_buf
}

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub enum TextStyle {
    Kanji,
    Body,
}

bitflags! {
pub struct ContextFlags: u32 {
    /// Whether the renderer supports updating font atlases on the fly.
    const SUPPORTS_ATLAS_UPDATE = 1 << 0;
}
}

/// A fully rasterized font atlas that has not yet been installed into an imgui context.
struct BuiltAtlas {
    atlas: *mut sys::ImFontAtlas,
    fonts: HashMap<TextStyle, FontId>,
    scaling_factor: f64,
    // Referenced by the atlas without ownership, so they must outlive it.
    font_data: Arc<[u8]>,
    glyph_ranges: Box<[u32]>,
}
// The atlas is exclusively owned and does not reference any imgui context until installed.
unsafe impl Send for BuiltAtlas {}

impl Drop for BuiltAtlas {
    fn drop(&mut self) {
        if !self.atlas.is_null() {
            unsafe { sys::ImFontAtlas_destroy(self.atlas) };
        }
    }
}

impl BuiltAtlas {
    fn build(font_data: Arc<[u8]>, glyph_ranges: Box<[u32]>, hidpi_factor: f64) -> Self {
        // let scaling_factor = hidpi_factor.max(1.0); // only scale fonts down
        let scaling_factor = hidpi_factor;

        let atlas = unsafe {
            let atlas = sys::ImFontAtlas_ImFontAtlas();
            // https://github.com/imgui-rs/imgui-rs/issues/773
            (*atlas).FontBuilderIO = sys::ImGuiFreeType_GetBuilderForFreeType();
            atlas
        };
        let mut built = BuiltAtlas {
            atlas,
            fonts: HashMap::new(),
            scaling_factor,
            font_data,
            glyph_ranges,
        };

        let rasterizer_multiply = if hidpi_factor < 1.0 { 1.0 } else { 1.75 };
        let body = built.add_font("Body", 18.0 * scaling_factor, rasterizer_multiply);
        let kanji = built.add_font("Kanji", 38.0 * scaling_factor, rasterizer_multiply);
        built.fonts.insert(TextStyle::Body, body);
        built.fonts.insert(TextStyle::Kanji, kanji);

        unsafe { FontAtlas::from_raw_mut(&mut *built.atlas) }.build_rgba32_texture();
        built
    }

    fn add_font(&mut self, name: &str, size_pixels: f64, rasterizer_multiply: f32) -> FontId {
        unsafe {
            let default_config = sys::ImFontConfig_ImFontConfig();
            let mut config = ptr::read(default_config);
            sys::ImFontConfig_destroy(default_config);

            // imgui-rs's add_font copies the font data into the atlas, which is expensive
            // for a CJK font, so reference the shared buffer instead.
            config.FontData = self.font_data.as_ptr() as *mut _;
            config.FontDataSize = self.font_data.len() as i32;
            config.FontDataOwnedByAtlas = false;
            config.SizePixels = size_pixels as f32;
            config.GlyphRanges = self.glyph_ranges.as_ptr();
            config.RasterizerMultiply = rasterizer_multiply;
            let name = &name.as_bytes()[..name.len().min(config.Name.len() - 1)];
            for (dst, &src) in config.Name.iter_mut().zip(name) {
                *dst = src as _;
            }

            let font = sys::ImFontAtlas_AddFont(self.atlas, &config);
            assert!(!font.is_null());
            Font::from_raw(&*font).id()
        }
    }
}

pub struct Context {
    font_data: Arc<[u8]>,
    fonts: HashMap<TextStyle, FontId>,

    added_font_glyphs: HashSet<u32>,
    font_glyph_ranges: Vec<u32>,
    font_glyph_range_size: usize,
    font_atlas_dirty: bool,
    pending_atlas: Option<JoinHandle<BuiltAtlas>>,
    // Referenced by the installed atlas.
    atlas_glyph_ranges: Box<[u32]>,

    flags: ContextFlags,
}
unsafe impl Send for Context {}

impl Context {
    pub fn new(flags: ContextFlags) -> Self {
        const FONT_GLYPH_RANGE_BUFFER_SZ: usize = 16384;
        let mut font_glyph_ranges = vec![0; FONT_GLYPH_RANGE_BUFFER_SZ];
        font_glyph_ranges[0..FONT_BASIC_RANGES_UTF8.len()].copy_from_slice(FONT_BASIC_RANGES_UTF8);

        const SARASA_MONO_J_REGULAR: &[u8] =
            include_bytes!("../../res/sarasa-mono-j-regular.ttf.gz");
        let font_data = decompress_gzip_font(SARASA_MONO_J_REGULAR).into();

        let mut ctx = Context {
            font_data,
            fonts: HashMap::new(),

            added_font_glyphs: HashSet::new(),
            font_glyph_ranges,
            font_glyph_range_size: FONT_BASIC_RANGES_UTF8.len(),
            font_atlas_dirty: true,
            pending_atlas: None,
            atlas_glyph_ranges: Box::default(),

            flags,
        };
        ctx.add_default_glyphs();
        ctx
    }
    pub fn flags(&self) -> &ContextFlags {
        &self.flags
    }
    pub fn font_atlas_dirty(&self) -> bool {
        self.font_atlas_dirty || self.pending_atlas.is_some()
    }
    fn add_default_glyphs(&mut self) {
        let mut code: u32 = 0x4e00;
        for off in FONT_JA_ACC_OFF_4E00_UTF8 {
            code += *off as u32;
            self.add_font_glyph(code);
        }
    }
    fn add_font_glyph(&mut self, code: u32) {
        debug_assert!(!self.has_font_glyph(code));
        self.added_font_glyphs.insert(code);
        self.font_glyph_ranges[self.font_glyph_range_size] = code;
        self.font_glyph_ranges[self.font_glyph_range_size + 1] = code;
        self.font_glyph_range_size += 2;
        self.font_atlas_dirty = true;
    }
    fn has_font_glyph(&self, code: u32) -> bool {
        self.added_font_glyphs.contains(&code)
    }
    pub fn get_font(&self, style: TextStyle) -> FontId {
        *self.fonts.get(&style).unwrap()
    }
    fn glyph_ranges(&self) -> Box<[u32]> {
        self.font_glyph_ranges[0..self.font_glyph_range_size + 1].into()
    }
    /// Must be called between frames, while the current atlas is not locked.
    fn install_atlas(&mut self, imgui: &mut imgui::Context, mut built: BuiltAtlas) {
        let io = imgui.io_mut();
        io.font_global_scale = (1.0 / built.scaling_factor) as f32;
        unsafe {
            let new = mem::replace(&mut built.atlas, ptr::null_mut());
            // The context owns io.Fonts and destroys whatever it points to on shutdown.
            let old = mem::replace(&mut io.raw_mut().Fonts, new);
            sys::ImFontAtlas_destroy(old);
        }
        self.fonts = mem::take(&mut built.fonts);
        self.atlas_glyph_ranges = mem::take(&mut built.glyph_ranges);
    }
    /// Synchronously rebuilds the font atlas if it is dirty.
    pub fn update_fonts(&mut self, imgui: &mut imgui::Context, hidpi_factor: f64) -> bool {
        if !self.font_atlas_dirty {
            return false;
        }
        let built = BuiltAtlas::build(self.font_data.clone(), self.glyph_ranges(), hidpi_factor);
        self.install_atlas(imgui, built);
        self.font_atlas_dirty = false;
        true
    }
    /// Installs a finished background atlas build, and starts a new build if the atlas is dirty.
    /// Returns true if the atlas was replaced and the renderer must rebuild its font texture.
    pub fn poll_fonts(&mut self, imgui: &mut imgui::Context, hidpi_factor: f64) -> bool {
        let mut installed = false;
        if let Some(handle) = self.pending_atlas.as_mut() {
            let Some(result) = handle.now_or_never() else {
                return false;
            };
            self.pending_atlas = None;
            match result {
                Ok(built) => {
                    self.install_atlas(imgui, built);
                    installed = true;
                }
                Err(err) => tracing::error!(%err, "font atlas build failed"),
            }
        }
        if self.font_atlas_dirty {
            let font_data = self.font_data.clone();
            let glyph_ranges = self.glyph_ranges();
            self.pending_atlas = Some(tokio::task::spawn_blocking(move || {
                let now = Instant::now();
                let built = BuiltAtlas::build(font_data, glyph_ranges, hidpi_factor);
                tracing::info!("built font atlas (took {:?})", now.elapsed());
                built
            }));
            self.font_atlas_dirty = false;
        }
        installed
    }
    fn add_unknown_glyphs<T: AsRef<str>>(&mut self, text: T) {
        let text = text.as_ref();
        for c in text.chars() {
            if is_kanji(&c) {
                let code = c as u32;
                if !self.has_font_glyph(code) {
                    self.add_font_glyph(code);
                }
            }
        }
    }
    pub fn add_unknown_glyphs_from_root(&mut self, root: &Root) {
        struct RootVisitor<'a>(&'a mut Context);
        impl RootVisitor<'_> {
            fn visit_conj(&mut self, conj: &Conjugation) {
                if let Some(reading) = conj.reading() {
                    self.0.add_unknown_glyphs(reading);
                }
            }
            fn visit_meta(&mut self, meta: &Meta) {
                self.0.add_unknown_glyphs(meta.text());
            }
            fn visit_word(&mut self, word: &Word) {
                match word {
                    Word::Plain(plain) => {
                        self.visit_meta(plain.meta());
                        plain.conj().iter().for_each(|x| self.visit_conj(x));
                    }
                    Word::Compound(compound) => {
                        self.visit_meta(compound.meta());
                        compound
                            .components()
                            .iter()
                            .for_each(|x| self.visit_term(x))
                    }
                }
            }
            fn visit_term(&mut self, term: &Term) {
                match term {
                    Term::Word(word) => {
                        self.visit_word(word);
                    }
                    Term::Alternative(alt) => {
                        alt.alts().iter().for_each(|x| self.visit_word(x));
                    }
                }
            }

            fn visit_clause(&mut self, clause: &Clause) {
                clause
                    .romanized()
                    .iter()
                    .map(|x| x.term())
                    .for_each(|x| self.visit_term(x));
            }
            fn visit_segment(&mut self, segment: &Segment) {
                if let Segment::Clauses(clauses) = &segment {
                    clauses.iter().for_each(|x| self.visit_clause(x))
                }
            }
            pub fn visit_root(&mut self, root: &Root) {
                root.segments().iter().for_each(|x| self.visit_segment(x));
            }
        }
        let mut root_visitor = RootVisitor(self);
        root_visitor.visit_root(root);
    }
}
