//! Bundled Liberation fonts, rustybuzz shaping, subsetting, and Type0 PDF embedding.

use std::collections::BTreeMap;

use miniz_oxide::deflate::{CompressionLevel, compress_to_vec_zlib};
use pdf_writer::types::{CidFontType, FontFlags, SystemInfo, UnicodeCmap};
use pdf_writer::{Filter, Finish, Name, Pdf, Rect, Ref, Str};
use rustybuzz::{Face as RbFace, UnicodeBuffer};
use subsetter::GlyphRemapper;
use ttf_parser::{Face as TtfFace, GlyphId};

use crate::error::WeaveError;
use crate::ir::InlineStyle;
use crate::options::FontResolveMode;

const SYSTEM_INFO: SystemInfo = SystemInfo {
    registry: Str(b"Adobe"),
    ordering: Str(b"Identity"),
    supplement: 0,
};

const CMAP_NAME: Name = Name(b"Custom");

/// Which bundled face to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FaceId {
    /// Liberation Sans Regular.
    SansRegular,
    /// Liberation Sans Bold.
    SansBold,
    /// Liberation Sans Italic.
    SansItalic,
    /// Liberation Sans Bold Italic.
    SansBoldItalic,
    /// Liberation Serif Regular (manuscript body).
    SerifRegular,
    /// Liberation Serif Bold.
    SerifBold,
    /// Liberation Serif Italic.
    SerifItalic,
    /// Liberation Serif Bold Italic.
    SerifBoldItalic,
    /// Liberation Mono Regular (code).
    MonoRegular,
    /// Font Awesome Free Solid (`icons` feature).
    #[cfg(feature = "icons")]
    IconSolid,
    /// Font Awesome Free Regular (`icons` feature).
    #[cfg(feature = "icons")]
    IconRegular,
    /// Font Awesome Free Brands (`icons` feature).
    #[cfg(feature = "icons")]
    IconBrands,
    /// Sealed CJK sans fallback (`cjk` feature; tiny subset for CI/smoke).
    #[cfg(feature = "cjk")]
    CjkSans,
    /// Sealed emoji fallback (`emoji` feature; B&W Noto Emoji subset).
    #[cfg(feature = "emoji")]
    Emoji,
}

impl FaceId {
    /// Pick a face from inline style + whether the profile prefers serif body.
    #[must_use]
    pub fn from_style(style: &InlineStyle, serif_body: bool) -> Self {
        if style.code {
            return Self::MonoRegular;
        }
        match (style.strong, style.emphasis, serif_body) {
            (true, true, true) => Self::SerifBoldItalic,
            (true, false, true) => Self::SerifBold,
            (false, true, true) => Self::SerifItalic,
            (false, false, true) => Self::SerifRegular,
            (true, true, false) => Self::SansBoldItalic,
            (true, false, false) => Self::SansBold,
            (false, true, false) => Self::SansItalic,
            (false, false, false) => Self::SansRegular,
        }
    }

