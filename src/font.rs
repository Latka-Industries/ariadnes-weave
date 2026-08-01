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
    /// Built-in Liberation / optional icon face.
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
}

impl FontBag {
    /// Register pinned faces from a sorted map (BTreeMap → deterministic indices).
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
                    .map(|(k, _)| k.as_str())
                    .unwrap_or("pinned");
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

    fn is_serif(&self, face: FaceRef) -> bool {
        match face {
            FaceRef::Bundled(id) => id.is_serif(),
            FaceRef::Pinned(_) => false,
        }
    }

    fn is_italic(&self, face: FaceRef) -> bool {
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
    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(text);
    let glyphs = rustybuzz::shape(&rb, &[], buffer);
    let infos = glyphs.glyph_infos();
    let positions = glyphs.glyph_positions();
    let mut out = Vec::with_capacity(infos.len());
    for (info, pos) in infos.iter().zip(positions.iter()) {
        let advance = (pos.x_advance as f32) / units * font_size;
        out.push(ShapedGlyph {
            gid: info.glyph_id as u16,
            advance,
        });
    }
    Ok(out)
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
            advance: glyph.advance,
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
    flags.set(FontFlags::SERIF, fonts.is_serif(face));
    flags.set(FontFlags::FIXED_PITCH, ttf.is_monospaced());
    flags.set(FontFlags::ITALIC, fonts.is_italic(face));
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
}
