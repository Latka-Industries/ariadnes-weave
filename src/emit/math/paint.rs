//! PDF paint for structured math boxes (glyphs + geometric chrome).

use pdf_writer::types::{LineCapStyle, LineJoinStyle};
use pdf_writer::{Content, Name, Str};

use crate::font::{FontBag, encode_gids};
use crate::knobs::{MathParenStyle, PageChromeKnobs};

use super::super::types::{LaidMath, LaidMathEl, MathSymKind};

struct ArrowGeom {
    x: f32,
    mid_y: f32,
    width: f32,
    height: f32,
    thickness: f32,
    left: bool,
}

struct ParenGeom {
    x: f32,
    axis_y: f32,
    half_h: f32,
    width: f32,
    thickness: f32,
    left: bool,
    style: MathParenStyle,
}

struct IntegralGeom {
    x: f32,
    axis_y: f32,
    half_h: f32,
    width: f32,
    thickness: f32,
    contour: bool,
}

struct RadicalGeom {
    x: f32,
    top_y: f32,
    height: f32,
    width: f32,
    thickness: f32,
}

struct SymGeom {
    kind: MathSymKind,
    x: f32,
    mid_y: f32,
    width: f32,
    height: f32,
    thickness: f32,
}

/// Shared stroke prelude for math chrome (rules, arrows, parens, integrals, radicals).
fn begin_math_stroke(
    content: &mut Content,
    chrome: &PageChromeKnobs,
    thickness: f32,
    round_cap: bool,
    round_join: bool,
) {
    content.save_state();
    content.set_stroke_gray(chrome.stroke_gray);
    content.set_line_width(thickness);
    if round_cap {
        content.set_line_cap(LineCapStyle::RoundCap);
    }
    if round_join {
        content.set_line_join(LineJoinStyle::RoundJoin);
    }
}

/// Paint a structured math box; `top_y` is the top edge in PDF space.
pub(in crate::emit) fn paint_math(
    content: &mut Content,
    math: &LaidMath,
    origin_x: f32,
    top_y: f32,
    content_width: f32,
    fonts: &FontBag,
    chrome: &PageChromeKnobs,
) {
    let origin_x = if math.center {
        origin_x + (content_width - math.width) / 2.0
    } else {
        origin_x
    };
    for el in &math.elements {
        paint_math_el(content, el, origin_x, top_y, fonts, chrome);
    }
}

fn paint_math_el(
    content: &mut Content,
    el: &LaidMathEl,
    origin_x: f32,
    top_y: f32,
    fonts: &FontBag,
    chrome: &PageChromeKnobs,
) {
    match el {
        LaidMathEl::Text {
            x,
            y,
            face,
            font_size,
            glyphs,
        } => {
            let face_name = fonts.resource_name(*face);
            content.begin_text();
            content.set_font(Name(&face_name), *font_size);
            content.set_text_matrix([1.0, 0.0, 0.0, 1.0, origin_x + x, top_y - y]);
            content.show(Str(&encode_gids(glyphs)));
            content.end_text();
        }
        LaidMathEl::Rule {
            x,
            y,
            width,
            thickness,
        } => {
            begin_math_stroke(content, chrome, *thickness, false, false);
            let rule_y = top_y - y;
            content.move_to(origin_x + x, rule_y);
            content.line_to(origin_x + x + width, rule_y);
            content.stroke();
            content.restore_state();
        }
        LaidMathEl::Paren { .. }
        | LaidMathEl::Arrow { .. }
        | LaidMathEl::Integral { .. }
        | LaidMathEl::Radical { .. }
        | LaidMathEl::Sym { .. } => {
            paint_math_chrome(content, el, origin_x, top_y, chrome);
        }
    }
}

