//! Build PDF content streams from laid items (text, images, tables).

use std::collections::{BTreeMap, BTreeSet};

use pdf_writer::types::{LineCapStyle, LineJoinStyle};
use pdf_writer::{Content, Name, Str};

use crate::error::WeaveError;
use crate::font::{FaceId, FaceRef, FontBag, encode_gids, shape_text, shaped_width};
use crate::knobs::{LayoutKnobs, PageChromeKnobs, TextAlign};
use crate::profile::ProfileMetrics;

use super::types::{LaidColumns, LaidItem, LaidMath, LaidMathEl, LaidSpan, LaidTable, SubsetMap};

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

fn paint_span_text(
    content: &mut Content,
    fonts: &FontBag,
    span: &LaidSpan,
    origin_x: f32,
    baseline_y: f32,
) {
    if let Some([red, green, blue]) = non_black_rgb(span.fill) {
        content.set_fill_rgb(red, green, blue);
    }
    let face_name = fonts.resource_name(span.face);
    content.set_font(Name(&face_name), span.font_size);
    content.set_text_matrix([1.0, 0.0, 0.0, 1.0, origin_x, baseline_y]);
    content.show(Str(&encode_gids(&span.glyphs)));
}

fn stroke_span_underline(
    content: &mut Content,
    span: &LaidSpan,
    origin_x: f32,
    baseline_y: f32,
    width: f32,
) {
    if !span.underline || width <= 0.0 {
        return;
    }
    let [red, green, blue] = span.fill;
    let underline_y = baseline_y - span.font_size * 0.12;
    content.save_state();
    content.set_stroke_rgb(red, green, blue);
    content.set_line_width((span.font_size * 0.06).max(0.4));
    content.move_to(origin_x, underline_y);
    content.line_to(origin_x + width, underline_y);
    content.stroke();
    content.restore_state();
}

fn paint_span_underlines(
    content: &mut Content,
    spans: &[LaidSpan],
    mut origin_x: f32,
    baseline_y: f32,
) {
    for span in spans {
        let width = shaped_width(&span.glyphs);
        stroke_span_underline(content, span, origin_x, baseline_y, width);
        origin_x += width;
    }
}

/// Paint a horizontal run of spans (text object + optional underlines).
fn paint_laid_spans(
    content: &mut Content,
    fonts: &FontBag,
    spans: &[LaidSpan],
    origin_x: f32,
    baseline_y: f32,
) {
    if spans.is_empty() {
        return;
    }
    content.begin_text();
    let mut x = origin_x;
    for span in spans {
        paint_span_text(content, fonts, span, x, baseline_y);
        x += shaped_width(&span.glyphs);
    }
    content.end_text();
    paint_span_underlines(content, spans, origin_x, baseline_y);
}

/// Per-line justify geometry: stretch slots, discarded trailing WS, optical width.
struct JustifyPlan {
    space_slots: Vec<(usize, usize)>,
    trailing: BTreeSet<(usize, usize)>,
    /// Advance width excluding trailing WS, with last glyph ink flush.
    natural: f32,
}

fn plan_justify(spans: &[LaidSpan]) -> JustifyPlan {
    let mut space_slots: Vec<(usize, usize)> = Vec::new();
    for (si, span) in spans.iter().enumerate() {
        for (gi, glyph) in span.glyphs.iter().enumerate() {
            if glyph.is_whitespace {
                space_slots.push((si, gi));
            }
        }
    }
    let mut trailing = BTreeSet::new();
    while space_slots
        .last()
        .is_some_and(|&(si, gi)| is_line_tail_whitespace(spans, si, gi))
    {
        if let Some(slot) = space_slots.pop() {
            trailing.insert(slot);
        }
    }

    let mut natural_advance = 0.0_f32;
    let mut last_non_ws = None;
    for (si, span) in spans.iter().enumerate() {
        for (gi, glyph) in span.glyphs.iter().enumerate() {
            if trailing.contains(&(si, gi)) {
                continue;
            }
            natural_advance += glyph.advance;
            if !glyph.is_whitespace {
                last_non_ws = Some(*glyph);
            }
        }
    }
    let natural = match last_non_ws {
        Some(glyph) if glyph.ink_x_max > 0.0 && glyph.ink_x_max < glyph.advance => {
            natural_advance - glyph.advance + glyph.ink_x_max
        }
        _ => natural_advance,
    };
    JustifyPlan {
        space_slots,
        trailing,
        natural,
    }
}