    fn ttf_bytes(self) -> &'static [u8] {
        match self {
            Self::SansRegular => {
                include_bytes!("../fonts/LiberationSans-Regular.ttf")
            }
            Self::SansBold => include_bytes!("../fonts/LiberationSans-Bold.ttf"),
            Self::SansItalic => include_bytes!("../fonts/LiberationSans-Italic.ttf"),
            Self::SansBoldItalic => {
                include_bytes!("../fonts/LiberationSans-BoldItalic.ttf")
            }
            Self::SerifRegular => {
                include_bytes!("../fonts/LiberationSerif-Regular.ttf")
            }
            Self::SerifBold => include_bytes!("../fonts/LiberationSerif-Bold.ttf"),
            Self::SerifItalic => {
                include_bytes!("../fonts/LiberationSerif-Italic.ttf")
            }
            Self::SerifBoldItalic => {
                include_bytes!("../fonts/LiberationSerif-BoldItalic.ttf")
            }
            Self::MonoRegular => {
                include_bytes!("../fonts/LiberationMono-Regular.ttf")
            }
            #[cfg(feature = "icons")]
            Self::IconSolid => include_bytes!("../fonts/fa-solid-900.ttf"),
            #[cfg(feature = "icons")]
            Self::IconRegular => include_bytes!("../fonts/fa-regular-400.ttf"),
            #[cfg(feature = "icons")]
            Self::IconBrands => include_bytes!("../fonts/fa-brands-400.ttf"),
            #[cfg(feature = "cjk")]
            Self::CjkSans => include_bytes!("../fonts/sealed-cjk-subset.ttf"),
            #[cfg(feature = "emoji")]
            Self::Emoji => include_bytes!("../fonts/sealed-emoji-subset.ttf"),
        }
    }

    fn postscript_name(self) -> &'static str {
        match self {
            Self::SansRegular => "LiberationSans",
            Self::SansBold => "LiberationSans-Bold",
            Self::SansItalic => "LiberationSans-Italic",
            Self::SansBoldItalic => "LiberationSans-BoldItalic",
            Self::SerifRegular => "LiberationSerif",
            Self::SerifBold => "LiberationSerif-Bold",
            Self::SerifItalic => "LiberationSerif-Italic",
            Self::SerifBoldItalic => "LiberationSerif-BoldItalic",
            Self::MonoRegular => "LiberationMono",
            #[cfg(feature = "icons")]
            Self::IconSolid => "FontAwesome6Free-Solid",
            #[cfg(feature = "icons")]
            Self::IconRegular => "FontAwesome6Free-Regular",
            #[cfg(feature = "icons")]
            Self::IconBrands => "FontAwesome6Brands-Regular",
            #[cfg(feature = "cjk")]
            Self::CjkSans => "SealedCjkSans",
            #[cfg(feature = "emoji")]
            Self::Emoji => "SealedNotoEmoji",
        }
    }

    fn resource_name(self) -> &'static [u8] {
        match self {
            Self::SansRegular => b"R",
            Self::SansBold => b"B",
            Self::SansItalic => b"I",
            Self::SansBoldItalic => b"BI",
            Self::SerifRegular => b"S",
            Self::SerifBold => b"SB",
            Self::SerifItalic => b"SI",
            Self::SerifBoldItalic => b"SBI",
            Self::MonoRegular => b"M",
            #[cfg(feature = "icons")]
            Self::IconSolid => b"IS",
            #[cfg(feature = "icons")]
            Self::IconRegular => b"IR",
            #[cfg(feature = "icons")]
            Self::IconBrands => b"IB",
            #[cfg(feature = "cjk")]
            Self::CjkSans => b"CJK",
            #[cfg(feature = "emoji")]
            Self::Emoji => b"EM",
        }
    }

    fn is_serif(self) -> bool {
        matches!(
            self,
            Self::SerifRegular | Self::SerifBold | Self::SerifItalic | Self::SerifBoldItalic
        )
    }

    fn is_italic(self) -> bool {
        matches!(
            self,
            Self::SansItalic | Self::SansBoldItalic | Self::SerifItalic | Self::SerifBoldItalic
        )
    }
}

/// Face used during layout/emit: sealed pack or a host-pinned TTF.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FaceRef {
    /// Built-in Liberation / optional icon, CJK, or emoji face.
    Bundled(FaceId),
    /// Index into [`FontBag::pinned`] (stable order from sorted pin ids).
    Pinned(u16),
}

impl From<FaceId> for FaceRef {
    fn from(id: FaceId) -> Self {
        Self::Bundled(id)
    }
}

/// Sealed faces plus host-pinned TTFs for one emit.
#[derive(Debug, Default)]
pub struct FontBag {
    /// `(id, ttf_bytes)` in sorted pin-id order.
    pinned: Vec<(String, Vec<u8>)>,
    /// How unknown [`crate::TextRun::face`] ids are handled.
    resolve: FontResolveMode,
}

impl FontBag {
    /// Register pinned faces from a sorted map (`BTreeMap` → deterministic indices).
    ///
    /// # Errors
    ///
    /// Returns [`WeaveError::Font`] if a face cannot be parsed as TrueType.
    pub fn from_pinned(
        pinned: &std::collections::BTreeMap<String, Vec<u8>>,
    ) -> Result<Self, WeaveError> {
        let mut bag = Self::default();
        for (id, bytes) in pinned {
            bag.pin_face(id.clone(), bytes.clone())?;
        }
        Ok(bag)
    }

    /// Set font resolve policy for layout (unknown face ids).
    pub fn set_resolve_mode(&mut self, mode: FontResolveMode) {
        self.resolve = mode;
    }

    /// Current resolve policy.
    #[must_use]
    pub fn resolve_mode(&self) -> FontResolveMode {
        self.resolve
    }