fn paint_math_chrome(
    content: &mut Content,
    el: &LaidMathEl,
    origin_x: f32,
    top_y: f32,
    chrome: &PageChromeKnobs,
) {
    match el {
        LaidMathEl::Paren {
            x,
            axis_y,
            half_h,
            width,
            thickness,
            left,
            style,
        } => paint_math_paren(
            content,
            &ParenGeom {
                x: origin_x + x,
                axis_y: top_y - axis_y,
                half_h: *half_h,
                width: *width,
                thickness: *thickness,
                left: *left,
                style: *style,
            },
            chrome,
        ),
        LaidMathEl::Arrow {
            x,
            y,
            width,
            height,
            thickness,
            left,
        } => paint_math_arrow(
            content,
            &ArrowGeom {
                x: origin_x + x,
                mid_y: top_y - y,
                width: *width,
                height: *height,
                thickness: *thickness,
                left: *left,
            },
            chrome,
        ),
        LaidMathEl::Integral {
            x,
            axis_y,
            half_h,
            width,
            thickness,
            contour,
        } => paint_math_integral(
            content,
            &IntegralGeom {
                x: origin_x + x,
                axis_y: top_y - axis_y,
                half_h: *half_h,
                width: *width,
                thickness: *thickness,
                contour: *contour,
            },
            chrome,
        ),
        LaidMathEl::Radical {
            x,
            y,
            height,
            width,
            thickness,
        } => paint_math_radical(
            content,
            &RadicalGeom {
                x: origin_x + x,
                top_y: top_y - y,
                height: *height,
                width: *width,
                thickness: *thickness,
            },
            chrome,
        ),
        LaidMathEl::Sym {
            kind,
            x,
            y,
            width,
            height,
            thickness,
        } => paint_math_sym(
            content,
            &SymGeom {
                kind: *kind,
                x: origin_x + x,
                mid_y: top_y - y,
                width: *width,
                height: *height,
                thickness: *thickness,
            },
            chrome,
        ),
        LaidMathEl::Text { .. } | LaidMathEl::Rule { .. } => unreachable!("glyphs handled above"),
    }
}

/// Stroke a radical checkmark; top-right joins the vinculum [`LaidMathEl::Rule`].
fn paint_math_radical(content: &mut Content, geom: &RadicalGeom, chrome: &PageChromeKnobs) {
    let RadicalGeom {
        x,
        top_y,
        height,
        width,
        thickness,
    } = *geom;
    // Path: left tick → bottom V → up to vinculum join (top-right of radical box).
    let join_x = x + width;
    let join_y = top_y;
    let bot_y = top_y - height;
    let v_x = x + width * 0.38;
    let tick_x = x + width * 0.08;
    let tick_y = bot_y + height * 0.28;
    begin_math_stroke(content, chrome, thickness, true, true);
    content.move_to(tick_x, tick_y);
    content.line_to(v_x, bot_y + thickness * 0.5);
    content.line_to(join_x, join_y);
    content.stroke();
    content.restore_state();
}

fn paint_math_arrow(content: &mut Content, geom: &ArrowGeom, chrome: &PageChromeKnobs) {
    let ArrowGeom {
        x,
        mid_y,
        width,
        height,
        thickness,
        left,
    } = *geom;
    let head_w = width * 0.32;
    let head_h = height * 0.55;
    let (tail_x, tip_x, head_base) = if left {
        (x + width, x, x + head_w)
    } else {
        (x, x + width, x + width - head_w)
    };
    begin_math_stroke(content, chrome, thickness, true, false);
    content.set_fill_gray(chrome.fill_gray);
    content.move_to(tail_x, mid_y);
    content.line_to(head_base, mid_y);
    content.stroke();
    content.move_to(tip_x, mid_y);
    content.line_to(head_base, mid_y + head_h / 2.0);
    content.line_to(head_base, mid_y - head_h / 2.0);
    content.close_path();
    content.fill_nonzero();
    content.restore_state();
}