/// Word-justify spans across `measure` by padding inter-word whitespace advances.
///
/// Trailing whitespace is discarded for width/slack (and not painted). Slack is
/// computed so the **ink** of the last glyph (not its advance box) meets the band
/// end — otherwise side-bearing leaves a visible hairline short of the figure edge.
fn paint_justified_spans(
    content: &mut Content,
    fonts: &FontBag,
    spans: &[LaidSpan],
    origin_x: f32,
    baseline_y: f32,
    measure: f32,
) {
    if spans.is_empty() {
        return;
    }
    let plan = plan_justify(spans);
    let slack = (measure - plan.natural).max(0.0);
    let space_pads = distribute_justify_pads(&plan.space_slots, slack);

    content.begin_text();
    let mut x = origin_x;
    for (si, span) in spans.iter().enumerate() {
        if let Some([red, green, blue]) = non_black_rgb(span.fill) {
            content.set_fill_rgb(red, green, blue);
        }
        let face_name = fonts.resource_name(span.face);
        content.set_font(Name(&face_name), span.font_size);
        for (gi, glyph) in span.glyphs.iter().enumerate() {
            if plan.trailing.contains(&(si, gi)) {
                continue;
            }
            content.set_text_matrix([1.0, 0.0, 0.0, 1.0, x, baseline_y]);
            content.show(Str(&encode_gids(std::slice::from_ref(glyph))));
            x += glyph.advance;
            if let Some(&pad) = space_pads.get(&(si, gi)) {
                x += pad;
            }
        }
    }
    content.end_text();
    paint_span_underlines_justified(
        content,
        spans,
        origin_x,
        baseline_y,
        &space_pads,
        &plan.trailing,
    );
}

/// Equal gap pads with residual on the last slot so advances sum exactly to slack.
fn distribute_justify_pads(
    space_slots: &[(usize, usize)],
    slack: f32,
) -> BTreeMap<(usize, usize), f32> {
    let mut pads = BTreeMap::new();
    if space_slots.is_empty() || slack <= 0.0 {
        return pads;
    }
    let n = space_slots.len();
    let each = slack / n as f32;
    let mut used = 0.0_f32;
    for (i, &slot) in space_slots.iter().enumerate() {
        let pad = if i + 1 == n {
            (slack - used).max(0.0)
        } else {
            each
        };
        used += pad;
        pads.insert(slot, pad);
    }
    pads
}

fn is_line_tail_whitespace(spans: &[LaidSpan], span_i: usize, glyph_i: usize) -> bool {
    // True when every glyph after (span_i, glyph_i) is also whitespace (or absent).
    for (si, span) in spans.iter().enumerate().skip(span_i) {
        let start = if si == span_i { glyph_i + 1 } else { 0 };
        for glyph in span.glyphs.iter().skip(start) {
            if !glyph.is_whitespace {
                return false;
            }
        }
    }
    true
}

fn paint_span_underlines_justified(
    content: &mut Content,
    spans: &[LaidSpan],
    mut origin_x: f32,
    baseline_y: f32,
    space_pads: &BTreeMap<(usize, usize), f32>,
    trailing: &BTreeSet<(usize, usize)>,
) {
    for (si, span) in spans.iter().enumerate() {
        let width: f32 = span
            .glyphs
            .iter()
            .enumerate()
            .filter(|(gi, _)| !trailing.contains(&(si, *gi)))
            .map(|(gi, g)| g.advance + space_pads.get(&(si, gi)).copied().unwrap_or(0.0))
            .sum();
        stroke_span_underline(content, span, origin_x, baseline_y, width);
        origin_x += width;
    }
}

/// `Some(rgb)` when fill is not engine black (omit ops to keep default PDFs byte-stable).
fn non_black_rgb(fill: [f32; 3]) -> Option<[f32; 3]> {
    let [red, green, blue] = fill;
    (red != 0.0 || green != 0.0 || blue != 0.0).then_some(fill)
}

/// Resource name bytes for image `XObject` `Im{idx}`.
pub(super) fn image_resource_name(idx: usize) -> Vec<u8> {
    format!("Im{idx}").into_bytes()
}

/// Paint one page's items top-down and append a centered page-number footer.
///
/// # Errors
///
/// Returns [`WeaveError::Font`] if footer shaping fails.
pub(super) fn build_page_content(
    items: &[LaidItem],
    metrics: &ProfileMetrics,
    page_no: usize,
    page_count: usize,
    fonts: &FontBag,
    subsets: &SubsetMap,
    knobs: &LayoutKnobs,
) -> Result<Vec<u8>, WeaveError> {
    let mut content = Content::new();
    let mut y = metrics.page_h - metrics.margin;
    let bottom_limit = metrics.margin + knobs.page.content.bottom_clearance;

    for item in items {
        if !paint_page_item(
            &mut content,
            item,
            &mut y,
            bottom_limit,
            metrics,
            fonts,
            knobs,
        ) {
            break;
        }
    }

    paint_page_footer(
        &mut content,
        metrics,
        page_no,
        page_count,
        fonts,
        subsets,
        knobs,
    )?;
    Ok(content.finish().into_vec())
}

