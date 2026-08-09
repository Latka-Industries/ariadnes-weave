//! Geometric stand-ins for Liberation-missing math glyphs (∫, sets, logic, …).

use super::super::types::MathSymKind;
use super::rel::{
    MathBox, MathCtx, RelEl, box_from_midline, clamped_stroke, layout_ord_raw, pad_box_h,
    shift_box_vert, upright_face,
};
use crate::error::WeaveError;

/// Try a geometric draw for `text`; `None` means fall back to font shaping.
pub(super) fn try_layout_geo(
    text: &str,
    ctx: &mut MathCtx,
    font_size: f32,
) -> Option<Result<MathBox, WeaveError>> {
    Some(Ok(match text.trim() {
        "←" => layout_geo_arrow(ctx, font_size, true),
        "→" | "⇒" | "↦" => layout_geo_arrow(ctx, font_size, false),
        "∞" => return Some(layout_infinity(ctx, font_size)),
        "∫" => layout_geo_integral(ctx, font_size, false),
        "∮" => layout_geo_integral(ctx, font_size, true),
        "∈" => layout_geo_sym(ctx, font_size, MathSymKind::In, false),
        "∉" => layout_geo_sym(ctx, font_size, MathSymKind::NotIn, false),
        "⊂" => layout_geo_sym(ctx, font_size, MathSymKind::Subset, false),
        "⊃" => layout_geo_sym(ctx, font_size, MathSymKind::Superset, false),
        "⊆" => layout_geo_sym(ctx, font_size, MathSymKind::SubsetEq, false),
        "⊇" => layout_geo_sym(ctx, font_size, MathSymKind::SupersetEq, false),
        "∪" => layout_geo_sym(ctx, font_size, MathSymKind::Cup, false),
        "⋃" => layout_geo_sym(ctx, font_size, MathSymKind::BigCup, true),
        "⋂" => layout_geo_sym(ctx, font_size, MathSymKind::BigCap, true),
        "∐" => layout_geo_sym(ctx, font_size, MathSymKind::Coprod, true),
        "∀" => layout_geo_sym(ctx, font_size, MathSymKind::Forall, false),
        "∃" => layout_geo_sym(ctx, font_size, MathSymKind::Exists, false),
        "∅" => layout_geo_sym(ctx, font_size, MathSymKind::Empty, false),
        "∘" => layout_geo_sym(ctx, font_size, MathSymKind::Circ, false),
        "∓" => layout_geo_sym(ctx, font_size, MathSymKind::Mp, false),
        "∇" => layout_geo_sym(ctx, font_size, MathSymKind::Nabla, false),
        _ => return None,
    }))
}

/// Compact geometric integral centered on the math axis (displaylimits-friendly).
fn layout_geo_integral(ctx: &MathCtx, font_size: f32, contour: bool) -> MathBox {
    let draw_size = font_size * ctx.knobs.op.size_factor;
    // Match ∑ optical height so under/over limits sit in the same band.
    let half_h = draw_size * 0.48;
    let width = draw_size * 0.55;
    let thickness = clamped_stroke(font_size, 0.06, 0.7, 1.45);
    let axis = ctx.axis(font_size);
    box_from_midline(
        axis,
        half_h,
        width,
        vec![RelEl::Integral {
            x: 0.0,
            axis,
            half_h,
            width,
            thickness,
            contour,
        }],
    )
}

/// Geometric stand-in for a Liberation-missing math symbol.
fn layout_geo_sym(ctx: &MathCtx, font_size: f32, kind: MathSymKind, big: bool) -> MathBox {
    let ink = ctx.letter_ink(font_size);
    let (height, width, thickness) = if big {
        let draw = font_size * ctx.knobs.op.size_factor;
        (
            draw * 0.95,
            draw * 0.72,
            clamped_stroke(font_size, 0.055, 0.7, 1.4),
        )
    } else {
        match kind {
            MathSymKind::Circ => (
                ink.span().max(font_size * 0.28),
                font_size * 0.38,
                clamped_stroke(font_size, 0.05, 0.55, 1.1),
            ),
            MathSymKind::Mp => (
                ink.span().max(font_size * 0.7),
                font_size * 0.55,
                clamped_stroke(font_size, 0.055, 0.6, 1.25),
            ),
            _ => (
                ink.span().max(font_size * 0.7),
                font_size * 0.62,
                clamped_stroke(font_size, 0.055, 0.6, 1.25),
            ),
        }
    };
    let y = if big {
        ctx.axis(font_size)
    } else {
        ink.center()
    };
    box_from_midline(
        y,
        height / 2.0,
        width,
        vec![RelEl::Sym {
            kind,
            x: 0.0,
            y,
            width,
            height,
            thickness,
        }],
    )
}

/// Stroked arrow sized/centered to match surrounding letter ink (not the tiny → glyph).
fn layout_geo_arrow(ctx: &MathCtx, font_size: f32, left: bool) -> MathBox {
    let ink = ctx.letter_ink(font_size);
    let height = ink
        .span()
        .max(font_size * ctx.knobs.arrow.min_height_factor);
    let width = font_size * ctx.knobs.arrow.width_factor;
    let thickness = ctx.knobs.arrow.stroke_thickness(font_size);
    let y = ink.center();
    box_from_midline(
        y,
        height / 2.0,
        width,
        vec![RelEl::Arrow {
            x: 0.0,
            y,
            width,
            height,
            thickness,
            left,
        }],
    )
}

/// Upright ∞: ink-matched to letters, then optically enlarged / lowered / padded.
fn layout_infinity(ctx: &mut MathCtx, font_size: f32) -> Result<MathBox, WeaveError> {
    let face_u = upright_face(ctx.face);
    let reference = ctx.letter_ink(font_size);
    let probe =
        super::rel::char_ink(ctx.fonts, face_u, '∞', font_size).unwrap_or(super::rel::InkBox {
            above: font_size * ctx.knobs.infinity.ref_above_factor,
            below: font_size * ctx.knobs.infinity.ref_below_factor,
        });
    // ∞ reads optically small/light vs Greek; bump past geometric ink match.
    let scale = if probe.span() > 0.01 {
        ctx.knobs
            .infinity
            .clamp_scale(reference.span() / probe.span() * ctx.knobs.infinity.scale_boost)
    } else {
        1.65
    };
    let draw_size = font_size * scale;
    let mut box_ = ctx.with_upright_face(|ctx| layout_ord_raw("∞", ctx, draw_size))?;
    if let Some(ink) = super::rel::char_ink(ctx.fonts, face_u, '∞', draw_size) {
        // Center-match, then nudge down — glyph centers sit optically high.
        let dy = reference.center() - ink.center() - font_size * ctx.knobs.infinity.lower_factor;
        box_ = shift_box_vert(box_, dy);
        box_.height = box_.height.max(ink.above + dy);
        box_.depth = box_.depth.max((ink.below - dy).max(0.0));
    }
    let pad = ctx.mu(ctx.knobs.infinity.pad_mu, font_size);
    Ok(pad_box_h(box_, pad, pad))
}
