//! Box layout for parsed [`super::parse::MathExpr`] trees.

use crate::error::WeaveError;

use super::geo::try_layout_geo;
use super::parse::MathExpr;
use super::rel::{
    MathBox, RelEl, append_box, char_ink, h_center, layout_ord_raw, rule_thickness, shift_box_vert,
    upright_face,
};

pub(super) use super::rel::{MathCtx, shift_to_top_origin};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AtomKind {
    Ord,
    Bin,
    Rel,
    Open,
    Close,
    Op,
    /// Delimited matrix / similar “inner” atom (needs pad outside fences).
    Inner,
}

fn atom_kind(expr: &MathExpr) -> AtomKind {
    match expr {
        MathExpr::Ord(text) => classify_symbol(text),
        MathExpr::Scripts { base, .. } => atom_kind(base),
        MathExpr::MathRm(inner) => atom_kind(inner),
        MathExpr::Matrix {
            delimited: true, ..
        } => AtomKind::Inner,
        MathExpr::Sqrt(_) | MathExpr::Row(_) | MathExpr::Frac(_, _) | MathExpr::Matrix { .. } => {
            AtomKind::Ord
        }
    }
}

fn classify_symbol(text: &str) -> AtomKind {
    match text.trim() {
        "+" | "-" | "−" | "±" | "∓" | "×" | "·" | "∘" | "÷" | "∪" | "∩" => {
            AtomKind::Bin
        }
        // ∞ grouped with relations for spacing; drawn upright like arrows.
        "=" | "≤" | "≥" | "≠" | "≈" | "≡" | "→" | "←" | "⇒" | "⇔" | "↔" | "↦" | "∞" | "∈" | "∉"
        | "⊂" | "⊃" | "⊆" | "⊇" | "∀" | "∃" => AtomKind::Rel,
        "(" | "[" | "{" => AtomKind::Open,
        ")" | "]" | "}" => AtomKind::Close,
        t if is_big_op(t) => AtomKind::Op,
        _ => AtomKind::Ord,
    }
}

/// Large operators that take Op spacing (and display `.v1` glyphs when available).
fn is_big_op(text: &str) -> bool {
    matches!(text.trim(), "∑" | "∏" | "∫" | "∮" | "∐" | "⋃" | "⋂")
}

/// TeX `\displaylimits` family: under/over limits in display (not ∫/∮ — those stay `\nolimits`).
fn is_displaylimits_op(text: &str) -> bool {
    matches!(text.trim(), "∑" | "∏" | "∐" | "⋃" | "⋂")
}

/// TeX `\nolimits` integrals: tip-side scripts (not mid-body letter scripts).
fn is_integral_op(text: &str) -> bool {
    matches!(text.trim(), "∫" | "∮")
}

fn ord_atom(expr: &MathExpr) -> Option<&str> {
    match expr {
        MathExpr::Ord(text) => Some(text.as_str()),
        _ => None,
    }
}

/// TeX-like inter-atom space in mu (0 = tight).
fn space_mu(left: AtomKind, right: AtomKind) -> f32 {
    use AtomKind::{Bin, Close, Inner, Op, Open, Ord, Rel};
    match (left, right) {
        (Ord | Bin | Rel | Op | Close, Inner) | (Inner, Ord | Bin | Rel | Op | Close) => 3.5,
        (Ord | Close, Bin) | (Bin, Ord | Open) => 4.0,
        (Ord | Close | Op | Bin, Rel) | (Rel, Ord | Open | Op | Bin) => 5.0,
        (Ord | Close, Op) | (Op, Ord | Open) => 3.0,
        (Ord | Inner, Open) | (Close, Open | Ord) => 2.0,
        _ => 0.0,
    }
}

/// Unary minus / plus: Bin at list start or after Bin/Rel/Open/Op → Ord.
fn normalize_row_kinds(items: &[MathExpr]) -> Vec<AtomKind> {
    let mut kinds: Vec<AtomKind> = items.iter().map(atom_kind).collect();
    for i in 0..kinds.len() {
        if kinds[i] != AtomKind::Bin {
            continue;
        }
        let unary = match i.checked_sub(1).map(|j| kinds[j]) {
            None | Some(AtomKind::Bin | AtomKind::Rel | AtomKind::Open | AtomKind::Op) => true,
            Some(_) => false,
        };
        if unary {
            kinds[i] = AtomKind::Ord;
        }
    }
    kinds
}

