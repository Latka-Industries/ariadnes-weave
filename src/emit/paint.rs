//! Build PDF content streams from laid items (text, images, tables).

use pdf_writer::{Content, Name, Str};

use crate::error::WeaveError;
use crate::font::{FaceId, FaceRef, FontBag, encode_gids, shape_text, shaped_width};
use crate::profile::ProfileMetrics;

use super::types::{LaidItem, LaidMath, LaidMathEl, LaidTable, SubsetMap};

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
) -> Result<Vec<u8>, WeaveError> {
    let mut content = Content::new();
    let mut y = metrics.page_h - metrics.margin;

    for item in items {
        match item {
            LaidItem::Text(line) => {
                y -= line.leading;
                if y < metrics.margin + 18.0 {
                    break;
                }
                if line.spans.is_empty() {
                    continue;
                }
                content.begin_text();
                let mut x = if line.center {
                    metrics.margin + (metrics.content_width() - line.width()) / 2.0
                } else {
                    metrics.margin + line.indent
                };
                for span in &line.spans {
                    let face_name = fonts.resource_name(span.face);
                    content.set_font(Name(&face_name), span.font_size);
                    content.set_text_matrix([1.0, 0.0, 0.0, 1.0, x, y]);
                    let encoded = encode_gids(&span.glyphs);
                    content.show(Str(&encoded));
                    x += shaped_width(&span.glyphs);
                }
                content.end_text();
            }
            LaidItem::Image {
                img_idx,
                width,
                height,
                glue_after: _,
            } => {
                y -= *height;
                if y < metrics.margin + 18.0 {
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
                if y - table_h < metrics.margin + 18.0 {
                    break;
                }
                paint_table(&mut content, table, metrics.margin, y, fonts);
                y -= table_h + table.gap_after;
            }
            LaidItem::Math(math) => {
                if y - math.height < metrics.margin + 18.0 {
                    break;
                }
                paint_math(
                    &mut content,
                    math,
                    metrics.margin,
                    y,
                    metrics.content_width(),
                    fonts,
                );
                y -= math.height + math.gap_after;
            }
        }
    }

    let footer = format!("{page_no} / {page_count}");
    let footer_face = FaceRef::Bundled(FaceId::SansRegular);
    let mut footer_glyphs = shape_text(fonts, footer_face, &footer, 9.0)?;
    if let Some(subset) = subsets.get(&footer_face) {
        for g in &mut footer_glyphs {
            *g = subset.remap_glyph(*g);
        }
    }
    let footer_w = shaped_width(&footer_glyphs);
    let footer_y = metrics.margin * 0.45;
    let footer_x = (metrics.page_w - footer_w) / 2.0;
    let footer_name = fonts.resource_name(footer_face);
    content.begin_text();
    content.set_font(Name(&footer_name), 9.0);
    content.set_text_matrix([1.0, 0.0, 0.0, 1.0, footer_x, footer_y]);
    content.show(Str(&encode_gids(&footer_glyphs)));
    content.end_text();

    Ok(content.finish().into_vec())
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
                if line.spans.is_empty() {
                    continue;
                }
                content.begin_text();
                let mut span_x = cell_x + table.pad;
                for span in &line.spans {
                    let face_name = fonts.resource_name(span.face);
                    content.set_font(Name(&face_name), span.font_size);
                    content.set_text_matrix([1.0, 0.0, 0.0, 1.0, span_x, text_y]);
                    content.show(Str(&encode_gids(&span.glyphs)));
                    span_x += shaped_width(&span.glyphs);
                }
                content.end_text();
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
        }
    }
}
