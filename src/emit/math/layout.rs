//! Box layout for parsed [`super::parse::MathExpr`] trees.

use crate::error::WeaveError;
use crate::font::{
    FaceId, FaceRef, FontBag, ShapedGlyph, collect_glyph_set, note_shaped_glyphs, shape_text,
    shaped_width,
};
use crate::knobs::MathKnobs;
use ttf_parser::Face as TtfFace;

use super::parse::MathExpr;
use super::super::types::{GlyphSets, LaidMathEl};

pub(super) struct MathCtx<'a> {
    pub(super) fonts: &'a FontBag,
    pub(super) face: FaceRef,
    pub(super) knobs: &'a MathKnobs,
    pub(super) glyph_sets: &'a mut GlyphSets,
}

/// Math axis above the baseline (TeX-ish); fraction bars and big ops share this.
fn math_axis(font_size: f32, knobs: &MathKnobs) -> f32 {
    font_size * knobs.metrics.axis_factor
}

/// Convert math units (18 mu = 1 em ≈ `font_size`) to PDF points.
fn mu(font_size: f32, n: f32, knobs: &MathKnobs) -> f32 {
    font_size * n / knobs.metrics.mu_per_em
}

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
        MathExpr::Matrix {
            delimited: true, ..
        } => AtomKind::Inner,
        MathExpr::Row(_) | MathExpr::Frac(_, _) | MathExpr::Matrix { .. } => AtomKind::Ord,
    }
}

fn classify_symbol(text: &str) -> AtomKind {
    match text.trim() {
        "+" | "-" | "−" | "±" | "×" | "·" => AtomKind::Bin,
        // ∞ grouped with relations for spacing; drawn upright like arrows.
        "=" | "≤" | "≥" | "≠" | "≈" | "→" | "←" | "⇒" | "∞" => AtomKind::Rel,
        "(" | "[" | "{" => AtomKind::Open,
        ")" | "]" | "}" => AtomKind::Close,
        "∑" | "∏" | "∫" => AtomKind::Op,
        _ => AtomKind::Ord,
    }
}