pub(super) fn layout_expr(
    expr: &MathExpr,
    ctx: &mut MathCtx,
    font_size: f32,
) -> Result<MathBox, WeaveError> {
    match expr {
        MathExpr::Ord(text) => layout_ord(text, ctx, font_size),
        MathExpr::Row(items) => layout_row(items, ctx, font_size),
        MathExpr::Frac(num, den) => layout_frac(num, den, ctx, font_size),
        MathExpr::Scripts { base, sup, sub } => {
            layout_scripts(base, sup.as_deref(), sub.as_deref(), ctx, font_size)
        }
        MathExpr::MathRm(inner) => ctx.with_upright_face(|ctx| layout_expr(inner, ctx, font_size)),
        MathExpr::Sqrt(inner) => layout_sqrt(inner, ctx, font_size),
        MathExpr::Matrix { delimited, rows } => layout_matrix(*delimited, rows, ctx, font_size),
    }
}

fn layout_opt(
    expr: Option<&MathExpr>,
    ctx: &mut MathCtx,
    font_size: f32,
) -> Result<Option<MathBox>, WeaveError> {
    expr.map(|e| layout_expr(e, ctx, font_size)).transpose()
}

/// Layout `base` plus optional scripts at `size_factor × font_size`.
fn layout_base_with_scripts(
    base: &MathExpr,
    limsup: Option<&MathExpr>,
    liminf: Option<&MathExpr>,
    ctx: &mut MathCtx,
    font_size: f32,
    size_factor: f32,
) -> Result<(MathBox, Option<MathBox>, Option<MathBox>), WeaveError> {
    let base_box = layout_expr(base, ctx, font_size)?;
    let script_size = font_size * size_factor;
    let upper = layout_opt(limsup, ctx, script_size)?;
    let lower = layout_opt(liminf, ctx, script_size)?;
    Ok((base_box, upper, lower))
}

/// Sealed LM Math PUA → display-style `.v1` operator glyphs (TeX `\displaystyle`).
fn display_op_char(ch: char) -> Option<char> {
    match ch {
        '∫' => Some('\u{E000}'),
        '∮' => Some('\u{E001}'),
        '∑' => Some('\u{E002}'),
        '∏' => Some('\u{E003}'),
        '∐' => Some('\u{E004}'),
        '⋃' => Some('\u{E005}'),
        '⋂' => Some('\u{E006}'),
        _ => None,
    }
}

fn layout_ord(text: &str, ctx: &mut MathCtx, font_size: f32) -> Result<MathBox, WeaveError> {
    if let Some(geo) = try_layout_geo(text, ctx, font_size) {
        return geo;
    }
    let trimmed = text.trim();
    // Display: use the font's large op glyph (`.v1`), not a scaled text-style ∫/∑.
    let draw_text = if ctx.display {
        trimmed
            .chars()
            .next()
            .filter(|_| is_big_op(trimmed))
            .and_then(display_op_char)
            .map(|c| c.to_string())
            .unwrap_or_else(|| trimmed.to_string())
    } else {
        trimmed.to_string()
    };
    let op = classify_symbol(trimmed) == AtomKind::Op;
    let mut box_ = if op {
        ctx.with_upright_face(|ctx| layout_ord_raw(&draw_text, ctx, font_size))?
    } else {
        layout_ord_raw(&draw_text, ctx, font_size)?
    };
    if op {
        let ink_ch = draw_text.chars().next().unwrap_or('∑');
        if let Some(ink) = char_ink(ctx.fonts, upright_face(ctx.face), ink_ch, font_size) {
            box_.height = ink.above;
            box_.depth = ink.below;
        }
        let center = (box_.height - box_.depth) / 2.0;
        let dy = ctx.axis(font_size) - center;
        box_ = shift_box_vert(box_, dy);
    }
    Ok(box_)
}

