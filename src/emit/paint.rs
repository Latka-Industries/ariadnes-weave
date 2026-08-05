//! Build PDF content streams from laid items (text, images, tables).

use pdf_writer::types::LineCapStyle;
use pdf_writer::{Content, Name, Str};

use crate::error::WeaveError;
use crate::font::{FaceId, FaceRef, FontBag, encode_gids, shape_text, shaped_width};
use crate::knobs::{LayoutKnobs, PageChromeKnobs};
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

fn paint_span_underlines(
    content: &mut Content,
    spans: &[LaidSpan],
    mut origin_x: f32,
    baseline_y: f32,
) {
    for span in spans {
        let width = shaped_width(&span.glyphs);
        if span.underline && width > 0.0 {
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
        match item {
            LaidItem::Text(line) => {
                y -= line.leading;
                if y < bottom_limit {
                    break;
                }
                if line.spans.is_empty() {
                    continue;
                }
                let x = if line.center {
                    metrics.margin + (metrics.content_width() - line.width()) / 2.0
                } else {
                    metrics.margin + line.indent
                };
                paint_laid_spans(&mut content, fonts, &line.spans, x, y);
            }
            LaidItem::Image {
                img_idx,
                width,
                height,
                glue_after: _,
            } => {
                y -= *height;
                if y < bottom_limit {
                    break;
                }
                let name = image_resource_name(*img_idx);
                content.save_state();
                content.transform([*width, 0.0, 0.0, *height, metrics.margin, y]);
                content.x_object(Name(&name));
                content.restore_state();
                y -= 8.0;
            }
            LaidItem::Table(table) => {
                let table_h = table.rows.iter().map(|r| r.height).sum::<f32>();
                if y - table_h < bottom_limit {
                    break;
                }
                paint_table(&mut content, table, metrics.margin, y, fonts);
                y -= table_h + table.gap_after;
            }
            LaidItem::Columns(cols) => {
                let h = cols.height() - cols.gap_after;
                if y - h < bottom_limit {
                    break;
                }
                paint_columns(&mut content, cols, metrics.margin, y, fonts);
                y -= cols.height();
            }
            LaidItem::Math(math) => {
                if y - math.height < bottom_limit {
                    break;
                }
                paint_math(
                    &mut content,
                    math,
                    metrics.margin,
                    y,
                    metrics.content_width(),
                    fonts,
                    &knobs.page.chrome,
                );
                y -= math.height + math.gap_after;
            }
        }
    }

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

    Ok(content.finish().into_vec())
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
                content.save_state();
                content.set_stroke_gray(0.1);
                content.set_line_width(*thickness);
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
        }
    }
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
    content.save_state();
    content.set_stroke_gray(chrome.stroke_gray);
    content.set_fill_gray(chrome.fill_gray);
    content.set_line_width(thickness);
    content.set_line_cap(LineCapStyle::RoundCap);
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
    content.save_state();
    content.set_stroke_gray(chrome.stroke_gray);
    content.set_line_width(thickness);
    content.set_line_cap(LineCapStyle::RoundCap);
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
