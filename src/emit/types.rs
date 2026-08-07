//! Laid-out intermediate types shared across layout, paginate, and paint.

use std::collections::BTreeMap;

use crate::error::WeaveError;
use crate::font::{
    FaceRef, FontBag, ShapedGlyph, collect_glyph_set, note_shaped_glyphs, resolve_char_face,
    shape_text_with_fallback, shaped_runs_width, shaped_width,
};
use crate::image_prep::PreparedImage;
use crate::knobs::{FigureAlign, TextAlign};

/// Original TrueType GID → Unicode text for `ToUnicode`.
pub(super) type GlyphSet = BTreeMap<u16, String>;
/// Per-face glyph sets collected during layout.
pub(super) type GlyphSets = BTreeMap<FaceRef, GlyphSet>;
/// Per-face subset packages ready for embedding.
pub(super) type SubsetMap = BTreeMap<FaceRef, crate::font::PreparedSubset>;

/// Record `ToUnicode` (first) then shaped GIDs for a fallback-shaped chunk.
pub(super) fn record_shaped_chunk(
    fonts: &FontBag,
    primary: FaceRef,
    chunk: &str,
    runs: &[(FaceRef, Vec<ShapedGlyph>)],
    glyph_sets: &mut GlyphSets,
) {
    for ch in chunk.chars() {
        let run_face = resolve_char_face(fonts, primary, ch);
        let set = glyph_sets.entry(run_face).or_default();
        let mut buf = [0u8; 4];
        let s = ch.encode_utf8(&mut buf);
        collect_glyph_set(fonts, run_face, s, set);
    }
    for (run_face, glyphs) in runs {
        let set = glyph_sets.entry(*run_face).or_default();
        note_shaped_glyphs(glyphs, set);
    }
}

/// Convert shaped fallback runs into paint spans.
pub(super) fn spans_from_shaped_runs(
    font_size: f32,
    runs: Vec<(FaceRef, Vec<ShapedGlyph>)>,
    fill: [f32; 3],
    underline: bool,
) -> Vec<LaidSpan> {
    runs.into_iter()
        .map(|(face, glyphs)| LaidSpan {
            face,
            font_size,
            glyphs,
            fill,
            underline,
        })
        .collect()
}

/// Shape with sealed script fallback, record glyphs, return spans + width.
pub(super) fn shape_and_record_spans(
    fonts: &FontBag,
    face: FaceRef,
    text: &str,
    font_size: f32,
    glyph_sets: &mut GlyphSets,
    fill: [f32; 3],
    underline: bool,
) -> Result<(Vec<LaidSpan>, f32), WeaveError> {
    let runs = shape_text_with_fallback(fonts, face, text, font_size)?;
    let width = shaped_runs_width(&runs);
    record_shaped_chunk(fonts, face, text, &runs, glyph_sets);
    Ok((
        spans_from_shaped_runs(font_size, runs, fill, underline),
        width,
    ))
}

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
    /// Fill RGB in 0.0..=1.0 (engine black when knobs omit color).
    pub fill: [f32; 3],
    /// Stroke an underline under this span (`InlineStyle.underline` or cite policy).
    pub underline: bool,
}

/// One horizontal line of text (or an empty gap).
#[derive(Debug, Clone)]
pub(super) struct LaidLine {
    pub spans: Vec<LaidSpan>,
    /// Vertical advance consumed by this line (points).
    pub leading: f32,
    /// Prefer to keep this item with the next when paginating.
    pub glue_after: bool,
    /// Left edge of the alignment box inside the content box (points).
    pub indent: f32,
    /// Width of the alignment box; text is placed within `[indent, indent+measure]`.
    pub measure: f32,
    /// In-band text alignment within [`Self::measure`].
    pub text_align: TextAlign,
}

impl LaidLine {
    /// True when this line is vertical whitespace with no glyphs.
    pub(super) fn is_gap(&self) -> bool {
        self.spans.is_empty()
    }

    /// Vertical whitespace with no glyphs.
    pub(super) fn gap(leading: f32) -> Self {
        Self {
            spans: Vec::new(),
            leading,
            glue_after: false,
            indent: 0.0,
            measure: 0.0,
            text_align: TextAlign::Left,
        }
    }

    /// Left-aligned wrapped line occupying `measure` (tables, body cells).
    pub(super) fn wrapped(spans: Vec<LaidSpan>, leading: f32, measure: f32) -> Self {
        Self {
            spans,
            leading,
            glue_after: false,
            indent: 0.0,
            measure,
            text_align: TextAlign::Left,
        }
    }