fn layout_row(
    items: &[MathExpr],
    ctx: &mut MathCtx,
    font_size: f32,
) -> Result<MathBox, WeaveError> {
    if items.is_empty() {
        return layout_ord("", ctx, font_size);
    }
    let kinds = normalize_row_kinds(items);
    let mut x = 0.0;
    let mut height = 0.0_f32;
    let mut depth = 0.0_f32;
    let mut elements = Vec::new();
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            x += ctx.mu(space_mu(kinds[i - 1], kinds[i]), font_size);
            if kinds[i - 1] == AtomKind::Op && matches!(&items[i - 1], MathExpr::Scripts { .. }) {
                x += ctx.mu(ctx.knobs.op.after_space_mu, font_size);
            }
        }
        let box_ = layout_expr(item, ctx, font_size)?;
        height = height.max(box_.height);
        depth = depth.max(box_.depth);
        let w = box_.width;
        append_box(&mut elements, box_, x, 0.0);
        x += w;
    }
    Ok(MathBox {
        width: x,
        height,
        depth,
        elements,
    })
}

fn layout_frac(
    num: &MathExpr,
    den: &MathExpr,
    ctx: &mut MathCtx,
    font_size: f32,
) -> Result<MathBox, WeaveError> {
    let script = font_size * ctx.knobs.frac.script_size_factor;
    let num_b = layout_expr(num, ctx, script)?;
    let den_b = layout_expr(den, ctx, script)?;
    let thickness = rule_thickness(
        font_size,
        ctx.knobs.frac.rule_thickness_factor,
        ctx.knobs.frac.rule_thickness_min,
    );
    let gap_num = font_size * ctx.knobs.frac.gap_num_factor;
    let gap_den = font_size * ctx.knobs.frac.gap_den_factor;
    let axis = ctx.axis(font_size);
    let pad = font_size * ctx.knobs.frac.pad_factor;
    let width = num_b.width.max(den_b.width) + 2.0 * pad;
    let num_x = h_center(width, num_b.width);
    let den_x = h_center(width, den_b.width);
    let num_baseline = axis + thickness / 2.0 + gap_num + num_b.depth;
    let den_baseline = axis - thickness / 2.0 - gap_den - den_b.height;
    let height = num_baseline + num_b.height;
    let depth = -den_baseline + den_b.depth;
    let mut elements = Vec::new();
    append_box(&mut elements, num_b, num_x, num_baseline);
    elements.push(RelEl::Rule {
        x: pad * 0.25,
        y: axis,
        width: width - pad * 0.5,
        thickness,
    });
    append_box(&mut elements, den_b, den_x, den_baseline);
    Ok(MathBox {
        width,
        height,
        depth,
        elements,
    })
}

/// `\sqrt{…}`: geometric radical + vinculum spanning the full radicand width.
fn layout_sqrt(inner: &MathExpr, ctx: &mut MathCtx, font_size: f32) -> Result<MathBox, WeaveError> {
    let body = layout_expr(inner, ctx, font_size)?;
    let body_w = body.width;
    let gap = font_size * ctx.knobs.sqrt.gap_factor;
    let thickness = rule_thickness(
        font_size,
        ctx.knobs.sqrt.rule_thickness_factor,
        ctx.knobs.sqrt.rule_thickness_min,
    );
    let pad = font_size * ctx.knobs.sqrt.pad_factor;
    let overhang = font_size * ctx.knobs.sqrt.overhang_factor;

    let content_span = body.height + body.depth;
    let radical_h = (content_span + gap + thickness).max(font_size * 0.95);
    let radical_w = font_size * 0.55;
    let vinculum_y = body.height + gap + thickness / 2.0;
    let radical_top = vinculum_y + thickness / 2.0;
    let body_x = radical_w + pad;
    let vinculum_x = radical_w * 0.85;
    let vinculum_w = body_x + body_w - vinculum_x + overhang;

    let height = radical_top;
    let depth = body.depth.max(radical_h - radical_top).max(0.0);

    let mut elements = vec![
        RelEl::Radical {
            x: 0.0,
            y: radical_top,
            height: radical_h,
            width: radical_w,
            thickness,
        },
        RelEl::Rule {
            x: vinculum_x,
            y: vinculum_y,
            width: vinculum_w,
            thickness,
        },
    ];
    append_box(&mut elements, body, body_x, 0.0);

    Ok(MathBox {
        width: body_x + body_w + overhang,
        height,
        depth,
        elements,
    })
}

fn layout_scripts(
    base: &MathExpr,
    sup: Option<&MathExpr>,
    sub: Option<&MathExpr>,
    ctx: &mut MathCtx,
    font_size: f32,
) -> Result<MathBox, WeaveError> {
    match ord_atom(base) {
        Some(text) if ctx.display && is_displaylimits_op(text) => {
            layout_op_limits(base, sup, sub, ctx, font_size)
        }
        Some(text) if is_integral_op(text) => layout_int_nolimits(base, sup, sub, ctx, font_size),
        _ => layout_side_scripts(base, sup, sub, ctx, font_size),
    }
}