/// Stroke a tall upright integral (optional contour loop), centered on the math axis.
///
/// Kept nearly vertical (minimal italic slant) so display under/over limits center
/// cleanly above/below like ∑, rather than tucking into slanted hooks.
fn paint_math_integral(content: &mut Content, geom: &IntegralGeom, chrome: &PageChromeKnobs) {
    let IntegralGeom {
        x,
        axis_y,
        half_h,
        width,
        thickness,
        contour,
    } = *geom;
    let top = axis_y + half_h;
    let bot = axis_y - half_h;
    let mid = axis_y;
    let cx = x + width * 0.5;
    let serif = width * 0.28;
    begin_math_stroke(content, chrome, thickness, true, true);
    // Top serif (rightward) → upright stem with slight S → bottom serif (leftward).
    content.move_to(cx + serif, top);
    content.line_to(cx - serif * 0.15, top);
    content.cubic_to(
        cx - serif * 0.35,
        top - half_h * 0.08,
        cx - serif * 0.2,
        mid + half_h * 0.35,
        cx,
        mid,
    );
    content.cubic_to(
        cx + serif * 0.2,
        mid - half_h * 0.35,
        cx + serif * 0.35,
        bot + half_h * 0.08,
        cx + serif * 0.15,
        bot,
    );
    content.line_to(cx - serif, bot);
    content.stroke();
    if contour {
        let r = width * 0.16;
        content.set_line_width(thickness * 0.85);
        let k = 0.5523 * r;
        content.move_to(cx + r, mid);
        content.cubic_to(cx + r, mid + k, cx + k, mid + r, cx, mid + r);
        content.cubic_to(cx - k, mid + r, cx - r, mid + k, cx - r, mid);
        content.cubic_to(cx - r, mid - k, cx - k, mid - r, cx, mid - r);
        content.cubic_to(cx + k, mid - r, cx + r, mid - k, cx + r, mid);
        content.stroke();
    }
    content.restore_state();
}

/// Stroke a stretchy matrix delimiter centered on `axis_y` (PDF space).
fn paint_math_paren(content: &mut Content, geom: &ParenGeom, chrome: &PageChromeKnobs) {
    let ParenGeom {
        x,
        axis_y,
        half_h,
        width,
        thickness,
        left,
        style,
    } = *geom;
    let top = axis_y + half_h;
    let bot = axis_y - half_h;
    begin_math_stroke(content, chrome, thickness, true, false);
    match style {
        MathParenStyle::Square => {
            if left {
                content.move_to(x + width, top);
                content.line_to(x, top);
                content.line_to(x, bot);
                content.line_to(x + width, bot);
            } else {
                content.move_to(x, top);
                content.line_to(x + width, top);
                content.line_to(x + width, bot);
                content.line_to(x, bot);
            }
        }
        MathParenStyle::Round => {
            let mid = axis_y;
            if left {
                content.move_to(x + width, top);
                content.cubic_to(
                    x + width * 0.15,
                    top - half_h * 0.05,
                    x,
                    mid + half_h * 0.45,
                    x,
                    mid,
                );
                content.cubic_to(
                    x,
                    mid - half_h * 0.45,
                    x + width * 0.15,
                    bot + half_h * 0.05,
                    x + width,
                    bot,
                );
            } else {
                content.move_to(x, top);
                content.cubic_to(
                    x + width * 0.85,
                    top - half_h * 0.05,
                    x + width,
                    mid + half_h * 0.45,
                    x + width,
                    mid,
                );
                content.cubic_to(
                    x + width,
                    mid - half_h * 0.45,
                    x + width * 0.85,
                    bot + half_h * 0.05,
                    x,
                    bot,
                );
            }
        }
    }
    content.stroke();
    content.restore_state();
}

