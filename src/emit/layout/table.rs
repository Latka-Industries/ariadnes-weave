//! Table grid layout with wrapped cell text.

use crate::error::WeaveError;
use crate::font::{FaceId, FaceRef};
use crate::ir::TableRow;

use super::super::types::{
    LaidItem, LaidLine, LaidSpan, LaidTable, LaidTableRow, shape_and_record_spans,
};
use super::LayoutCtx;
use super::runs::{hard_break_text, next_wrap_chunk, skip_wrap_chunk_at_line_start};

pub(super) fn push_table(
    out: &mut Vec<LaidItem>,
    rows: &[TableRow],
    ctx: &mut LayoutCtx,
) -> Result<(), WeaveError> {
    if rows.is_empty() {
        out.push(LaidItem::Text(LaidLine::shaped(
            ctx.fonts,
            FaceRef::Bundled(FaceId::SansItalic),
            "[table]",
            ctx.metrics.body_size,
            ctx.metrics.body_leading,
            ctx.glyph_sets,
            ctx.knobs.prose.text_fill_rgb01(),
        )?));
        out.push(LaidItem::Text(LaidLine::gap(
            ctx.knobs.prose.paragraph.gap_after,
        )));
        return Ok(());
    }

    let cols = rows.iter().map(|r| r.cells.len()).max().unwrap_or(0).max(1);
    let pad = ctx.knobs.table.cell.pad;
    let font_size = ctx.metrics.body_size;
    let leading = ctx
        .metrics
        .body_leading
        .min(font_size * ctx.knobs.table.cell.leading_factor);
    let face = FaceRef::Bundled(if ctx.metrics.serif_body {
        FaceId::SerifRegular
    } else {
        FaceId::SansRegular
    });
    let header_face = FaceRef::Bundled(if ctx.metrics.serif_body {
        FaceId::SerifBold
    } else {
        FaceId::SansBold
    });
    let col_width = ctx.metrics.content_width() / cols as f32;
    let inner_width = (col_width - pad * 2.0).max(ctx.knobs.table.cell.min_inner_width);
    let col_widths = vec![col_width; cols];

    let mut laid_rows = Vec::with_capacity(rows.len());
    for (row_idx, row) in rows.iter().enumerate() {
        let mut cells = Vec::with_capacity(cols);
        let mut row_h = pad * 2.0 + leading;
        for col in 0..cols {
            let text = row.cells.get(col).map_or("", String::as_str);
            let cell_face = if row_idx == 0 { header_face } else { face };
            let lines = wrap_plain_text(text, cell_face, font_size, leading, inner_width, ctx)?;
            let content_h = if lines.is_empty() {
                leading
            } else {
                lines.iter().map(|l| l.leading).sum::<f32>()
            };
            row_h = row_h.max(pad * 2.0 + content_h);
            cells.push(lines);
        }
        laid_rows.push(LaidTableRow {
            height: row_h,
            cells,
        });
    }

    out.push(LaidItem::Table(LaidTable {
        col_widths,
        rows: laid_rows,
        pad,
        gap_after: ctx.knobs.table.block.gap_after,
    }));
    Ok(())
}

fn wrap_plain_text(
    text: &str,
    face: FaceRef,
    font_size: f32,
    leading: f32,
    max_width: f32,
    ctx: &mut LayoutCtx,
) -> Result<Vec<LaidLine>, WeaveError> {
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let mut lines = Vec::new();
    let mut current: Vec<LaidSpan> = Vec::new();
    let mut current_width = 0.0_f32;
    let mut remaining = text;
    while !remaining.is_empty() {
        let (chunk, rest) = next_wrap_chunk(remaining);
        remaining = rest;
        // Keep trailing spaces so inter-word advances are shaped; drop
        // whitespace-only chunks at the start of a line.
        if current.is_empty() && skip_wrap_chunk_at_line_start(chunk) {
            continue;
        }
        let fill = ctx.knobs.prose.text_fill_rgb01();
        let (spans, w) = shape_and_record_spans(
            ctx.fonts,
            face,
            chunk,
            font_size,
            ctx.glyph_sets,
            fill,
            false,
            None,
            0.0,
        )?;
        if current_width + w > max_width && !current.is_empty() {
            lines.push(LaidLine::wrapped(
                std::mem::take(&mut current),
                leading,
                max_width,
            ));
            current_width = 0.0;
            if skip_wrap_chunk_at_line_start(chunk) {
                continue;
            }
        }
        if w > max_width && current.is_empty() {
            for piece in hard_break_text(ctx.fonts, face, chunk, font_size, max_width)? {
                let (spans, _) = shape_and_record_spans(
                    ctx.fonts,
                    face,
                    &piece,
                    font_size,
                    ctx.glyph_sets,
                    fill,
                    false,
                    None,
                    0.0,
                )?;
                lines.push(LaidLine::wrapped(spans, leading, max_width));
            }
            continue;
        }
        current.extend(spans);
        current_width += w;
    }
    if !current.is_empty() {
        lines.push(LaidLine::wrapped(current, leading, max_width));
    }
    Ok(lines)
}