/// Paint one laid item; returns `false` when the cursor is below `bottom_limit`.
fn paint_page_item(
    content: &mut Content,
    item: &LaidItem,
    y: &mut f32,
    bottom_limit: f32,
    metrics: &ProfileMetrics,
    fonts: &FontBag,
    knobs: &LayoutKnobs,
) -> bool {
    match item {
        LaidItem::Text(line) => {
            *y -= line.leading;
            if *y < bottom_limit {
                return false;
            }
            if !line.is_gap() {
                let origin_x = metrics.margin + line.indent;
                match line.text_align {
                    TextAlign::Justify => {
                        paint_justified_spans(
                            content,
                            fonts,
                            &line.spans,
                            origin_x,
                            *y,
                            line.measure.max(line.width()),
                        );
                    }
                    align => {
                        let x =
                            origin_x + align.offset_x(line.measure.max(line.width()), line.width());
                        paint_laid_spans(content, fonts, &line.spans, x, *y);
                    }
                }
            }
        }
        LaidItem::Image {
            img_idx,
            width,
            height,
            glue_after: _,
            gap_after,
            align,
        } => {
            *y -= *height;
            if *y < bottom_limit {
                return false;
            }
            let name = image_resource_name(*img_idx);
            let x = metrics.margin + align.offset_x(metrics.content_width(), *width);
            content.save_state();
            content.transform([*width, 0.0, 0.0, *height, x, *y]);
            content.x_object(Name(&name));
            content.restore_state();
            *y -= *gap_after;
        }
        LaidItem::Table(table) => {
            let table_h = table.rows.iter().map(|r| r.height).sum::<f32>();
            if *y - table_h < bottom_limit {
                return false;
            }
            paint_table(content, table, metrics.margin, *y, fonts);
            *y -= table_h + table.gap_after;
        }
        LaidItem::Columns(cols) => {
            let h = cols.height() - cols.gap_after;
            if *y - h < bottom_limit {
                return false;
            }
            paint_columns(content, cols, metrics.margin, *y, fonts);
            *y -= cols.height();
        }
        LaidItem::Math(math) => {
            if *y - math.height < bottom_limit {
                return false;
            }
            paint_math(
                content,
                math,
                metrics.margin,
                *y,
                metrics.content_width(),
                fonts,
                &knobs.page.chrome,
            );
            *y -= math.height + math.gap_after;
        }
        LaidItem::Rule {
            width,
            thickness,
            leading,
            gap_after,
        } => {
            if *y - *leading < bottom_limit {
                return false;
            }
            paint_layout_rule(
                content,
                metrics.margin,
                *y,
                *width,
                *thickness,
                *leading,
                knobs.page.chrome.stroke_gray,
            );
            *y -= *leading + *gap_after;
        }
    }
    true
}

fn paint_layout_rule(
    content: &mut Content,
    origin_x: f32,
    top_y: f32,
    width: f32,
    thickness: f32,
    leading: f32,
    stroke_gray: f32,
) {
    let mid_y = top_y - leading / 2.0;
    content.save_state();
    content.set_stroke_gray(stroke_gray);
    content.set_line_width(thickness);
    content.move_to(origin_x, mid_y);
    content.line_to(origin_x + width, mid_y);
    content.stroke();
    content.restore_state();
}

fn paint_page_footer(
    content: &mut Content,
    metrics: &ProfileMetrics,
    page_no: usize,
    page_count: usize,
    fonts: &FontBag,
    subsets: &SubsetMap,
    knobs: &LayoutKnobs,
) -> Result<(), WeaveError> {
    let footer = format!("{page_no} / {page_count}");
    let footer_face = FaceRef::Bundled(FaceId::SansRegular);
    let footer_size = knobs.page.footer.font_size;
    let mut footer_glyphs = shape_text(fonts, footer_face, &footer, footer_size)?;
    if let Some(subset) = subsets.get(&footer_face) {
        for g in &mut footer_glyphs {
            *g = subset.remap_glyph(*g);
        }
    }
    let footer_w = shaped_width(&footer_glyphs);
    let footer_y = metrics.margin * knobs.page.footer.y_margin_factor;
    let footer_x = (metrics.page_w - footer_w) / 2.0;
    let footer_name = fonts.resource_name(footer_face);
    content.begin_text();
    content.set_font(Name(&footer_name), footer_size);
    content.set_text_matrix([1.0, 0.0, 0.0, 1.0, footer_x, footer_y]);
    content.show(Str(&encode_gids(&footer_glyphs)));
    content.end_text();
    Ok(())
}