/// TeX-style under/over limits for display ∑/∏/….
fn layout_op_limits(
    base: &MathExpr,
    limsup: Option<&MathExpr>,
    liminf: Option<&MathExpr>,
    ctx: &mut MathCtx,
    font_size: f32,
) -> Result<MathBox, WeaveError> {
    let (base_box, upper, lower) = layout_base_with_scripts(
        base,
        limsup,
        liminf,
        ctx,
        font_size,
        ctx.knobs.op.limit_size_factor,
    )?;
    let base_w = base_box.width;
    let mut height = base_box.height;
    let mut depth = base_box.depth;
    let upper_w = upper.as_ref().map_or(0.0, |b| b.width);
    let lower_w = lower.as_ref().map_or(0.0, |b| b.width);
    let width = base_w.max(upper_w).max(lower_w);
    let gap_above = font_size * ctx.knobs.op.gap_above_factor;
    let gap_below = font_size * ctx.knobs.op.gap_below_factor;

    let mut elements = Vec::new();
    append_box(&mut elements, base_box, h_center(width, base_w), 0.0);

    if let Some(upper) = upper {
        let uw = upper.width;
        let y = height + gap_above + upper.depth;
        height = y + upper.height;
        append_box(&mut elements, upper, h_center(width, uw), y);
    }
    if let Some(lower) = lower {
        let lw = lower.width;
        let y = -(depth + gap_below + lower.height);
        depth = -y + lower.depth;
        append_box(&mut elements, lower, h_center(width, lw), y);
    }

    Ok(MathBox {
        width,
        height,
        depth,
        elements,
    })
}

/// Lower-limit start as a fraction of integral advance (TeX italic correction stand-in).
const INT_LOWER_X_FRAC: f32 = 0.58;
/// Extra drop below the glyph bottom, as a fraction of the lower limit's height.
const INT_LOWER_DROP_FRAC: f32 = 0.15;

/// ∫/∮ TeX `\nolimits`: upper at the top-right tip; lower near the bottom curl.
fn layout_int_nolimits(
    base: &MathExpr,
    limsup: Option<&MathExpr>,
    liminf: Option<&MathExpr>,
    ctx: &mut MathCtx,
    font_size: f32,
) -> Result<MathBox, WeaveError> {
    let (base_box, upper, lower) = layout_base_with_scripts(
        base,
        limsup,
        liminf,
        ctx,
        font_size,
        ctx.knobs.op.limit_size_factor,
    )?;
    let gap = ctx.mu(ctx.knobs.script.gap_mu, font_size);
    let upper_x = base_box.width + gap;
    let lower_x = base_box.width * INT_LOWER_X_FRAC;
    let mut width = base_box.width;
    let mut height = base_box.height;
    let mut depth = base_box.depth;
    let mut elements = base_box.elements;

    if let Some(upper) = upper {
        let y = base_box.height - upper.height * 0.5;
        height = height.max(y + upper.height);
        width = width.max(upper_x + upper.width);
        append_box(&mut elements, upper, upper_x, y);
    }
    if let Some(lower) = lower {
        let y = -base_box.depth - lower.height * INT_LOWER_DROP_FRAC;
        depth = depth.max((-y + lower.depth).max(0.0));
        width = width.max(lower_x + lower.width);
        append_box(&mut elements, lower, lower_x, y);
    }

    Ok(MathBox {
        width,
        height,
        depth,
        elements,
    })
}