    /// Add a named TTF; returns its [`FaceRef::Pinned`] index.
    ///
    /// # Errors
    ///
    /// Duplicate ids, empty ids, or unparseable font bytes.
    pub fn pin_face(
        &mut self,
        id: impl Into<String>,
        bytes: Vec<u8>,
    ) -> Result<FaceRef, WeaveError> {
        let id = id.into();
        if id.is_empty() {
            return Err(WeaveError::Font("pinned face id must be non-empty".into()));
        }
        if self.pinned.iter().any(|(k, _)| k == &id) {
            return Err(WeaveError::Font(format!("duplicate pinned face `{id}`")));
        }
        TtfFace::parse(&bytes, 0).map_err(|e| {
            WeaveError::Font(format!("pinned face `{id}` is not a parseable TTF: {e:?}"))
        })?;
        let idx = u16::try_from(self.pinned.len())
            .map_err(|_| WeaveError::Font("too many pinned faces (u16::MAX)".into()))?;
        self.pinned.push((id, bytes));
        Ok(FaceRef::Pinned(idx))
    }

    /// Resolve a pin id to a [`FaceRef`], if registered.
    #[must_use]
    pub fn resolve_pin(&self, id: &str) -> Option<FaceRef> {
        self.pinned
            .iter()
            .position(|(k, _)| k == id)
            .map(|i| FaceRef::Pinned(i as u16))
    }

    /// TrueType bytes for `face`.
    ///
    /// # Errors
    ///
    /// Unknown pinned index.
    pub fn ttf_bytes(&self, face: FaceRef) -> Result<&[u8], WeaveError> {
        match face {
            FaceRef::Bundled(id) => Ok(id.ttf_bytes()),
            FaceRef::Pinned(i) => self
                .pinned
                .get(usize::from(i))
                .map(|(_, b)| b.as_slice())
                .ok_or_else(|| WeaveError::Font(format!("unknown pinned face index {i}"))),
        }
    }

    fn postscript_name(&self, face: FaceRef) -> String {
        match face {
            FaceRef::Bundled(id) => id.postscript_name().into(),
            FaceRef::Pinned(i) => {
                let raw = self
                    .pinned
                    .get(usize::from(i))
                    .map_or("pinned", |(k, _)| k.as_str());
                let safe: String = raw
                    .chars()
                    .map(|c| {
                        if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                            c
                        } else {
                            '-'
                        }
                    })
                    .collect();
                format!("Pinned-{safe}")
            }
        }
    }

    /// PDF resource name bytes (`R`, `B`, … or `P0`, `P1`, …).
    #[must_use]
    pub fn resource_name(&self, face: FaceRef) -> Vec<u8> {
        match face {
            FaceRef::Bundled(id) => id.resource_name().to_vec(),
            FaceRef::Pinned(i) => format!("P{i}").into_bytes(),
        }
    }

    fn is_serif(face: FaceRef) -> bool {
        match face {
            FaceRef::Bundled(id) => id.is_serif(),
            FaceRef::Pinned(_) => false,
        }
    }

    fn is_italic(face: FaceRef) -> bool {
        match face {
            FaceRef::Bundled(id) => id.is_italic(),
            FaceRef::Pinned(_) => false,
        }
    }
}

/// One shaped glyph with advance in PDF points (at the requested size).
#[derive(Debug, Clone, Copy)]
pub struct ShapedGlyph {
    /// TrueType glyph id (= CID for Identity mapping).
    pub gid: u16,
    /// Horizontal advance in points.
    pub advance: f32,
    /// True when this glyph maps a whitespace cluster (word-gap for justify).
    pub is_whitespace: bool,
    /// Ink right extent from glyph origin in points (`0` if unknown).
    pub ink_x_max: f32,
}

/// Shape `text` with a face from `fonts` at `font_size` points.
///
/// # Errors
///
/// Returns [`WeaveError::Font`] if the face cannot be parsed or shaped.
pub fn shape_text(
    fonts: &FontBag,
    face: FaceRef,
    text: &str,
    font_size: f32,
) -> Result<Vec<ShapedGlyph>, WeaveError> {
    let data = fonts.ttf_bytes(face)?;
    let rb = RbFace::from_slice(data, 0).ok_or_else(|| {
        WeaveError::Font(format!(
            "rustybuzz failed to parse {}",
            fonts.postscript_name(face)
        ))
    })?;
    let units = rb.units_per_em() as f32;
    let ttf = TtfFace::parse(data, 0);
    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(text);
    let glyphs = rustybuzz::shape(&rb, &[], buffer);
    let infos = glyphs.glyph_infos();
    let positions = glyphs.glyph_positions();
    let mut out = Vec::with_capacity(infos.len());
    for (info, pos) in infos.iter().zip(positions.iter()) {
        let advance = (pos.x_advance as f32) / units * font_size;
        let gid = info.glyph_id as u16;
        let is_whitespace = text
            .get(info.cluster as usize..)
            .and_then(|s| s.chars().next())
            .is_some_and(char::is_whitespace);
        let ink_x_max = ttf
            .as_ref()
            .ok()
            .and_then(|face| face.glyph_bounding_box(GlyphId(gid)))
            .map_or(0.0, |bbox| f32::from(bbox.x_max) / units * font_size);
        out.push(ShapedGlyph {
            gid,
            advance,
            is_whitespace,
            ink_x_max,
        });
    }
    Ok(out)
}

