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
            if let Some(prev) = seg
                .1
                .iter_mut()
                .rev()
                .find(|item| !matches!(item, LaidItem::Text(line) if line.spans.is_empty()))
            {
                prev.set_glue_after(true);
            }
        }

        let glue_after = !caption.is_empty() || float_near;
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
            line.glue_after = glue_after;
            seg.1.push(LaidItem::Text(line));
            return finish_caption_or_gap(
                &mut seg.1,
                caption,
                knobs.prose.figure.alt_gap_after,
                metrics,
                fonts,
                knobs,
                glyph_sets,
            );
        };

        let (w, h) = prepared.fit_width(metrics.content_width());
        let img_idx = images.len();
        images.push(prepared);

        seg.1.push(LaidItem::Image {
            img_idx,
            width: w,
            height: h,
            glue_after,
        });
        finish_caption_or_gap(
            &mut seg.1,
            caption,
            knobs.prose.figure.gap_after,
            metrics,
            fonts,
            knobs,
            glyph_sets,
        )
    }
}

fn finish_caption_or_gap(
    out: &mut Vec<LaidItem>,
    caption: &[TextRun],
    empty_gap: f32,
    metrics: &ProfileMetrics,
    fonts: &FontBag,
    knobs: &LayoutKnobs,
    glyph_sets: &mut GlyphSets,
) -> Result<(), WeaveError> {
    if caption.is_empty() {
        out.push(LaidItem::Text(LaidLine::gap(empty_gap)));
        return Ok(());
    }
    let runs = with_knob_italic(caption, knobs.prose.caption.italic);
    push_styled_runs(
        out,
        &runs,
        &mut layout_ctx(metrics, fonts, knobs, glyph_sets),
        caption_layout(metrics, knobs),
    )
}