    /// Place this line in a figure-width band (indent + measure + in-band align).
    pub(super) fn apply_figure_band(&mut self, align: FigureAlign, content_w: f32, band_w: f32) {
        self.indent = align.offset_x(content_w, band_w);
        self.measure = band_w;
        self.text_align = TextAlign::from(align);
    }

    /// Shape `text` (with sealed script fallback) and record glyphs into `glyph_sets`.
    pub(super) fn shaped(
        fonts: &FontBag,
        face: FaceRef,
        text: &str,
        font_size: f32,
        leading: f32,
        glyph_sets: &mut GlyphSets,
        fill: [f32; 3],
    ) -> Result<Self, WeaveError> {
        let (spans, _) =
            shape_and_record_spans(fonts, face, text, font_size, glyph_sets, fill, false)?;
        Ok(Self {
            spans,
            leading,
            glue_after: false,
            indent: 0.0,
            measure: 0.0,
            text_align: TextAlign::Left,
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

/// Side-by-side column band (slide `two-column` layouts).
#[derive(Debug, Clone)]
pub(super) struct LaidColumns {
    /// Per-column wrapped lines (same length as `col_widths`).
    pub columns: Vec<Vec<LaidLine>>,
    pub col_widths: Vec<f32>,
    /// Gap between columns (points).
    pub gap: f32,
    pub gap_after: f32,
}

impl LaidColumns {
    /// Height of the tallest column plus trailing gap.
    pub(super) fn height(&self) -> f32 {
        let col_h = self
            .columns
            .iter()
            .map(|lines| lines.iter().map(|l| l.leading).sum::<f32>())
            .fold(0.0_f32, f32::max);
        col_h + self.gap_after
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
        /// Trailing gap after the image (from `[figure].gap_after_image`).
        gap_after: f32,
        /// Horizontal alignment within the content box.
        align: FigureAlign,
    },
    Table(LaidTable),
    Columns(LaidColumns),
    Math(LaidMath),
    /// Horizontal rule from a layout `rule` op.
    Rule {
        /// Stroke width (points).
        width: f32,
        /// Stroke thickness (points).
        thickness: f32,
        /// Vertical band reserved for the rule (stroke centered).
        leading: f32,
        /// Trailing gap after the rule band.
        gap_after: f32,
    },
}

impl LaidItem {
    /// True when this is a gap-only text line (no glyphs).
    pub(super) fn is_gap(&self) -> bool {
        matches!(self, Self::Text(line) if line.is_gap())
    }

    /// Mutable access when this item is a gap-only text line.
    pub(super) fn as_gap_mut(&mut self) -> Option<&mut LaidLine> {
        match self {
            Self::Text(line) if line.is_gap() => Some(line),
            _ => None,
        }
    }

    /// Vertical space this item occupies (including image trailing gap).
    pub(super) fn height(&self) -> f32 {
        match self {
            Self::Text(line) => line.leading,
            Self::Image {
                height, gap_after, ..
            } => *height + *gap_after,
            Self::Table(table) => table.height(),
            Self::Columns(cols) => cols.height(),
            Self::Math(math) => math.height + math.gap_after,
            Self::Rule {
                leading, gap_after, ..
            } => *leading + *gap_after,
        }
    }

    /// Whether pagination should try to keep this item with the next.
    pub(super) fn glue_after(&self) -> bool {
        match self {
            Self::Text(line) => line.glue_after,
            Self::Image { glue_after, .. } => *glue_after,
            Self::Table(_) | Self::Columns(_) | Self::Math(_) | Self::Rule { .. } => false,
        }
    }

    pub(super) fn set_glue_after(&mut self, glue: bool) {
        match self {
            Self::Text(line) => line.glue_after = glue,
            Self::Image { glue_after, .. } => *glue_after = glue,
            Self::Table(_) | Self::Columns(_) | Self::Math(_) | Self::Rule { .. } => {}
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

/// Which aesthetic color category applies to a run sequence.
pub(super) use crate::knobs::ProsePaintCategory as PaintCategory;

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
    /// Override wrap width; `None` uses content width minus indent.
    pub max_width: Option<f32>,
    pub paint: PaintCategory,
    /// Split tokens wider than the wrap measure (false = soft wrap only).
    pub hard_break_overflow: bool,
    /// In-band text alignment within the wrap measure.
    pub text_align: TextAlign,
}