/// Whether `face` maps `ch` to a non-`.notdef` glyph.
#[must_use]
pub fn face_covers_char(fonts: &FontBag, face: FaceRef, ch: char) -> bool {
    let Ok(data) = fonts.ttf_bytes(face) else {
        return false;
    };
    let Ok(ttf) = TtfFace::parse(data, 0) else {
        return false;
    };
    ttf.glyph_index(ch).is_some_and(|g| g.0 != 0)
}

/// Conservative CJK / fullwidth / kana / hangul detector for sealed fallback.
#[cfg(feature = "cjk")]
#[must_use]
pub fn is_cjk_script_char(ch: char) -> bool {
    matches!(
        ch,
        '\u{1100}'..='\u{11FF}'   // Hangul Jamo
        | '\u{2E80}'..='\u{2EFF}' // CJK Radicals Supplement
        | '\u{2F00}'..='\u{2FDF}' // Kangxi Radicals
        | '\u{3000}'..='\u{303F}' // CJK Symbols and Punctuation
        | '\u{3040}'..='\u{309F}' // Hiragana
        | '\u{30A0}'..='\u{30FF}' // Katakana
        | '\u{3100}'..='\u{312F}' // Bopomofo
        | '\u{3130}'..='\u{318F}' // Hangul Compatibility Jamo
        | '\u{31A0}'..='\u{31BF}' // Bopomofo Extended
        | '\u{31F0}'..='\u{31FF}' // Katakana Phonetic Extensions
        | '\u{3200}'..='\u{32FF}' // Enclosed CJK Letters and Months
        | '\u{3300}'..='\u{33FF}' // CJK Compatibility
        | '\u{3400}'..='\u{4DBF}' // CJK Extension A
        | '\u{4E00}'..='\u{9FFF}' // CJK Unified Ideographs
        | '\u{A960}'..='\u{A97F}' // Hangul Jamo Extended-A
        | '\u{AC00}'..='\u{D7AF}' // Hangul Syllables
        | '\u{D7B0}'..='\u{D7FF}' // Hangul Jamo Extended-B
        | '\u{F900}'..='\u{FAFF}' // CJK Compatibility Ideographs
        | '\u{FF00}'..='\u{FFEF}' // Halfwidth and Fullwidth Forms
        | '\u{20000}'..='\u{2A6DF}' // Extension B
        | '\u{2A700}'..='\u{2B73F}'
        | '\u{2B740}'..='\u{2B81F}'
        | '\u{2B820}'..='\u{2CEAF}'
        | '\u{2CEB0}'..='\u{2EBEF}'
        | '\u{30000}'..='\u{3134F}'
    )
}

/// Conservative emoji / pictograph detector for sealed fallback.
#[cfg(feature = "emoji")]
#[must_use]
pub fn is_emoji_script_char(ch: char) -> bool {
    matches!(
        ch,
        '\u{200D}' // ZWJ
        | '\u{203C}'
        | '\u{2049}'
        | '\u{20E3}' // combining enclosing keycap
        | '\u{2122}'
        | '\u{2139}'
        | '\u{2194}'..='\u{2199}'
        | '\u{21A9}'..='\u{21AA}'
        | '\u{231A}'..='\u{231B}'
        | '\u{2328}'
        | '\u{23CF}'
        | '\u{23E9}'..='\u{23F3}'
        | '\u{23F8}'..='\u{23FA}'
        | '\u{24C2}'
        | '\u{25AA}'..='\u{25AB}'
        | '\u{25B6}'
        | '\u{25C0}'
        | '\u{25FB}'..='\u{25FE}'
        | '\u{2600}'..='\u{27BF}' // Misc symbols + dingbats (incl. ❤)
        | '\u{2934}'..='\u{2935}'
        | '\u{2B05}'..='\u{2B07}'
        | '\u{2B1B}'..='\u{2B1C}'
        | '\u{2B50}'
        | '\u{2B55}'
        | '\u{3030}'
        | '\u{303D}'
        | '\u{3297}'
        | '\u{3299}'
        | '\u{FE0E}'..='\u{FE0F}' // variation selectors
        | '\u{1F000}'..='\u{1FAFF}' // emoji blocks
    )
}