fn paint_math_sym(content: &mut Content, geom: &SymGeom, chrome: &PageChromeKnobs) {
    let SymGeom {
        kind,
        x,
        mid_y,
        width,
        height,
        thickness,
    } = *geom;
    let top = mid_y + height / 2.0;
    let bot = mid_y - height / 2.0;
    let cx = x + width * 0.5;
    begin_math_stroke(content, chrome, thickness, true, true);
    match kind {
        MathSymKind::In | MathSymKind::NotIn => {
            paint_sym_in(content, x, mid_y, width, height, bot, top, thickness, kind);
        }
        MathSymKind::Subset
        | MathSymKind::Superset
        | MathSymKind::SubsetEq
        | MathSymKind::SupersetEq => {
            paint_sym_subset(content, kind, x, mid_y, width, height, bot, top);
        }
        MathSymKind::Cup | MathSymKind::BigCup => {
            paint_sym_cup(content, x, mid_y, width, height, top, bot, false);
        }
        MathSymKind::BigCap => {
            paint_sym_cup(content, x, mid_y, width, height, top, bot, true);
        }
        MathSymKind::Coprod => paint_sym_coprod(content, x, width, top, bot),
        MathSymKind::Forall => paint_sym_forall(content, x, cx, width, height, mid_y, top, bot),
        MathSymKind::Exists => paint_sym_exists(content, x, width, mid_y, top, bot),
        MathSymKind::Empty => paint_sym_empty(content, cx, mid_y, width, height, top, bot),
        MathSymKind::Circ => {
            // Keep the ring optically small inside a padded advance box.
            let r = height * 0.38;
            ellipse_stroke(content, cx, mid_y, r, r);
            content.stroke();
        }
        MathSymKind::Mp => paint_sym_mp(content, x, cx, width, height, mid_y),
        MathSymKind::Nabla => {
            content.move_to(x + width * 0.08, top);
            content.line_to(x + width * 0.92, top);
            content.line_to(cx, bot);
            content.close_path();
            content.stroke();
        }
    }
    content.restore_state();
}

