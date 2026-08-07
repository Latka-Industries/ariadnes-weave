//! Figure image + optional title + caption layout.

use crate::error::WeaveError;
use crate::font::{FaceId, FaceRef, FontBag};
use crate::image_prep::{PreparedImage, prepare_image};
use crate::ir::{FigurePlacement, PrintImage, TextRun};
use crate::knobs::{FigureAlign, LayoutKnobs};
use crate::profile::ProfileMetrics;

use super::super::types::{GlyphSets, LaidItem, LaidLine, LayoutSegment};
use super::LayoutCtx;
use super::runs::{
    caption_layout, figure_title_layout, layout_ctx, push_styled_runs, with_knob_italic,
};

/// Inputs for laying out a [`crate::ir::PrintBlock::Figure`].
pub(super) struct PushFigureArgs<'a> {
    pub segments: &'a mut [LayoutSegment],
    pub images: &'a mut Vec<PreparedImage>,
    pub image: &'a PrintImage,
    pub alt: &'a str,
    pub title: &'a [TextRun],
    pub caption: &'a [TextRun],
    pub placement: FigurePlacement,
    pub metrics: &'a ProfileMetrics,
    pub fonts: &'a FontBag,
    pub knobs: &'a LayoutKnobs,
    pub glyph_sets: &'a mut GlyphSets,
}

enum FigureVisual {
    Image {
        img_idx: usize,
        width: f32,
        height: f32,
        glue_after: bool,
        align: FigureAlign,
    },
    Alt {
        line: LaidLine,
        band_w: f32,
    },
}

impl FigureVisual {
    fn band_w(&self) -> f32 {
        match self {
            Self::Image { width, .. } => *width,
            Self::Alt { band_w, .. } => *band_w,
        }
    }
}

impl PushFigureArgs<'_> {
    /// Decode/fit the image (or alt placeholder), then append title + caption.
    ///
    /// [`FigurePlacement::FloatNear`] glues the figure to the preceding content
    /// and keeps title / image / caption together when possible.
    pub(super) fn run(self) -> Result<(), WeaveError> {
        let Self {
            segments,
            images,
            image,
            alt,
            title,
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

        let content_w = metrics.content_width();
        let align = knobs.prose.figure.align;
        let max_w = knobs.prose.figure.max_display_width(content_w);
        let glue_image_after = !caption.is_empty() || float_near;

        let visual = if let Ok(prepared) = prepare_image(image) {
            let (width, height) = prepared.fit_width(max_w);
            let img_idx = images.len();
            images.push(prepared);
            FigureVisual::Image {
                img_idx,
                width,
                height,
                glue_after: glue_image_after,
                align,
            }
        } else {
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
            line.apply_figure_band(align, content_w, band_w);
            line.glue_after = glue_image_after;
            FigureVisual::Alt { line, band_w }
        };

        let band_w = visual.band_w();
        let empty_gap = match &visual {
            FigureVisual::Image { .. } => knobs.prose.figure.gap_after,
            FigureVisual::Alt { .. } => knobs.prose.figure.alt_gap_after,
        };
        let gap_after_image = knobs.prose.figure.gap_after_image;

        let mut ctx = layout_ctx(metrics, fonts, knobs, glyph_sets);
        push_figure_title(&mut seg.1, title, band_w, &mut ctx)?;
        push_visual(&mut seg.1, visual, gap_after_image);
        finish_figure_caption(&mut seg.1, caption, empty_gap, band_w, &mut ctx)
    }
}

fn push_figure_title(
    out: &mut Vec<LaidItem>,
    title: &[TextRun],
    band_w: f32,
    ctx: &mut LayoutCtx<'_>,
) -> Result<(), WeaveError> {
    if title.is_empty() {
        return Ok(());
    }
    push_styled_runs(
        out,
        title,
        ctx,
        figure_title_layout(ctx.metrics, ctx.knobs, band_w),
    )
}

fn finish_figure_caption(
    out: &mut Vec<LaidItem>,
    caption: &[TextRun],
    empty_gap: f32,
    band_w: f32,
    ctx: &mut LayoutCtx<'_>,
) -> Result<(), WeaveError> {
    if caption.is_empty() {
        out.push(LaidItem::Text(LaidLine::gap(empty_gap)));
        return Ok(());
    }
    let runs = with_knob_italic(caption, ctx.knobs.prose.caption.italic);
    push_styled_runs(
        out,
        &runs,
        ctx,
        caption_layout(ctx.metrics, ctx.knobs, band_w),
    )
}

fn push_visual(out: &mut Vec<LaidItem>, visual: FigureVisual, gap_after_image: f32) {
    match visual {
        FigureVisual::Image {
            img_idx,
            width,
            height,
            glue_after,
            align,
        } => {
            out.push(LaidItem::Image {
                img_idx,
                width,
                height,
                glue_after,
                gap_after: gap_after_image,
                align,
            });
        }
        FigureVisual::Alt { line, .. } => {
            out.push(LaidItem::Text(line));
        }
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
