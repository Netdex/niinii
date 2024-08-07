use core::slice;
use std::{
    collections::{HashMap, HashSet},
    io::Read,
    mem::size_of,
};

use bitflags::bitflags;
use flate2::bufread::GzDecoder;
use ichiran::prelude::*;
use imgui::*;

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
    /// Whether we are sharing the renderer context with another application or not.
    const SHARED_RENDER_CONTEXT = 1 << 1;
}
}

pub struct Context {
    font_data: Vec<u8>,
    fonts: HashMap<TextStyle, FontId>,

    added_font_glyphs: HashSet<u32>,
    font_glyph_ranges: Vec<u32>,
    font_glyph_range_size: usize,
    font_atlas_dirty: bool,

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
        let font_data = decompress_gzip_font(SARASA_MONO_J_REGULAR);

        let mut ctx = Context {
            font_data,
            fonts: HashMap::new(),

            added_font_glyphs: HashSet::new(),
            font_glyph_ranges,
            font_glyph_range_size: FONT_BASIC_RANGES_UTF8.len(),
            font_atlas_dirty: true,

            flags,
        };
        ctx.add_default_glyphs();
        ctx
    }
    pub fn flags(&self) -> &ContextFlags {
        &self.flags
    }
    pub fn font_atlas_dirty(&self) -> bool {
        self.font_atlas_dirty
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
    fn add_font(&mut self, style: TextStyle, font_id: FontId) {
        self.fonts.insert(style, font_id);
    }
    pub fn get_font(&self, style: TextStyle) -> FontId {
        *self.fonts.get(&style).unwrap()
    }
    pub fn update_fonts(&mut self, imgui: &mut imgui::Context, hidpi_factor: f64) -> bool {
        if !self.font_atlas_dirty {
            return false;
        }
        // let scaling_factor = hidpi_factor.max(1.0); // only scale fonts down
        let scaling_factor = hidpi_factor;

        imgui.fonts().clear();
        imgui.io_mut().font_global_scale = (1.0 / scaling_factor) as f32;

        let glyph_ranges = unsafe {
            let glyph_ranges = &mut self.font_glyph_ranges[0..self.font_glyph_range_size + 1];
            // can't safely pass a reference so make a copy and leak it
            let ptr = sys::igMemAlloc(glyph_ranges.len() * size_of::<u32>()) as *mut u32;
            assert!(!ptr.is_null());
            std::ptr::copy_nonoverlapping(glyph_ranges.as_ptr(), ptr, glyph_ranges.len());
            slice::from_raw_parts(ptr.cast(), glyph_ranges.len())
        };

        let ext_font_config = [FontConfig {
            rasterizer_multiply: if hidpi_factor < 1.0 { 1.0 } else { 1.75 },
            glyph_ranges: FontGlyphRanges::from_slice(glyph_ranges),
            oversample_h: if hidpi_factor < 1.0 { 3 } else { 2 },
            oversample_v: if hidpi_factor < 1.0 { 2 } else { 1 },
            ..Default::default()
        }];

        let mut create_font =
            |name: &str, font_data: &[u8], size_pt: f64, config: &[FontConfig]| {
                let font_sources: Vec<_> = config
                    .iter()
                    .map(|config| FontSource::TtfData {
                        data: font_data,
                        size_pixels: (size_pt * scaling_factor) as f32,
                        config: Some(FontConfig {
                            name: Some(name.to_string()),
                            ..config.clone()
                        }),
                    })
                    .collect();
                imgui.fonts().add_font(font_sources.as_slice())
            };

        self.add_font(
            TextStyle::Body,
            create_font("Body", &self.font_data, 18.0, &ext_font_config),
        );
        self.add_font(
            TextStyle::Kanji,
            create_font("Kanji", &self.font_data, 38.0, &ext_font_config),
        );

        self.font_atlas_dirty = false;
        true
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
        impl<'a> RootVisitor<'a> {
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