/// TeX-like inter-atom space in mu (0 = tight).
fn space_mu(left: AtomKind, right: AtomKind) -> f32 {
    use AtomKind::{Bin, Close, Inner, Op, Open, Ord, Rel};
    match (left, right) {
        // Pad around delimited matrices (covers most Inner-* / *-Inner pairs).
        (Ord | Bin | Rel | Op | Close, Inner) | (Inner, Ord | Bin | Rel | Op | Close) => 3.5,
        (Ord | Close, Bin) | (Bin, Ord | Open) => 4.0,
        (Ord | Close | Op | Bin, Rel) | (Rel, Ord | Open | Op | Bin) => 5.0,
        (Ord | Close, Op) | (Op, Ord | Open) => 3.0,
        // Subtle pad outside `(…)`; Inner→Open is the remaining Inner case.
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

#[derive(Debug, Clone)]
pub(super) struct MathBox {
    pub(super) width: f32,
    /// Distance above the baseline.
    pub(super) height: f32,
    /// Distance below the baseline.
    pub(super) depth: f32,
    /// Positions relative to the box baseline (`y` positive = up).
    pub(super) elements: Vec<RelEl>,
}

#[derive(Debug, Clone)]
pub(super) enum RelEl {
    Text {
        x: f32,
        y: f32,
        face: FaceRef,
        font_size: f32,
        glyphs: Vec<ShapedGlyph>,
    },
    Rule {
        x: f32,
        y: f32,
        width: f32,
        thickness: f32,
    },
    /// Stroked paren; `axis` is the math-axis offset from the baseline.
    Paren {
        x: f32,
        axis: f32,
        half_h: f32,
        width: f32,
        thickness: f32,
        left: bool,
    },
    /// Geometric arrow; `y` is the shaft midline relative to the baseline.
    Arrow {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        thickness: f32,
        left: bool,
    },
}

impl RelEl {
    fn offset(self, dx: f32, dy: f32) -> Self {
        match self {
            Self::Text {
                x,
                y,
                face,
                font_size,
                glyphs,
            } => Self::Text {
                x: x + dx,
                y: y + dy,
                face,
                font_size,
                glyphs,
            },
            Self::Rule {
                x,
                y,
                width,
                thickness,
            } => Self::Rule {
                x: x + dx,
                y: y + dy,
                width,
                thickness,
            },
            Self::Paren {
                x,
                axis,
                half_h,
                width,
                thickness,
                left,
            } => Self::Paren {
                x: x + dx,
                axis: axis + dy,
                half_h,
                width,
                thickness,
                left,
            },
            Self::Arrow {
                x,
                y,
                width,
                height,
                thickness,
                left,
            } => Self::Arrow {
                x: x + dx,
                y: y + dy,
                width,
                height,
                thickness,
                left,
            },
        }
    }

    fn into_laid(self, top: f32) -> LaidMathEl {
        /// Baseline-relative `y` → distance from the box top (PDF paint space).
        fn from_top_y(top: f32, y: f32) -> f32 {
            top - y
        }
        match self {
            Self::Text {
                x,
                y,
                face,
                font_size,
                glyphs,
            } => LaidMathEl::Text {
                x,
                y: from_top_y(top, y),
                face,
                font_size,
                glyphs,
            },
            Self::Rule {
                x,
                y,
                width,
                thickness,
            } => LaidMathEl::Rule {
                x,
                y: from_top_y(top, y),
                width,
                thickness,
            },
            Self::Paren {
                x,
                axis,
                half_h,
                width,
                thickness,
                left,
            } => LaidMathEl::Paren {
                x,
                axis_y: from_top_y(top, axis),
                half_h,
                width,
                thickness,
                left,
            },
            Self::Arrow {
                x,
                y,
                width,
                height,
                thickness,
                left,
            } => LaidMathEl::Arrow {
                x,
                y: from_top_y(top, y),
                width,
                height,
                thickness,
                left,
            },
        }
    }
}

fn offset_elements(elements: Vec<RelEl>, dx: f32, dy: f32) -> Vec<RelEl> {
    elements.into_iter().map(|el| el.offset(dx, dy)).collect()
}

/// Horizontal centering offset for content inside a container.
fn h_center(container: f32, content: f32) -> f32 {
    (container - content) / 2.0
}

fn paren_element(x: f32, axis: f32, half_h: f32, width: f32, thickness: f32, left: bool) -> RelEl {
    RelEl::Paren {
        x,
        axis,
        half_h,
        width,
        thickness,
        left,
    }
}

fn append_box(dst: &mut Vec<RelEl>, src: MathBox, dx: f32, dy: f32) {
    dst.extend(offset_elements(src.elements, dx, dy));
}

pub(super) fn shift_to_top_origin(math: MathBox) -> Vec<LaidMathEl> {
    let top = math.height;
    math.elements
        .into_iter()
        .map(|el| el.into_laid(top))
        .collect()
}

pub(super) fn layout_expr(expr: &MathExpr, ctx: &mut MathCtx, font_size: f32) -> Result<MathBox, WeaveError> {
    match expr {
        MathExpr::Ord(text) => layout_ord(text, ctx, font_size),
        MathExpr::Row(items) => layout_row(items, ctx, font_size),
        MathExpr::Frac(num, den) => layout_frac(num, den, ctx, font_size),
        MathExpr::Scripts { base, sup, sub } => {
            layout_scripts(base, sup.as_deref(), sub.as_deref(), ctx, font_size)
        }
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

fn symbol_scale(text: &str) -> f32 {
    match text.trim() {
        "∑" | "∏" | "∫" => 1.35,
        _ => 1.0,
    }
}

fn upright_face(face: FaceRef) -> FaceRef {
    match face {
        FaceRef::Bundled(
            FaceId::SerifRegular
            | FaceId::SerifBold
            | FaceId::SerifItalic
            | FaceId::SerifBoldItalic,
        ) => FaceRef::Bundled(FaceId::SerifRegular),
        FaceRef::Bundled(_) => FaceRef::Bundled(FaceId::SansRegular),
        other @ FaceRef::Pinned(_) => other,
    }
}

#[derive(Clone, Copy)]
struct InkBox {
    above: f32,
    below: f32,
}

impl InkBox {
    fn span(self) -> f32 {
        self.above + self.below
    }

    fn center(self) -> f32 {
        (self.above - self.below) / 2.0
    }
}

fn char_ink(fonts: &FontBag, face: FaceRef, ch: char, font_size: f32) -> Option<InkBox> {
    let data = fonts.ttf_bytes(face).ok()?;
    let ttf = TtfFace::parse(data, 0).ok()?;
    let gid = ttf.glyph_index(ch)?;
    let bb = ttf.glyph_bounding_box(gid)?;
    let scale = font_size / f32::from(ttf.units_per_em());
    Some(InkBox {
        above: f32::from(bb.y_max) * scale,
        below: (-f32::from(bb.y_min) * scale).max(0.0),
    })
}

fn letter_ink_ref(ctx: &MathCtx, font_size: f32) -> InkBox {
    char_ink(ctx.fonts, ctx.face, 'α', font_size)
        .or_else(|| char_ink(ctx.fonts, ctx.face, 'x', font_size))
        .unwrap_or(InkBox {
            above: font_size * ctx.knobs.metrics.fallback_ink_above_factor,
            below: 0.0,
        })
}

fn layout_ord(text: &str, ctx: &mut MathCtx, font_size: f32) -> Result<MathBox, WeaveError> {
    match text.trim() {
        "←" => return Ok(layout_geo_arrow(ctx, font_size, true)),
        "→" | "⇒" => return Ok(layout_geo_arrow(ctx, font_size, false)),
        "∞" => return layout_infinity(ctx, font_size),
        _ => {}
    }
    let draw_size = font_size * symbol_scale(text);
    let mut box_ = layout_ord_raw(text, ctx, draw_size)?;
    if classify_symbol(text) == AtomKind::Op {
        let center = (box_.height - box_.depth) / 2.0;
        let dy = math_axis(font_size, ctx.knobs) - center;
        box_ = shift_box_vert(box_, dy);
    }
    Ok(box_)
}

/// Stroked arrow sized/centered to match surrounding letter ink (not the tiny → glyph).
fn layout_geo_arrow(ctx: &MathCtx, font_size: f32, left: bool) -> MathBox {
    let ink = letter_ink_ref(ctx, font_size);
    let height = ink
        .span()
        .max(font_size * ctx.knobs.arrow.min_height_factor);
    let width = font_size * ctx.knobs.arrow.width_factor;
    let thickness = (font_size * ctx.knobs.arrow.thickness_factor)
        .clamp(ctx.knobs.arrow.thickness_min, ctx.knobs.arrow.thickness_max);
    let y = ink.center();
    MathBox {
        width,
        height: y + height / 2.0,
        depth: (height / 2.0 - y).max(0.0),
        elements: vec![RelEl::Arrow {
            x: 0.0,
            y,
            width,
            height,
            thickness,
            left,
        }],
    }
}

/// Upright ∞: ink-matched to letters, then optically enlarged / lowered / padded.
fn layout_infinity(ctx: &mut MathCtx, font_size: f32) -> Result<MathBox, WeaveError> {
    let face_u = upright_face(ctx.face);
    let reference = letter_ink_ref(ctx, font_size);
    let probe = char_ink(ctx.fonts, face_u, '∞', font_size).unwrap_or(InkBox {
        above: font_size * ctx.knobs.infinity.ref_above_factor,
        below: font_size * ctx.knobs.infinity.ref_below_factor,
    });
    // ∞ reads optically small/light vs Greek; bump past geometric ink match.
    let scale = if probe.span() > 0.01 {
        (reference.span() / probe.span() * ctx.knobs.infinity.scale_boost)
            .clamp(ctx.knobs.infinity.scale_min, ctx.knobs.infinity.scale_max)
    } else {
        1.65
    };
    let draw_size = font_size * scale;
    let saved_face = ctx.face;
    ctx.face = face_u;
    let mut box_ = layout_ord_raw("∞", ctx, draw_size)?;
    ctx.face = saved_face;
    if let Some(ink) = char_ink(ctx.fonts, face_u, '∞', draw_size) {
        // Center-match, then nudge down — glyph centers sit optically high.
        let dy = reference.center() - ink.center() - font_size * ctx.knobs.infinity.lower_factor;
        box_ = shift_box_vert(box_, dy);
        box_.height = box_.height.max(ink.above + dy);
        box_.depth = box_.depth.max((ink.below - dy).max(0.0));
    }
    // Tiny breath after → / before ∞ only (size/vertical already set).
    let pad = mu(font_size, ctx.knobs.infinity.pad_mu, ctx.knobs);
    Ok(pad_box_h(box_, pad, pad))
}

fn shift_box_vert(box_: MathBox, dy: f32) -> MathBox {
    MathBox {
        width: box_.width,
        height: (box_.height + dy).max(0.0),
        depth: (box_.depth - dy).max(0.0),
        elements: offset_elements(box_.elements, 0.0, dy),
    }
}

fn pad_box_h(box_: MathBox, left: f32, right: f32) -> MathBox {
    MathBox {
        width: box_.width + left + right,
        height: box_.height,
        depth: box_.depth,
        elements: offset_elements(box_.elements, left, 0.0),
    }
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
            x += mu(font_size, space_mu(kinds[i - 1], kinds[i]), ctx.knobs);
            // Extra clearance only after large-op limits (∑_{i=1}^{n} i), not ordinary x^y.
            if kinds[i - 1] == AtomKind::Op && matches!(&items[i - 1], MathExpr::Scripts { .. }) {
                x += mu(font_size, ctx.knobs.metrics.op_after_space_mu, ctx.knobs);
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
    let thickness =
        (font_size * ctx.knobs.frac.rule_thickness_factor).max(ctx.knobs.frac.rule_thickness_min);
    // Slightly more air under the bar than above so num/den look even optically.
    let gap_num = font_size * ctx.knobs.frac.gap_num_factor;
    let gap_den = font_size * ctx.knobs.frac.gap_den_factor;
    let axis = math_axis(font_size, ctx.knobs);
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

fn layout_scripts(
    base: &MathExpr,
    sup: Option<&MathExpr>,
    sub: Option<&MathExpr>,
    ctx: &mut MathCtx,
    font_size: f32,
) -> Result<MathBox, WeaveError> {
    let base_box = layout_expr(base, ctx, font_size)?;
    let script_size = font_size * ctx.knobs.script.size_factor;
    let superscript = layout_opt(sup, ctx, script_size)?;
    let subscript = layout_opt(sub, ctx, script_size)?;
    let script_w = superscript
        .as_ref()
        .map_or(0.0, |b| b.width)
        .max(subscript.as_ref().map_or(0.0, |b| b.width));
    // Asymmetric: a little air before y; trim advance sidebearing after y.
    let script_gap = mu(font_size, ctx.knobs.script.gap_mu, ctx.knobs);
    let script_x = base_box.width + script_gap;
    let width =
        (script_x + script_w - mu(font_size, ctx.knobs.script.overlap_mu, ctx.knobs)).max(script_x);
    let mut height = base_box.height;
    let mut depth = base_box.depth;
    let mut elements = base_box.elements;
    if let Some(superscript) = superscript {
        let y = font_size * ctx.knobs.script.superscript_raise_factor;
        height = height.max(y + superscript.height);
        append_box(&mut elements, superscript, script_x, y);
    }
    if let Some(subscript) = subscript {
        let y = -font_size * ctx.knobs.script.subscript_lower_factor;
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
    let axis = math_axis(font_size, ctx.knobs);
    // Center the matrix body on the math axis (aligns with fraction bars).
    let half = inner_h / 2.0;
    let body_top = axis + half;
    let body_bot = axis - half;
    let height = body_top + pad * 0.5;
    let depth = -body_bot + pad * 0.5;

    // Stroked upright parens (not italic glyphs) sized to the matrix body.
    let mut elements = Vec::new();
    let (delim_w, width) = if delimited {
        let half_h = half + pad * 0.15;
        let paren_w = (half_h * ctx.knobs.paren.width_factor)
            .clamp(ctx.knobs.paren.width_min, ctx.knobs.paren.width_max);
        let thick = (font_size * ctx.knobs.paren.thickness_factor)
            .clamp(ctx.knobs.paren.thickness_min, ctx.knobs.paren.thickness_max);
        let width = inner_w + 2.0 * pad + 2.0 * paren_w;
        elements.push(paren_element(0.0, axis, half_h, paren_w, thick, true));
        elements.push(paren_element(
            width - paren_w,
            axis,
            half_h,
            paren_w,
            thick,
            false,
        ));
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

/// Shape an Ord without Bin/Rel axis centering (used for stretchy-ish delims).
fn layout_ord_raw(text: &str, ctx: &mut MathCtx, font_size: f32) -> Result<MathBox, WeaveError> {
    let text = if text.is_empty() { "\u{00A0}" } else { text };
    let glyphs = shape_text(ctx.fonts, ctx.face, text, font_size)?;
    let set = ctx.glyph_sets.entry(ctx.face).or_default();
    collect_glyph_set(ctx.fonts, ctx.face, text, set);
    note_shaped_glyphs(&glyphs, set);
    Ok(MathBox {
        width: shaped_width(&glyphs),
        height: font_size * ctx.knobs.metrics.notdef_height_factor,
        depth: font_size * ctx.knobs.metrics.notdef_depth_factor,
        elements: vec![RelEl::Text {
            x: 0.0,
            y: 0.0,
            face: ctx.face,
            font_size,
            glyphs,
        }],
    })
}
