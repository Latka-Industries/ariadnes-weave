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
    /// Liberation Mono Regular (code).
    MonoRegular,
}

impl FaceId {
    /// Pick a face from inline style + whether the profile prefers serif body.
    #[must_use]
    pub fn from_style(style: &InlineStyle, serif_body: bool) -> Self {
        if style.code {
            return Self::MonoRegular;
        }
        match (style.strong, style.emphasis, serif_body) {
            (true, true, _) => Self::SansBoldItalic,
            (true, false, _) => Self::SansBold,
            (false, true, _) => Self::SansItalic,
            (false, false, true) => Self::SerifRegular,
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
            Self::MonoRegular => {
                include_bytes!("../fonts/LiberationMono-Regular.ttf")
            }
        }
    }

    fn postscript_name(self) -> &'static str {
        match self {
            Self::SansRegular => "LiberationSans",
            Self::SansBold => "LiberationSans-Bold",
            Self::SansItalic => "LiberationSans-Italic",
            Self::SansBoldItalic => "LiberationSans-BoldItalic",
            Self::SerifRegular => "LiberationSerif",
            Self::MonoRegular => "LiberationMono",
        }
    }

    fn resource_name(self) -> &'static [u8] {
        match self {
            Self::SansRegular => b"R",
            Self::SansBold => b"B",
            Self::SansItalic => b"I",
            Self::SansBoldItalic => b"BI",
            Self::SerifRegular => b"S",
            Self::MonoRegular => b"M",
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

/// Shape `text` with a bundled face at `font_size` points.
///
/// # Errors
///
/// Returns [`WeaveError::Font`] if the face cannot be parsed or shaped.
pub fn shape_text(
    face_id: FaceId,
    text: &str,
    font_size: f32,
) -> Result<Vec<ShapedGlyph>, WeaveError> {
    let data = face_id.ttf_bytes();
    let rb = RbFace::from_slice(data, 0).ok_or_else(|| {
        WeaveError::Font(format!(
            "rustybuzz failed to parse {}",
            face_id.postscript_name()
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

/// Collect glyph→unicode mapping from plain text (for ToUnicode).
pub fn collect_glyph_set(face_id: FaceId, text: &str, into: &mut BTreeMap<u16, String>) {
    let Ok(ttf) = TtfFace::parse(face_id.ttf_bytes(), 0) else {
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
    /// Subset TTF bytes (cmap stripped; for PDF FontFile2 only).
    pub data: Vec<u8>,
    /// Glyph set keyed by **new** subset GIDs (for widths + ToUnicode).
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

/// Subset a bundled face to the glyphs in `glyph_set` (original GIDs).
///
/// # Errors
///
/// Returns [`WeaveError::Font`] if subsetting fails.
pub fn prepare_subset(
    face_id: FaceId,
    glyph_set: &BTreeMap<u16, String>,
) -> Result<PreparedSubset, WeaveError> {
    let mut remapper = GlyphRemapper::new();
    for &gid in glyph_set.keys() {
        remapper.remap(gid);
    }
    let data = subsetter::subset(face_id.ttf_bytes(), 0, &remapper)
        .map_err(|e| WeaveError::Font(format!("subset {}: {e}", face_id.postscript_name())))?;

    let mut remapped = BTreeMap::new();
    for (&old, text) in glyph_set {
        if let Some(new) = remapper.get(old) {
            remapped.insert(new, text.clone());
        }
    }

    Ok(PreparedSubset {
        data,
        glyph_set: remapped,
        remapper,
    })
}

/// Indirect object ids for one embedded Type0 face.
#[derive(Debug, Clone, Copy)]
pub struct FontObjIds {
    /// Type0 font dictionary.
    pub type0: Ref,
    /// CIDFontType2 descendant.
    pub cid: Ref,
    /// Font descriptor.
    pub descriptor: Ref,
    /// ToUnicode CMap stream.
    pub cmap: Ref,
    /// FontFile2 stream.
    pub data: Ref,
}

/// Write Type0 + CIDFontType2 + descriptor + font file + ToUnicode for `face_id`.
///
/// Embeds `font_data` (typically a subset) compressed. `glyph_set` must use the
/// same GIDs as that file (Identity CID↔GID).
///
/// # Errors
///
/// Returns [`WeaveError::Font`] if the face cannot be parsed.
pub fn write_embedded_font(
    pdf: &mut Pdf,
    face_id: FaceId,
    font_data: &[u8],
    glyph_set: &BTreeMap<u16, String>,
    ids: FontObjIds,
) -> Result<(), WeaveError> {
    let ttf =
        TtfFace::parse(font_data, 0).map_err(|e| WeaveError::Font(format!("ttf-parser: {e:?}")))?;
    let units = f32::from(ttf.units_per_em());
    let to_font_units = |v: f32| (v / units) * 1000.0;

    let base_font = format!("AAAAAA+{}", face_id.postscript_name());

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
    flags.set(FontFlags::SERIF, matches!(face_id, FaceId::SerifRegular));
    flags.set(FontFlags::FIXED_PITCH, ttf.is_monospaced());
    flags.set(
        FontFlags::ITALIC,
        matches!(face_id, FaceId::SansItalic | FaceId::SansBoldItalic),
    );
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

/// Public resource name bytes for a face (for page resources / set_font).
#[must_use]
pub fn resource_name(face_id: FaceId) -> &'static [u8] {
    face_id.resource_name()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_ascii() {
        let glyphs = shape_text(FaceId::SansRegular, "Hello", 12.0).expect("shape");
        assert!(!glyphs.is_empty());
        assert!(shaped_width(&glyphs) > 0.0);
    }

    #[test]
    fn shapes_unicode_beyond_winansi() {
        // Em-dash — previously rejected by Helvetica MVP path.
        let glyphs = shape_text(FaceId::SansRegular, "a—b", 12.0).expect("shape");
        assert!(glyphs.len() >= 3);
    }

    #[test]
    fn subset_is_much_smaller_than_full_face() {
        let mut set = BTreeMap::new();
        collect_glyph_set(FaceId::SansRegular, "Hello world", &mut set);
        let prepared = prepare_subset(FaceId::SansRegular, &set).expect("subset");
        let full = FaceId::SansRegular.ttf_bytes().len();
        assert!(
            prepared.data.len() * 10 < full,
            "subset {} vs full {full}",
            prepared.data.len()
        );
    }
}
