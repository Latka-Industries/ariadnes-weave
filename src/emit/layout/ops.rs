//! D24 layout chunk ops (`place` / `vspace` / `rule`).

use crate::error::WeaveError;
use crate::ir::{LayoutOp, MeasureFrac, PlaceSkip, RuleWidth, TextRun, VspaceAmount};
use crate::knobs::FigureAlign;

use super::super::types::{FaceMode, LaidItem, LaidLine, LayoutSegment, PaintCategory, RunLayout};
use super::LayoutCtx;
use super::runs::{measure_runs_natural_width, push_styled_runs};

/// Paint D24 layout ops (`place` / `vspace` / `rule`) into the current segment.
pub(super) fn layout_layout_ops(
    ops: &[LayoutOp],
    ctx: &mut LayoutCtx,
    segments: &mut [LayoutSegment],
) -> Result<(), WeaveError> {
    let seg = segments.last_mut().expect("segment");
    for op in ops {
        match op {
            LayoutOp::Place { skip, runs } => push_place(seg, *skip, runs, ctx)?,
            LayoutOp::Vspace { amount } => {
                let leading = vspace_points(*amount, ctx.metrics.body_size);
                seg.1.push(LaidItem::Text(LaidLine::gap(leading)));
            }
            LayoutOp::Rule { width } => push_rule(seg, *width, ctx)?,
        }
    }
    // Default gap between layout chunk and neighbors = normal paragraph gap.
    if !ops.is_empty() {
        let gap = ctx.knobs.prose.paragraph.gap_after;
        if gap > 0.0 {
            seg.1.push(LaidItem::Text(LaidLine::gap(gap)));
        }
    }
    Ok(())
}

fn validate_frac(frac: MeasureFrac) -> Result<(), WeaveError> {
    if frac.bps > MeasureFrac::FULL.bps {
        Err(WeaveError::InvalidLayoutFrac(frac.bps))
    } else {
        Ok(())
    }
}

fn vspace_points(amount: VspaceAmount, body_size: f32) -> f32 {
    match amount {
        VspaceAmount::Small => body_size * 0.5,
        VspaceAmount::Med => body_size,
        VspaceAmount::Big => body_size * 2.0,
        VspaceAmount::Em { em } => em.to_points(body_size),
    }
}

fn rule_width_points(width: RuleWidth, measure: f32, body_size: f32) -> Result<f32, WeaveError> {
    if width.frac.is_none() && width.em.is_none() {
        return Err(WeaveError::EmptyRuleWidth);
    }
    let mut pts = 0.0_f32;
    if let Some(frac) = width.frac {
        validate_frac(frac)?;
        pts += measure * frac.as_f32();
    }
    if let Some(em) = width.em {
        pts += em.to_points(body_size);
    }
    Ok(pts.max(0.0))
}

fn push_rule(
    seg: &mut LayoutSegment,
    width: RuleWidth,
    ctx: &mut LayoutCtx,
) -> Result<(), WeaveError> {
    let measure = ctx.metrics.content_width();
    let w = rule_width_points(width, measure, ctx.metrics.body_size)?;
    let thickness = (ctx.metrics.body_size * ctx.knobs.math.frac.rule_thickness_factor)
        .max(ctx.knobs.math.frac.rule_thickness_min);
    let leading = (ctx.metrics.body_leading * 0.5).max(thickness + 2.0);
    seg.1.push(LaidItem::Rule {
        width: w,
        thickness,
        leading,
        gap_after: 0.0,
    });
    Ok(())
}

fn push_place(
    seg: &mut LayoutSegment,
    skip: PlaceSkip,
    runs: &[TextRun],
    ctx: &mut LayoutCtx,
) -> Result<(), WeaveError> {
    let measure = ctx.metrics.content_width();
    let body = ctx.metrics.body_size;
    let indent = match skip {
        PlaceSkip::Em { em } => em.to_points(body).max(0.0),
        PlaceSkip::Frac { frac } => {
            validate_frac(frac)?;
            if frac.bps == MeasureFrac::FULL.bps {
                // Flush to end edge: leftover after measuring content.
                let content_w = measure_runs_natural_width(runs, ctx, body)?;
                (measure - content_w).max(0.0)
            } else {
                measure * frac.as_f32()
            }
        }
    };
    let max_width = (measure - indent).max(ctx.knobs.prose.wrap.min_width);
    push_styled_runs(
        &mut seg.1,
        runs,
        ctx,
        RunLayout {
            font_size: body,
            leading: ctx.metrics.body_leading,
            gap_after: 0.0,
            glue_last_content: false,
            mode: FaceMode::Body,
            indent,
            max_width: Some(max_width),
            paint: PaintCategory::Text,
            hard_break_overflow: true,
            text_align: FigureAlign::Left,
        },
    )?;
    Ok(())
}
