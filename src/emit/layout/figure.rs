//! Figure image + caption layout.

use crate::error::WeaveError;
use crate::font::{FaceId, FaceRef, FontBag};
use crate::image_prep::{PreparedImage, prepare_image};
use crate::ir::{FigurePlacement, PrintImage, TextRun};
use crate::knobs::LayoutKnobs;
use crate::profile::ProfileMetrics;

use super::super::types::{GlyphSets, LaidItem, LaidLine, LayoutSegment};
use super::runs::{caption_layout, layout_ctx, push_styled_runs, with_knob_italic};

/// Inputs for laying out a [`crate::ir::PrintBlock::Figure`].
pub(super) struct PushFigureArgs<'a> {
    pub segments: &'a mut [LayoutSegment],
    pub images: &'a mut Vec<PreparedImage>,
    pub image: &'a PrintImage,
    pub alt: &'a str,
    pub caption: &'a [TextRun],
    pub placement: FigurePlacement,
    pub metrics: &'a ProfileMetrics,
    pub fonts: &'a FontBag,
    pub knobs: &'a LayoutKnobs,
    pub glyph_sets: &'a mut GlyphSets,
}

impl PushFigureArgs<'_> {
    /// Decode/fit the image (or alt placeholder), then append caption lines.
    ///
    /// [`FigurePlacement::FloatNear`] glues the figure to the preceding content
    /// and to its caption so pagination prefers to keep them together.
    pub(super) fn run(self) -> Result<(), WeaveError> {
        let Self {
            segments,
            images,
            image,
            alt,
            caption,
            placement,
            metrics,
            fonts,
            knobs,
            glyph_sets,
        } = self;
        let float_near = matches!(placement, FigurePlacement::FloatNear);
        let seg = segments.last_mut().expect("segment");
        if float_near {
            // Keep the figure with the preceding block when possible.
            if let Some(prev) = seg.1.iter_mut().rev().find(|item| !item.is_gap()) {
                prev.set_glue_after(true);
            }
        }

        apply_figure_gap_before(&mut seg.1, knobs.prose.figure.gap_before);

        let glue_after = !caption.is_empty() || float_near;
        let content_w = metrics.content_width();
        let align = knobs.prose.figure.align;
        let max_w = knobs.prose.figure.max_display_width(content_w);

        let Ok(prepared) = prepare_image(image) else {
            let label = if alt.is_empty() {
                "[figure]".into()
            } else {
                format!("[figure: {alt}]")
            };
            let mut line = LaidLine::shaped(
                fonts,
                FaceRef::Bundled(FaceId::SansItalic),
                &label,
                metrics.body_size,
                metrics.body_leading,
                glyph_sets,
                knobs.prose.text_fill_rgb01(),
            )?;
            let band_w = line.width().min(max_w);
            line.indent = align.offset_x(content_w, band_w);
            line.glue_after = glue_after;
            seg.1.push(LaidItem::Text(line));
            let empty_gap = knobs.prose.figure.alt_gap_after;
            return FigureLayoutCtx {
                metrics,
                fonts,
                knobs,
                glyph_sets,
            }
            .finish_caption(&mut seg.1, caption, empty_gap, band_w);
        };

        let (w, h) = prepared.fit_width(max_w);
        let img_idx = images.len();
        images.push(prepared);
        let empty_gap = knobs.prose.figure.gap_after;
        let gap_after_image = knobs.prose.figure.gap_after_image;

        seg.1.push(LaidItem::Image {
            img_idx,
            width: w,
            height: h,
            glue_after,
            gap_after: gap_after_image,
            align,
        });
        FigureLayoutCtx {
            metrics,
            fonts,
            knobs,
            glyph_sets,
        }
        .finish_caption(&mut seg.1, caption, empty_gap, w)
    }
}

/// Replace the prior block's trailing gap with `[figure].gap_before`.
///
/// When the segment already has content but no trailing gap line, insert one.
/// First content on a segment keeps page-margin spacing (no inserted gap).
fn apply_figure_gap_before(items: &mut Vec<LaidItem>, gap_before: f32) {
    if let Some(line) = items.last_mut().and_then(LaidItem::as_gap_mut) {
        line.leading = gap_before;
    } else if !items.is_empty() {
        items.push(LaidItem::Text(LaidLine::gap(gap_before)));
    }
}

struct FigureLayoutCtx<'a> {
    metrics: &'a ProfileMetrics,
    fonts: &'a FontBag,
    knobs: &'a LayoutKnobs,
    glyph_sets: &'a mut GlyphSets,
}

impl FigureLayoutCtx<'_> {
    fn finish_caption(
        &mut self,
        out: &mut Vec<LaidItem>,
        caption: &[TextRun],
        empty_gap: f32,
        band_w: f32,
    ) -> Result<(), WeaveError> {
        if caption.is_empty() {
            out.push(LaidItem::Text(LaidLine::gap(empty_gap)));
            return Ok(());
        }
        let runs = with_knob_italic(caption, self.knobs.prose.caption.italic);
        push_styled_runs(
            out,
            &runs,
            &mut layout_ctx(self.metrics, self.fonts, self.knobs, self.glyph_sets),
            caption_layout(self.metrics, self.knobs, band_w),
        )
    }
}
