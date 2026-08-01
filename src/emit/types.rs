//! Laid-out intermediate types shared across layout, paginate, and paint.

use std::collections::BTreeMap;

use crate::error::WeaveError;
use crate::font::{
    FaceRef, FontBag, ShapedGlyph, collect_glyph_set, note_shaped_glyphs, shape_text, shaped_width,
};
use crate::image_prep::PreparedImage;

/// Original TrueType GID → Unicode text for `ToUnicode`.
pub(super) type GlyphSet = BTreeMap<u16, String>;
/// Per-face glyph sets collected during layout.
pub(super) type GlyphSets = BTreeMap<FaceRef, GlyphSet>;
/// Per-face subset packages ready for embedding.
pub(super) type SubsetMap = BTreeMap<FaceRef, crate::font::PreparedSubset>;

/// One forced-break boundary plus the items that follow until the next segment.
pub(super) type LayoutSegment = (ForcedBreak, Vec<LaidItem>);
/// Segments, decoded images, and glyph sets from [`super::layout::collect_layout`].
pub(super) type LayoutDoc = (Vec<LayoutSegment>, Vec<PreparedImage>, GlyphSets);

/// One shaped run on a line (single face + size).
#[derive(Debug, Clone)]
pub(super) struct LaidSpan {
    pub face: FaceRef,
    pub font_size: f32,
    pub glyphs: Vec<ShapedGlyph>,
}

/// One horizontal line of text (or an empty gap).
#[derive(Debug, Clone)]
pub(super) struct LaidLine {
    pub spans: Vec<LaidSpan>,
    /// Vertical advance consumed by this line (points).
    pub leading: f32,
    /// Prefer to keep this item with the next when paginating.
    pub glue_after: bool,
    /// Left indent inside the content box (points).
    pub indent: f32,
    /// Center within the content box (ignores `indent`).
    pub center: bool,
}

impl LaidLine {
    /// Vertical whitespace with no glyphs.
    pub(super) fn gap(leading: f32) -> Self {
        Self {
            spans: Vec::new(),
            leading,
            glue_after: false,
            indent: 0.0,
            center: false,
        }
    }

    /// Shape `text` and record glyphs into `glyph_sets`.
    pub(super) fn shaped(
        fonts: &FontBag,
        face: FaceRef,
        text: &str,
        font_size: f32,
        leading: f32,
        glyph_sets: &mut GlyphSets,
    ) -> Result<Self, WeaveError> {
        let glyphs = shape_text(fonts, face, text, font_size)?;
        let set = glyph_sets.entry(face).or_default();
        collect_glyph_set(fonts, face, text, set);
        note_shaped_glyphs(&glyphs, set);
        Ok(Self {
            spans: vec![LaidSpan {
                face,
                font_size,
                glyphs,
            }],
            leading,
            glue_after: false,
            indent: 0.0,
            center: false,
        })
    }

    /// Total advance width of all spans.
    pub(super) fn width(&self) -> f32 {
        self.spans.iter().map(|s| shaped_width(&s.glyphs)).sum()
    }
}

/// One table row after cell wrapping.
#[derive(Debug, Clone)]
pub(super) struct LaidTableRow {
    pub height: f32,
    /// Per-column wrapped lines.
    pub cells: Vec<Vec<LaidLine>>,
}

/// Drawn table grid with equal column widths.
#[derive(Debug, Clone)]
pub(super) struct LaidTable {
    pub col_widths: Vec<f32>,
    pub rows: Vec<LaidTableRow>,
    pub pad: f32,
    pub gap_after: f32,
}

impl LaidTable {
    /// Total height including trailing gap.
    pub(super) fn height(&self) -> f32 {
        self.rows.iter().map(|r| r.height).sum::<f32>() + self.gap_after
    }
}

/// One drawn element inside a [`LaidMath`] box (coords from the box top).
#[derive(Debug, Clone)]
pub(super) enum LaidMathEl {
    /// Glyph run; `y` is the text baseline distance from the box top.
    Text {
        x: f32,
        y: f32,
        face: FaceRef,
        font_size: f32,
        glyphs: Vec<ShapedGlyph>,
    },
    /// Horizontal rule; `y` is the stroke midline from the box top.
    Rule {
        x: f32,
        y: f32,
        width: f32,
        thickness: f32,
    },
    /// Stroked stretchy parenthesis; `axis_y` is the math-axis distance from the box top.
    Paren {
        x: f32,
        axis_y: f32,
        half_h: f32,
        width: f32,
        thickness: f32,
        left: bool,
    },
    /// Geometric arrow; `y` is the shaft midline from the box top.
    Arrow {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        thickness: f32,
        left: bool,
    },
}

/// Structured math formula (fractions, scripts, matrices).
#[derive(Debug, Clone)]
pub(super) struct LaidMath {
    pub width: f32,
    pub height: f32,
    pub center: bool,
    pub gap_after: f32,
    pub elements: Vec<LaidMathEl>,
}

/// One paint/pagination unit on a page.
#[derive(Debug, Clone)]
pub(super) enum LaidItem {
    Text(LaidLine),
    Image {
        img_idx: usize,
        width: f32,
        height: f32,
        glue_after: bool,
    },
    Table(LaidTable),
    Math(LaidMath),
}

impl LaidItem {
    /// Vertical space this item occupies (including image trailing gap).
    pub(super) fn height(&self) -> f32 {
        match self {
            Self::Text(line) => line.leading,
            Self::Image { height, .. } => *height + 8.0,
            Self::Table(table) => table.height(),
            Self::Math(math) => math.height + math.gap_after,
        }
    }

    /// Whether pagination should try to keep this item with the next.
    pub(super) fn glue_after(&self) -> bool {
        match self {
            Self::Text(line) => line.glue_after,
            Self::Image { glue_after, .. } => *glue_after,
            Self::Table(_) | Self::Math(_) => false,
        }
    }

    pub(super) fn set_glue_after(&mut self, glue: bool) {
        match self {
            Self::Text(line) => line.glue_after = glue,
            Self::Image { glue_after, .. } => *glue_after = glue,
            Self::Table(_) | Self::Math(_) => {}
        }
    }
}

/// Segment-level page break request before the segment's items.
#[derive(Debug, Clone, Copy)]
pub(super) enum ForcedBreak {
    None,
    Always,
}

/// Face resolution mode for styled runs.
#[derive(Debug, Clone, Copy)]
pub(super) enum FaceMode {
    Body,
    Heading,
}

/// Parameters for wrapping a sequence of [`crate::ir::TextRun`]s into lines.
#[derive(Clone, Copy)]
pub(super) struct RunLayout {
    pub font_size: f32,
    pub leading: f32,
    pub gap_after: f32,
    /// Glue the last content line to the following item (keep-with-next).
    pub glue_last_content: bool,
    pub mode: FaceMode,
    pub indent: f32,
}