/// Paint side-by-side columns; `top_y` is the top edge in PDF space.
pub(super) fn paint_columns(
    content: &mut Content,
    cols: &LaidColumns,
    origin_x: f32,
    top_y: f32,
    fonts: &FontBag,
) {
    let mut x = origin_x;
    for (i, lines) in cols.columns.iter().enumerate() {
        let mut text_y = top_y;
        for line in lines {
            text_y -= line.leading;
            paint_laid_spans(content, fonts, &line.spans, x, text_y);
        }
        let col_w = cols.col_widths.get(i).copied().unwrap_or(0.0);
        x += col_w + cols.gap;
    }
}

/// Stroke the table grid and draw cell text; `top_y` is the top edge in PDF space.
pub(super) fn paint_table(
    content: &mut Content,
    table: &LaidTable,
    origin_x: f32,
    top_y: f32,
    fonts: &FontBag,
) {
    let table_w: f32 = table.col_widths.iter().sum();
    let table_h: f32 = table.rows.iter().map(|r| r.height).sum();
    let bottom = top_y - table_h;

    content.save_state();
    content.set_stroke_gray(0.25);
    content.set_line_width(0.6);
    content.rect(origin_x, bottom, table_w, table_h);
    content.stroke();

    let mut y = top_y;
    for row in &table.rows {
        y -= row.height;
        if (y - bottom).abs() > 0.01 {
            content.move_to(origin_x, y);
            content.line_to(origin_x + table_w, y);
            content.stroke();
        }
    }

    let mut x = origin_x;
    for width in table
        .col_widths
        .iter()
        .take(table.col_widths.len().saturating_sub(1))
    {
        x += width;
        content.move_to(x, top_y);
        content.line_to(x, bottom);
        content.stroke();
    }
    content.restore_state();

    let mut row_top = top_y;
    for row in &table.rows {
        let mut cell_x = origin_x;
        for (col, cell_lines) in row.cells.iter().enumerate() {
            let col_w = table.col_widths.get(col).copied().unwrap_or(0.0);
            let mut text_y = row_top - table.pad;
            for line in cell_lines {
                text_y -= line.leading;
                paint_laid_spans(content, fonts, &line.spans, cell_x + table.pad, text_y);
            }
            cell_x += col_w;
        }
        row_top -= row.height;
    }
}

/// Paint a structured math box; `top_y` is the top edge in PDF space.
pub(super) fn paint_math(
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
            LaidMathEl::Paren {
                x,
                axis_y,
                half_h,
                width,
                thickness,
                left,
            } => {
                paint_math_paren(
                    content,
                    &ParenGeom {
                        x: origin_x + x,
                        axis_y: top_y - axis_y,
                        half_h: *half_h,
                        width: *width,
                        thickness: *thickness,
                        left: *left,
                    },
                    chrome,
                );
            }
            LaidMathEl::Arrow {
                x,
                y,
                width,
                height,
                thickness,
                left,
            } => {
                paint_math_arrow(
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
                );
            }
            LaidMathEl::Integral {
                x,
                axis_y,
                half_h,
                width,
                thickness,
                contour,
            } => {
                paint_math_integral(
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
                );
            }
            LaidMathEl::Radical {
                x,
                y,
                height,
                width,
                thickness,
            } => {
                paint_math_radical(
                    content,
                    &RadicalGeom {
                        x: origin_x + x,
                        top_y: top_y - y,
                        height: *height,
                        width: *width,
                        thickness: *thickness,
                    },
                    chrome,
                );
            }
        }
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

/// Stroke a stretchy parenthesis centered on `axis_y` (PDF space).
fn paint_math_paren(content: &mut Content, geom: &ParenGeom, chrome: &PageChromeKnobs) {
    let ParenGeom {
        x,
        axis_y,
        half_h,
        width,
        thickness,
        left,
    } = *geom;
    let top = axis_y + half_h;
    let bot = axis_y - half_h;
    let mid = axis_y;
    begin_math_stroke(content, chrome, thickness, true, false);
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
    content.stroke();
    content.restore_state();
}