fn sealed_fallback_for_char(fonts: &FontBag, ch: char) -> Option<FaceRef> {
    #[cfg(feature = "emoji")]
    {
        if is_emoji_script_char(ch) {
            let face = FaceRef::Bundled(FaceId::Emoji);
            if face_covers_char(fonts, face, ch) {
                return Some(face);
            }
        }
    }
    #[cfg(feature = "cjk")]
    {
        if is_cjk_script_char(ch) {
            let face = FaceRef::Bundled(FaceId::CjkSans);
            if face_covers_char(fonts, face, ch) {
                return Some(face);
            }
        }
    }
    let _ = (fonts, ch);
    None
}

/// Resolve the face used for `ch`: primary if it covers the glyph (or is
/// whitespace/control), else a sealed CJK/emoji pack when compiled in.
#[must_use]
pub fn resolve_char_face(fonts: &FontBag, primary: FaceRef, ch: char) -> FaceRef {
    if ch.is_whitespace() || ch.is_control() || face_covers_char(fonts, primary, ch) {
        return primary;
    }
    sealed_fallback_for_char(fonts, ch).unwrap_or(primary)
}

/// Shape `text` with script fallback, returning contiguous `(face, glyphs)` runs.
///
/// Order: requested/`primary` → sealed `emoji` / `cjk` (when those Cargo features
/// are on and the face covers the codepoint) → stay on `primary` (`.notdef`).
///
/// # Errors
///
/// Returns [`WeaveError::Font`] if a selected face cannot be shaped.
pub fn shape_text_with_fallback(
    fonts: &FontBag,
    primary: FaceRef,
    text: &str,
    font_size: f32,
) -> Result<Vec<(FaceRef, Vec<ShapedGlyph>)>, WeaveError> {
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let mut runs: Vec<(FaceRef, String)> = Vec::new();
    for ch in text.chars() {
        let face = resolve_char_face(fonts, primary, ch);
        match runs.last_mut() {
            Some((f, buf)) if *f == face => buf.push(ch),
            _ => runs.push((face, ch.to_string())),
        }
    }
    let mut out = Vec::with_capacity(runs.len());
    for (face, segment) in runs {
        let glyphs = shape_text(fonts, face, &segment, font_size)?;
        out.push((face, glyphs));
    }
    Ok(out)
}

/// Total advance of multi-face shaped runs.
#[must_use]
pub fn shaped_runs_width(runs: &[(FaceRef, Vec<ShapedGlyph>)]) -> f32 {
    runs.iter().map(|(_, g)| shaped_width(g)).sum()
}

/// Total advance width of shaped glyphs.
#[must_use]
pub fn shaped_width(glyphs: &[ShapedGlyph]) -> f32 {
    glyphs.iter().map(|g| g.advance).sum()
}

/// Encode glyph ids as Identity-H show string (big-endian u16 pairs).
#[must_use]
pub fn encode_gids(glyphs: &[ShapedGlyph]) -> Vec<u8> {
    let mut out = Vec::with_capacity(glyphs.len() * 2);
    for g in glyphs {
        out.push((g.gid >> 8) as u8);
        out.push((g.gid & 0xff) as u8);
    }
    out
}

/// Collect glyph→unicode mapping from plain text (for `ToUnicode`).
pub fn collect_glyph_set(
    fonts: &FontBag,
    face: FaceRef,
    text: &str,
    into: &mut BTreeMap<u16, String>,
) {
    let Ok(data) = fonts.ttf_bytes(face) else {
        return;
    };
    let Ok(ttf) = TtfFace::parse(data, 0) else {
        return;
    };
    for ch in text.chars() {
        if let Some(gid) = ttf.glyph_index(ch) {
            into.entry(gid.0).or_insert_with(|| ch.to_string());
        }
    }
}

/// Ensure shaped glyph ids are present in the set (covers ligatures, etc.).
pub fn note_shaped_glyphs(glyphs: &[ShapedGlyph], into: &mut BTreeMap<u16, String>) {
    for g in glyphs {
        into.entry(g.gid).or_default();
    }
}