#[allow(clippy::too_many_arguments)]
fn paint_sym_in(
    content: &mut Content,
    x: f32,
    mid_y: f32,
    width: f32,
    height: f32,
    bot: f32,
    top: f32,
    thickness: f32,
    kind: MathSymKind,
) {
    // ∈ shares ⊂'s open-right bowl, plus a mid bar (not a square E).
    let near = x + width * 0.18;
    let far = x + width * 0.9;
    content.move_to(far, top - height * 0.08);
    content.cubic_to(
        near,
        top - height * 0.08,
        near,
        mid_y + height * 0.08,
        near,
        mid_y,
    );
    content.cubic_to(
        near,
        mid_y - height * 0.08,
        near,
        bot + height * 0.08,
        far,
        bot + height * 0.08,
    );
    content.stroke();
    content.move_to(near, mid_y);
    content.line_to(x + width * 0.78, mid_y);
    content.stroke();
    if kind == MathSymKind::NotIn {
        // Same stroke weight as the bowl — a heavier slash reads as a scribble.
        content.set_line_width(thickness);
        content.move_to(x + width * 0.06, bot + height * 0.12);
        content.line_to(x + width * 0.94, top - height * 0.12);
        content.stroke();
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_sym_subset(
    content: &mut Content,
    kind: MathSymKind,
    x: f32,
    mid_y: f32,
    width: f32,
    height: f32,
    bot: f32,
    top: f32,
) {
    let open_right = matches!(kind, MathSymKind::Subset | MathSymKind::SubsetEq);
    let (near, far) = if open_right {
        (x + width * 0.12, x + width * 0.88)
    } else {
        (x + width * 0.88, x + width * 0.12)
    };
    content.move_to(far, top - height * 0.08);
    content.cubic_to(
        near,
        top - height * 0.08,
        near,
        mid_y + height * 0.08,
        near,
        mid_y,
    );
    content.cubic_to(
        near,
        mid_y - height * 0.08,
        near,
        bot + height * 0.08,
        far,
        bot + height * 0.08,
    );
    content.stroke();
    if matches!(kind, MathSymKind::SubsetEq | MathSymKind::SupersetEq) {
        let y = bot + height * 0.02;
        content.move_to(x + width * 0.18, y);
        content.line_to(x + width * 0.82, y);
        content.stroke();
    }
}

/// `cap=false` draws ∪ (open at top); `cap=true` draws ∩ (open at bottom).
#[allow(clippy::too_many_arguments)]
fn paint_sym_cup(
    content: &mut Content,
    x: f32,
    mid_y: f32,
    width: f32,
    height: f32,
    top: f32,
    bot: f32,
    cap: bool,
) {
    let left = x + width * 0.12;
    let right = x + width * 0.88;
    if cap {
        content.move_to(left, bot);
        content.line_to(left, mid_y + height * 0.05);
        content.cubic_to(
            left,
            top - height * 0.05,
            right,
            top - height * 0.05,
            right,
            mid_y + height * 0.05,
        );
        content.line_to(right, bot);
    } else {
        content.move_to(left, top);
        content.line_to(left, mid_y - height * 0.05);
        content.cubic_to(
            left,
            bot + height * 0.05,
            right,
            bot + height * 0.05,
            right,
            mid_y - height * 0.05,
        );
        content.line_to(right, top);
    }
    content.stroke();
}

fn paint_sym_coprod(content: &mut Content, x: f32, width: f32, top: f32, bot: f32) {
    content.move_to(x + width * 0.1, top);
    content.line_to(x + width * 0.9, top);
    content.move_to(x + width * 0.22, top);
    content.line_to(x + width * 0.22, bot);
    content.move_to(x + width * 0.78, top);
    content.line_to(x + width * 0.78, bot);
    content.stroke();
}

#[allow(clippy::too_many_arguments)]
fn paint_sym_forall(
    content: &mut Content,
    x: f32,
    cx: f32,
    width: f32,
    height: f32,
    mid_y: f32,
    top: f32,
    bot: f32,
) {
    content.move_to(x + width * 0.08, top);
    content.line_to(cx, bot);
    content.line_to(x + width * 0.92, top);
    content.move_to(x + width * 0.28, mid_y + height * 0.05);
    content.line_to(x + width * 0.72, mid_y + height * 0.05);
    content.stroke();
}

fn paint_sym_exists(content: &mut Content, x: f32, width: f32, mid_y: f32, top: f32, bot: f32) {
    content.move_to(x + width * 0.85, top);
    content.line_to(x + width * 0.2, top);
    content.line_to(x + width * 0.2, bot);
    content.line_to(x + width * 0.85, bot);
    content.move_to(x + width * 0.2, mid_y);
    content.line_to(x + width * 0.72, mid_y);
    content.stroke();
}

#[allow(clippy::too_many_arguments)]
fn paint_sym_empty(
    content: &mut Content,
    cx: f32,
    mid_y: f32,
    width: f32,
    height: f32,
    top: f32,
    bot: f32,
) {
    let rx = width * 0.32;
    let ry = height * 0.36;
    ellipse_stroke(content, cx, mid_y, rx, ry);
    content.move_to(cx - rx * 0.85, bot + height * 0.2);
    content.line_to(cx + rx * 0.85, top - height * 0.2);
    content.stroke();
}

fn paint_sym_mp(content: &mut Content, x: f32, cx: f32, width: f32, height: f32, mid_y: f32) {
    content.move_to(x + width * 0.15, mid_y + height * 0.22);
    content.line_to(x + width * 0.85, mid_y + height * 0.22);
    content.move_to(x + width * 0.15, mid_y - height * 0.18);
    content.line_to(x + width * 0.85, mid_y - height * 0.18);
    content.move_to(cx, mid_y - height * 0.38);
    content.line_to(cx, mid_y + height * 0.02);
    content.stroke();
}

fn ellipse_stroke(content: &mut Content, cx: f32, cy: f32, rx: f32, ry: f32) {
    let k = 0.5523;
    content.move_to(cx + rx, cy);
    content.cubic_to(cx + rx, cy + k * ry, cx + k * rx, cy + ry, cx, cy + ry);
    content.cubic_to(cx - k * rx, cy + ry, cx - rx, cy + k * ry, cx - rx, cy);
    content.cubic_to(cx - rx, cy - k * ry, cx - k * rx, cy - ry, cx, cy - ry);
    content.cubic_to(cx + k * rx, cy - ry, cx + rx, cy - k * ry, cx + rx, cy);
}
