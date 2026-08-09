//! Structured math layout for a small LaTeX subset
//! (fractions, scripts, display op limits, matrices).
//!
//! Leaf tokens still map through [`prettify::prettify_tokens`]. This is not a TeX engine.

mod layout;
mod parse;
mod prettify;

use crate::error::WeaveError;
use crate::font::{FaceId, FaceRef, FontBag};
use crate::knobs::MathKnobs;
use crate::profile::ProfileMetrics;

use super::types::{GlyphSets, LaidItem, LaidLine, LaidMath, LayoutSegment};

use layout::{MathCtx, layout_expr, shift_to_top_origin};
use parse::{MathExpr, parse_math};
pub(in crate::emit) use prettify::prettify_latex_math;
use prettify::strip_math_delimiters;

/// Lay out display/inline math as a structured box.
pub(super) fn layout_math(
    display: bool,
    latex: &str,
    metrics: &ProfileMetrics,
    fonts: &FontBag,
    knobs: &MathKnobs,
    segments: &mut [LayoutSegment],
    glyph_sets: &mut GlyphSets,
) -> Result<(), WeaveError> {
    let face = FaceRef::Bundled(if metrics.serif_body {
        FaceId::SerifItalic
    } else {
        FaceId::SansItalic
    });
    let font_size = if display {
        metrics.body_size * knobs.display.size_factor
    } else {
        metrics.body_size
    };
    let seg = segments.last_mut().expect("segment");
    if display {
        seg.1
            .push(LaidItem::Text(LaidLine::gap(knobs.display.pre_gap)));
    }

    let body = strip_math_delimiters(latex);
    let expr = parse_math(&body).unwrap_or_else(|_| MathExpr::Ord(prettify_latex_math(latex)));
    let mut ctx = MathCtx {
        fonts,
        face,
        knobs,
        glyph_sets,
        display,
    };
    let math = layout_expr(&expr, &mut ctx, font_size)?;
    seg.1.push(LaidItem::Math(LaidMath {
        width: math.width,
        height: math.height + math.depth,
        center: display,
        gap_after: if display {
            knobs.display.gap_after
        } else {
            knobs.display.inline_gap_after
        },
        elements: shift_to_top_origin(math),
    }));
    Ok(())
}