/// Subsetted face ready for PDF embedding (CIDs = remapped GIDs).
#[derive(Debug)]
pub struct PreparedSubset {
    /// Subset TTF bytes (cmap stripped; for PDF `FontFile2` only).
    pub data: Vec<u8>,
    /// Glyph set keyed by **new** subset GIDs (for widths + `ToUnicode`).
    pub glyph_set: BTreeMap<u16, String>,
    /// Old full-font GID → new subset GID.
    remapper: GlyphRemapper,
}

impl PreparedSubset {
    /// Remap a shaped glyph's GID into the subset.
    #[must_use]
    pub fn remap_glyph(&self, glyph: ShapedGlyph) -> ShapedGlyph {
        ShapedGlyph {
            gid: self.remapper.get(glyph.gid).unwrap_or(0),
            ..glyph
        }
    }
}

/// Subset a face to the glyphs in `glyph_set` (original GIDs).
///
/// # Errors
///
/// Returns [`WeaveError::Font`] if subsetting fails.
pub fn prepare_subset(
    fonts: &FontBag,
    face: FaceRef,
    glyph_set: &BTreeMap<u16, String>,
) -> Result<PreparedSubset, WeaveError> {
    let mut remapper = GlyphRemapper::new();
    for &gid in glyph_set.keys() {
        remapper.remap(gid);
    }
    let src = fonts.ttf_bytes(face)?;
    let data = subsetter::subset(src, 0, &remapper)
        .map_err(|e| WeaveError::Font(format!("subset {}: {e}", fonts.postscript_name(face))))?;

    let mut subset_glyphs = BTreeMap::new();
    for (&old, text) in glyph_set {
        if let Some(new_gid) = remapper.get(old) {
            subset_glyphs.insert(new_gid, text.clone());
        }
    }

    Ok(PreparedSubset {
        data,
        glyph_set: subset_glyphs,
        remapper,
    })
}

/// Indirect object ids for one embedded Type0 face.
#[derive(Debug, Clone, Copy)]
pub struct FontObjIds {
    /// Type0 font dictionary.
    pub type0: Ref,
    /// `CIDFontType2` descendant.
    pub cid: Ref,
    /// Font descriptor.
    pub descriptor: Ref,
    /// `ToUnicode` `CMap` stream.
    pub cmap: Ref,
    /// `FontFile2` stream.
    pub data: Ref,
}

/// Write Type0 + `CIDFontType2` + descriptor + font file + `ToUnicode` for `face`.
///
/// Embeds `font_data` (typically a subset) compressed. `glyph_set` must use the
/// same GIDs as that file (Identity CID↔GID).
///
/// # Errors
///
/// Returns [`WeaveError::Font`] if the face cannot be parsed.
pub fn write_embedded_font(
    pdf: &mut Pdf,
    fonts: &FontBag,
    face: FaceRef,
    font_data: &[u8],
    glyph_set: &BTreeMap<u16, String>,
    ids: FontObjIds,
) -> Result<(), WeaveError> {
    let ttf =
        TtfFace::parse(font_data, 0).map_err(|e| WeaveError::Font(format!("ttf-parser: {e:?}")))?;
    let units = f32::from(ttf.units_per_em());
    let to_font_units = |v: f32| (v / units) * 1000.0;

    let base_font = format!("AAAAAA+{}", fonts.postscript_name(face));

    pdf.type0_font(ids.type0)
        .base_font(Name(base_font.as_bytes()))
        .encoding_predefined(Name(b"Identity-H"))
        .descendant_font(ids.cid)
        .to_unicode(ids.cmap);

    {
        let mut cid = pdf.cid_font(ids.cid);
        cid.subtype(CidFontType::Type2);
        cid.base_font(Name(base_font.as_bytes()));
        cid.system_info(SYSTEM_INFO);
        cid.font_descriptor(ids.descriptor);
        cid.default_width(0.0);
        cid.cid_to_gid_map_predefined(Name(b"Identity"));

        let mut width_writer = cid.widths();
        for &g in glyph_set.keys() {
            let adv = ttf.glyph_hor_advance(GlyphId(g)).unwrap_or(0);
            let w = to_font_units(f32::from(adv));
            if w != 0.0 {
                width_writer.same(g, g, w);
            }
        }
        width_writer.finish();
    }

    let mut flags = FontFlags::empty();
    flags.set(FontFlags::SERIF, FontBag::is_serif(face));
    flags.set(FontFlags::FIXED_PITCH, ttf.is_monospaced());
    flags.set(FontFlags::ITALIC, FontBag::is_italic(face));
    flags.insert(FontFlags::SYMBOLIC);

    let global_bbox = ttf.global_bounding_box();
    let bbox = Rect::new(
        to_font_units(f32::from(global_bbox.x_min)),
        to_font_units(f32::from(global_bbox.y_min)),
        to_font_units(f32::from(global_bbox.x_max)),
        to_font_units(f32::from(global_bbox.y_max)),
    );
    let italic_angle = ttf.italic_angle();
    let ascender = to_font_units(f32::from(
        ttf.typographic_ascender().unwrap_or(ttf.ascender()),
    ));
    let descender = to_font_units(f32::from(
        ttf.typographic_descender().unwrap_or(ttf.descender()),
    ));
    let cap_height = to_font_units(f32::from(ttf.capital_height().unwrap_or(ttf.ascender())));
    let stem_v = 10.0 + 0.244 * (f32::from(ttf.weight().to_number()) - 50.0);

    {
        let mut desc = pdf.font_descriptor(ids.descriptor);
        desc.name(Name(base_font.as_bytes()))
            .flags(flags)
            .bbox(bbox)
            .italic_angle(italic_angle)
            .ascent(ascender)
            .descent(descender)
            .cap_height(cap_height)
            .stem_v(stem_v)
            .font_file2(ids.data);
    }

    let cmap = create_cmap(glyph_set);
    let cmap_bytes = cmap.finish();
    pdf.cmap(ids.cmap, cmap_bytes.as_slice());

    let compressed = compress_to_vec_zlib(font_data, CompressionLevel::DefaultLevel as u8);
    pdf.stream(ids.data, &compressed)
        .filter(Filter::FlateDecode)
        .pair(
            Name(b"Length1"),
            i32::try_from(font_data.len()).unwrap_or(i32::MAX),
        );

    Ok(())
}