fn layout_side_scripts(
    base: &MathExpr,
    sup: Option<&MathExpr>,
    sub: Option<&MathExpr>,
    ctx: &mut MathCtx,
    font_size: f32,
) -> Result<MathBox, WeaveError> {
    let (base_box, superscript, subscript) =
        layout_base_with_scripts(base, sup, sub, ctx, font_size, ctx.knobs.script.size_factor)?;
    let script_w = superscript
        .as_ref()
        .map_or(0.0, |b| b.width)
        .max(subscript.as_ref().map_or(0.0, |b| b.width));
    let script_gap = ctx.mu(ctx.knobs.script.gap_mu, font_size);
    let script_x = base_box.width + script_gap;
    let overlap = if subscript.is_some() {
        0.0
    } else {
        ctx.mu(ctx.knobs.script.overlap_mu, font_size)
    };
    let width = (script_x + script_w - overlap).max(script_x);
    let mut height = base_box.height;
    let mut depth = base_box.depth;
    let mut elements = base_box.elements;
    if let Some(superscript) = superscript {
        let y = font_size * ctx.knobs.script.superscript_raise_factor;
        height = height.max(y + superscript.height);
        append_box(&mut elements, superscript, script_x, y);
    }
    if let Some(subscript) = subscript {
        let y = -(font_size * ctx.knobs.script.subscript_lower_factor)
            .max(base_box.depth * 0.55 + font_size * 0.06);
        depth = depth.max(-y + subscript.depth);
        append_box(&mut elements, subscript, script_x, y);
    }
    Ok(MathBox {
        width,
        height,
        depth,
        elements,
    })
}

fn layout_matrix(
    delimited: bool,
    rows: &[Vec<MathExpr>],
    ctx: &mut MathCtx,
    font_size: f32,
) -> Result<MathBox, WeaveError> {
    let cols = rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
    let cell_size = font_size * ctx.knobs.matrix.cell_size;
    let mut cells: Vec<Vec<MathBox>> = Vec::with_capacity(rows.len());
    for row in rows {
        let mut laid = Vec::with_capacity(cols);
        for c in 0..cols {
            let expr = row
                .get(c)
                .cloned()
                .unwrap_or_else(|| MathExpr::Ord(String::new()));
            laid.push(layout_expr(&expr, ctx, cell_size)?);
        }
        cells.push(laid);
    }

    let mut col_w = vec![0.0_f32; cols];
    let mut row_h = vec![0.0_f32; rows.len()];
    let mut row_d = vec![0.0_f32; rows.len()];
    for (r, row) in cells.iter().enumerate() {
        for (c, cell) in row.iter().enumerate() {
            col_w[c] = col_w[c].max(cell.width);
            row_h[r] = row_h[r].max(cell.height);
            row_d[r] = row_d[r].max(cell.depth);
        }
    }

    let col_gap = font_size * ctx.knobs.matrix.col_gap;
    let row_gap = font_size * ctx.knobs.matrix.row_gap;
    let pad = font_size * ctx.knobs.matrix.pad;
    let inner_w = col_w.iter().sum::<f32>() + col_gap * cols.saturating_sub(1) as f32;
    let row_totals: Vec<f32> = row_h.iter().zip(&row_d).map(|(h, d)| h + d).collect();
    let inner_h = row_totals.iter().sum::<f32>() + row_gap * rows.len().saturating_sub(1) as f32;
    let axis = ctx.axis(font_size);
    let half = inner_h / 2.0;
    let body_top = axis + half;
    let body_bot = axis - half;
    let height = body_top + pad * 0.5;
    let depth = -body_bot + pad * 0.5;

    let mut elements = Vec::new();
    let (delim_w, width) = if delimited {
        let half_h = half + pad * 0.15;
        let paren_w = ctx.knobs.paren.delim_width(half_h);
        let thick = ctx.knobs.paren.stroke_thickness(font_size);
        let width = inner_w + 2.0 * pad + 2.0 * paren_w;
        elements.push(RelEl::Paren {
            x: 0.0,
            axis,
            half_h,
            width: paren_w,
            thickness: thick,
            left: true,
        });
        elements.push(RelEl::Paren {
            x: width - paren_w,
            axis,
            half_h,
            width: paren_w,
            thickness: thick,
            left: false,
        });
        (paren_w, width)
    } else {
        (0.0, inner_w + 2.0 * pad)
    };

    let origin_x = delim_w + pad;
    let mut y_top = body_top;
    for (r, row) in cells.into_iter().enumerate() {
        let baseline = y_top - row_h[r];
        let mut x = origin_x;
        for (c, cell) in row.into_iter().enumerate() {
            let cell_x = x + h_center(col_w[c], cell.width);
            let w = col_w[c];
            append_box(&mut elements, cell, cell_x, baseline);
            x += w + col_gap;
        }
        y_top -= row_totals[r] + row_gap;
    }
    Ok(MathBox {
        width,
        height,
        depth,
        elements,
    })
}