fn create_cmap(glyph_set: &BTreeMap<u16, String>) -> UnicodeCmap {
    let mut cmap = UnicodeCmap::new(CMAP_NAME, SYSTEM_INFO);
    for (&g, text) in glyph_set {
        if !text.is_empty() {
            cmap.pair_with_multiple(g, text.chars());
        }
    }
    cmap
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bag() -> FontBag {
        FontBag::default()
    }

    #[test]
    fn shapes_ascii() {
        let fonts = bag();
        let glyphs = shape_text(&fonts, FaceId::SansRegular.into(), "Hello", 12.0).expect("shape");
        assert!(!glyphs.is_empty());
        assert!(shaped_width(&glyphs) > 0.0);
    }

    #[test]
    fn serif_body_picks_serif_faces() {
        let emph = InlineStyle {
            emphasis: true,
            ..InlineStyle::default()
        };
        let strong = InlineStyle {
            strong: true,
            ..InlineStyle::default()
        };
        assert_eq!(FaceId::from_style(&emph, true), FaceId::SerifItalic);
        assert_eq!(FaceId::from_style(&strong, true), FaceId::SerifBold);
        assert_eq!(
            FaceId::from_style(&InlineStyle::default(), true),
            FaceId::SerifRegular
        );
    }

    #[test]
    fn shapes_unicode_beyond_winansi() {
        let fonts = bag();
        // Em-dash — previously rejected by Helvetica MVP path.
        let glyphs = shape_text(&fonts, FaceId::SansRegular.into(), "a—b", 12.0).expect("shape");
        assert!(glyphs.len() >= 3);
    }

    #[test]
    fn subset_is_much_smaller_than_full_face() {
        let fonts = bag();
        let face = FaceRef::Bundled(FaceId::SansRegular);
        let mut set = BTreeMap::new();
        collect_glyph_set(&fonts, face, "Hello world", &mut set);
        let prepared = prepare_subset(&fonts, face, &set).expect("subset");
        let full = fonts.ttf_bytes(face).expect("bytes").len();
        assert!(
            prepared.data.len() * 10 < full,
            "subset {} vs full {full}",
            prepared.data.len()
        );
    }

    #[test]
    fn pinned_face_shapes_and_subsets() {
        let mut fonts = FontBag::default();
        let face = fonts
            .pin_face(
                "mono-pin",
                include_bytes!("../fonts/LiberationMono-Regular.ttf").to_vec(),
            )
            .expect("pin");
        let glyphs = shape_text(&fonts, face, "Pin", 12.0).expect("shape");
        assert!(!glyphs.is_empty());
        let mut set = BTreeMap::new();
        collect_glyph_set(&fonts, face, "Pin", &mut set);
        note_shaped_glyphs(&glyphs, &mut set);
        let prepared = prepare_subset(&fonts, face, &set).expect("subset");
        assert!(prepared.data.len() < fonts.ttf_bytes(face).expect("bytes").len());
        assert_eq!(fonts.resource_name(face), b"P0");
    }

    #[cfg(feature = "icons")]
    #[test]
    fn shapes_and_subsets_fa_solid_house() {
        let fonts = bag();
        let face = FaceRef::Bundled(FaceId::IconSolid);
        // Font Awesome Free 6 solid "house" (Private Use Area).
        const HOUSE: &str = "\u{f015}";
        let glyphs = shape_text(&fonts, face, HOUSE, 16.0).expect("shape");
        assert_eq!(glyphs.len(), 1);
        assert_ne!(glyphs[0].gid, 0);
        assert!(shaped_width(&glyphs) > 0.0);

        let mut set = BTreeMap::new();
        collect_glyph_set(&fonts, face, HOUSE, &mut set);
        note_shaped_glyphs(&glyphs, &mut set);
        let prepared = prepare_subset(&fonts, face, &set).expect("subset");
        let remapped = prepared.remap_glyph(glyphs[0]);
        assert_ne!(remapped.gid, 0);
        assert!(prepared.data.len() < fonts.ttf_bytes(face).expect("bytes").len());
    }

    #[cfg(feature = "icons")]
    #[test]
    fn shapes_fa_brands_and_regular() {
        let fonts = bag();
        // Brands "github" / Regular "user" — common Free codepoints.
        let brands =
            shape_text(&fonts, FaceId::IconBrands.into(), "\u{f09b}", 14.0).expect("brands");
        assert_eq!(brands.len(), 1);
        assert_ne!(brands[0].gid, 0);
        let regular =
            shape_text(&fonts, FaceId::IconRegular.into(), "\u{f007}", 14.0).expect("regular");
        assert_eq!(regular.len(), 1);
        assert_ne!(regular[0].gid, 0);
    }

    #[cfg(feature = "cjk")]
    #[test]
    fn shapes_and_subsets_sealed_cjk() {
        let fonts = bag();
        let face = FaceRef::Bundled(FaceId::CjkSans);
        let text = "中文";
        let glyphs = shape_text(&fonts, face, text, 14.0).expect("shape");
        assert_eq!(glyphs.len(), 2);
        assert!(glyphs.iter().all(|g| g.gid != 0));
        let mut set = BTreeMap::new();
        collect_glyph_set(&fonts, face, text, &mut set);
        note_shaped_glyphs(&glyphs, &mut set);
        let prepared = prepare_subset(&fonts, face, &set).expect("subset");
        assert!(prepared.data.len() <= fonts.ttf_bytes(face).expect("bytes").len());
    }

    #[cfg(feature = "cjk")]
    #[test]
    fn latin_run_falls_back_to_sealed_cjk() {
        let fonts = bag();
        let primary = FaceRef::Bundled(FaceId::SansRegular);
        let runs = shape_text_with_fallback(&fonts, primary, "Hi中文", 12.0).expect("shape");
        assert!(runs.len() >= 2);
        assert_eq!(runs[0].0, primary);
        assert_eq!(runs[1].0, FaceRef::Bundled(FaceId::CjkSans));
        assert!(runs[1].1.iter().all(|g| g.gid != 0));
    }

    #[cfg(feature = "emoji")]
    #[test]
    fn shapes_and_subsets_sealed_emoji() {
        let fonts = bag();
        let face = FaceRef::Bundled(FaceId::Emoji);
        let text = "😀";
        let glyphs = shape_text(&fonts, face, text, 16.0).expect("shape");
        assert_eq!(glyphs.len(), 1);
        assert_ne!(glyphs[0].gid, 0);
        let mut set = BTreeMap::new();
        collect_glyph_set(&fonts, face, text, &mut set);
        note_shaped_glyphs(&glyphs, &mut set);
        let prepared = prepare_subset(&fonts, face, &set).expect("subset");
        assert!(prepared.data.len() <= fonts.ttf_bytes(face).expect("bytes").len());
    }

    #[cfg(feature = "emoji")]
    #[test]
    fn latin_run_falls_back_to_sealed_emoji() {
        let fonts = bag();
        let primary = FaceRef::Bundled(FaceId::SansRegular);
        let runs = shape_text_with_fallback(&fonts, primary, "Hi🔥", 12.0).expect("shape");
        assert!(runs.len() >= 2);
        assert_eq!(runs[0].0, primary);
        assert_eq!(runs[1].0, FaceRef::Bundled(FaceId::Emoji));
        assert!(runs[1].1.iter().all(|g| g.gid != 0));
    }
}
