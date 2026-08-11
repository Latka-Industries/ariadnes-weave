//! Heading, quote, code, and list layout.

use crate::error::WeaveError;
use crate::font::{FaceId, FaceRef};
use crate::ir::{BreakHint, InlineStyle, PrintBlock, TextRun};
use crate::knobs::TextAlign;
use crate::profile;

use super::super::types::{
    FaceMode, ForcedBreak, LaidColumns, LaidItem, LaidLine, LayoutSegment, PaintCategory,
    RunLayout, shape_and_record_spans,
};
use super::LayoutCtx;
use super::block_name;
use super::runs::{
    body_layout, measure_runs_natural_width, push_styled_runs, resolve_run_face, with_knob_italic,
};
use super::segment_has_content;

pub(super) fn layout_heading(
    level: u8,
    runs: &[TextRun],
    break_before: BreakHint,
    ctx: &mut LayoutCtx,
    segments: &mut Vec<LayoutSegment>,
) -> Result<(), WeaveError> {
    // Profile H1 break (manuscript@0) and/or explicit Page / PageAlways.
    let profile_h1_break = ctx.metrics.force_h1_page_break && level == 1;
    if (profile_h1_break || break_before.forces_page_break()) && segment_has_content(segments) {
        segments.push((ForcedBreak::Always, Vec::new()));
    }
    let font_size = profile::heading_size(level, ctx.metrics);
    let glue = matches!(break_before, BreakHint::KeepWithNext) || level <= 2;
    let seg = segments.last_mut().expect("segment");
    push_styled_runs(
        &mut seg.1,
        runs,
        ctx,
        RunLayout {
            font_size,
            leading: font_size * ctx.knobs.prose.heading.leading_factor,
            gap_after: ctx.knobs.prose.heading.gap_after,
            glue_last_content: glue,
            mode: FaceMode::Heading,
            indent: 0.0,
            max_width: None,
            paint: PaintCategory::Text,
            hard_break_overflow: true,
            text_align: TextAlign::Left,
        },
    )
}

pub(super) fn layout_quote(
    runs: &[TextRun],
    ctx: &mut LayoutCtx,
    segments: &mut [LayoutSegment],
) -> Result<(), WeaveError> {
    let seg = segments.last_mut().expect("segment");
    let body = with_knob_italic(runs, ctx.knobs.prose.quote.italic);
    let quoted: Vec<_> = std::iter::once(emphasized_quote_mark())
        .chain(body)
        .chain(std::iter::once(emphasized_quote_mark()))
        .collect();
    push_styled_runs(
        &mut seg.1,
        &quoted,
        ctx,
        body_layout(
            ctx.metrics,
            ctx.knobs,
            ctx.knobs.prose.quote.indent,
            PaintCategory::Quote,
        ),
    )
}

fn emphasized_quote_mark() -> TextRun {
    TextRun {
        text: "\"".into(),
        style: InlineStyle {
            emphasis: true,
            ..InlineStyle::default()
        },
        face: None,
        link_uri: None,
    }
}

pub(super) fn layout_code(
    text: &str,
    ctx: &mut LayoutCtx,
    segments: &mut [LayoutSegment],
) -> Result<(), WeaveError> {
    let seg = segments.last_mut().expect("segment");
    let font_size = ctx.metrics.code_size;
    let leading = font_size * ctx.knobs.prose.code.leading_factor;
    for line in text.lines() {
        seg.1.push(LaidItem::Text(LaidLine::shaped(
            ctx.fonts,
            FaceRef::Bundled(FaceId::MonoRegular),
            line,
            font_size,
            leading,
            ctx.glyph_sets,
            ctx.knobs.prose.text_fill_rgb01(),
        )?));
    }
    seg.1.push(LaidItem::Text(LaidLine::gap(
        ctx.knobs.prose.code.gap_after,
    )));
    Ok(())
}

pub(super) fn push_list_lines(
    out: &mut Vec<LaidItem>,
    ordered: bool,
    items: &[crate::ir::ListItem],
    depth: usize,
    ctx: &mut LayoutCtx,
) -> Result<(), WeaveError> {
    for (i, item) in items.iter().enumerate() {
        push_one_list_item(out, ordered, item, i, depth, ctx)?;
    }
    // Only top-level lists trail into the next CV entry; nested trailers
    // were stacking a fat gap after every sub-bullet group.
    if depth == 0 {
        out.push(LaidItem::Text(LaidLine::gap(
            if ctx.metrics.dense_headings { 2.5 } else { 8.0 },
        )));
    }
    Ok(())
}

fn push_one_list_item(
    out: &mut Vec<LaidItem>,
    ordered: bool,
    item: &crate::ir::ListItem,
    index: usize,
    depth: usize,
    ctx: &mut LayoutCtx,
) -> Result<(), WeaveError> {
    let marker = if ordered {
        format!("{}. ", index + 1)
    } else {
        "• ".into()
    };
    let base_indent = if ctx.metrics.dense_headings {
        // Match nested LaTeX `indentsection`: roles @ ~30pt, bullets flush with roles.
        30.0 + ctx.knobs.prose.list.indent_per_depth * depth as f32
    } else {
        ctx.knobs.prose.list.indent_per_depth * depth as f32
    };
    let font_size = ctx.metrics.body_size;
    let leading = if ctx.metrics.dense_headings {
        // Keep list wraps on the same baseline grid as body prose.
        ctx.metrics.body_leading
    } else {
        font_size * ctx.knobs.prose.list.item_leading_factor
    };
    let marker_run = TextRun::plain(marker.clone());
    let mut marker_w =
        measure_runs_natural_width(std::slice::from_ref(&marker_run), ctx, font_size)?;
    // Liberation bullet+" " can measure tight; keep hang near LaTeX `\textbullet`.
    if ctx.metrics.dense_headings {
        marker_w = marker_w.max(10.0);
    }
    let body_indent = base_indent + marker_w;
    let gutter = ctx.knobs.prose.list.end_gutter.max(0.0);
    let max_width =
        (ctx.metrics.content_width() - body_indent - gutter).max(ctx.knobs.prose.wrap.min_width);

    let start = out.len();
    let body_runs = if item.runs.is_empty() {
        // Marker-only item (rare): still emit the bullet on the first line.
        std::slice::from_ref(&marker_run)
    } else {
        item.runs.as_slice()
    };
    push_styled_runs(
        out,
        body_runs,
        ctx,
        RunLayout {
            font_size,
            leading,
            gap_after: 0.0,
            glue_last_content: false,
            mode: FaceMode::Body,
            indent: body_indent,
            max_width: Some(max_width),
            paint: PaintCategory::Text,
            hard_break_overflow: true,
            text_align: TextAlign::Left,
        },
    )?;
    prepend_list_marker(
        out,
        start,
        item,
        &marker_run,
        &marker,
        base_indent,
        marker_w,
        max_width,
        font_size,
        ctx,
    )?;
    for child in &item.children {
        match child {
            PrintBlock::List {
                ordered: child_ordered,
                items: child_items,
            } => push_list_lines(out, *child_ordered, child_items, depth + 1, ctx)?,
            other => return Err(WeaveError::UnsupportedBlock(block_name(other))),
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn prepend_list_marker(
    out: &mut [LaidItem],
    start: usize,
    item: &crate::ir::ListItem,
    marker_run: &TextRun,
    marker: &str,
    base_indent: f32,
    marker_w: f32,
    max_width: f32,
    font_size: f32,
    ctx: &mut LayoutCtx,
) -> Result<(), WeaveError> {
    // Hanging indent: marker on first line at `base_indent`; wraps stay under text.
    if !item.runs.is_empty()
        && let Some(line) = out[start..].iter_mut().find_map(|laid| match laid {
            LaidItem::Text(line) if !line.is_gap() => Some(line),
            _ => None,
        })
    {
        let face = resolve_run_face(
            marker_run,
            ctx.metrics,
            FaceMode::Body,
            ctx.fonts,
            ctx.knobs,
            PaintCategory::Text,
        )?;
        let (fill, underline) = ctx
            .knobs
            .prose
            .run_paint_rgb01(false, PaintCategory::Text, false);
        let (marker_spans, _) = shape_and_record_spans(
            ctx.fonts,
            face,
            marker,
            font_size,
            ctx.glyph_sets,
            fill,
            underline,
            None,
            0.0,
        )?;
        line.spans.splice(0..0, marker_spans);
        line.indent = base_indent;
        line.measure = max_width + marker_w;
    } else if item.runs.is_empty()
        && let Some(LaidItem::Text(line)) = out.get_mut(start)
    {
        line.indent = base_indent;
    }
    Ok(())
}

/// Left/right meta row: left wraps in leftover measure; right flushes to the end edge.
pub(super) fn push_row(
    out: &mut Vec<LaidItem>,
    left: &[TextRun],
    right: &[TextRun],
    ctx: &mut LayoutCtx,
) -> Result<(), WeaveError> {
    let measure = ctx.metrics.content_width();
    let body = ctx.metrics.body_size;
    let leading = ctx.metrics.body_leading;
    let min_w = ctx.knobs.prose.wrap.min_width;
    let gap = 6.0_f32;
    // LaTeX `indentsection` / nested role: org ~14pt, role/degree ~30pt (2×parindent).
    let row_indent = if ctx.metrics.dense_headings {
        let emph = left.iter().any(|r| r.style.emphasis);
        let strong = left.iter().any(|r| r.style.strong);
        if emph && !strong { 30.0 } else { 14.0 }
    } else {
        0.0
    };
    let right_w = if right.is_empty() {
        0.0
    } else {
        // Slack so italic loc/dates don't soft-wrap from float rounding.
        measure_runs_natural_width(right, ctx, body)? + 3.0
    };
    let left_w = if right_w > 0.0 {
        (measure - row_indent - right_w - gap).max(min_w)
    } else {
        (measure - row_indent).max(min_w)
    };

    let mut left_items = Vec::new();
    if !left.is_empty() {
        push_styled_runs(
            &mut left_items,
            left,
            ctx,
            RunLayout {
                font_size: body,
                leading,
                gap_after: 0.0,
                glue_last_content: false,
                mode: FaceMode::Body,
                indent: row_indent,
                max_width: Some(left_w),
                paint: PaintCategory::Text,
                hard_break_overflow: true,
                text_align: TextAlign::Left,
            },
        )?;
    }
    let mut right_items = Vec::new();
    if !right.is_empty() {
        push_styled_runs(
            &mut right_items,
            right,
            ctx,
            RunLayout {
                font_size: body,
                leading,
                gap_after: 0.0,
                glue_last_content: false,
                mode: FaceMode::Body,
                indent: 0.0,
                max_width: Some(right_w.max(min_w)),
                paint: PaintCategory::Text,
                hard_break_overflow: true,
                text_align: TextAlign::Left,
            },
        )?;
    }

    let left_lines = take_content_lines(left_items);
    let right_lines = take_content_lines(right_items);
    if left_lines.is_empty() && right_lines.is_empty() {
        return Ok(());
    }

    // Dense CV rows: stay on the 11.5pt baseline grid (LaTeX itemize/indentsection
    // have ~0 extra between org→role→bullets). Entry separation is the list trailer.
    let gap_after = if ctx.metrics.dense_headings {
        0.5
    } else {
        (ctx.knobs.prose.paragraph.gap_after * 0.5).max(2.0)
    };
    if right_lines.is_empty() {
        for mut line in left_lines {
            line.indent = row_indent;
            out.push(LaidItem::Text(line));
        }
        out.push(LaidItem::Text(LaidLine::gap(gap_after)));
        return Ok(());
    }
    if left_lines.is_empty() {
        // Flush-right only: indent = leftover after measuring.
        for mut line in right_lines {
            line.indent = (measure - line.width()).max(0.0);
            out.push(LaidItem::Text(line));
        }
        out.push(LaidItem::Text(LaidLine::gap(gap_after)));
        return Ok(());
    }

    out.push(LaidItem::Columns(LaidColumns {
        columns: vec![left_lines, right_lines],
        col_widths: vec![left_w, right_w.max(min_w)],
        gap,
        gap_after,
        indent: row_indent,
    }));
    Ok(())
}

fn take_content_lines(items: Vec<LaidItem>) -> Vec<LaidLine> {
    items
        .into_iter()
        .filter_map(|item| match item {
            LaidItem::Text(line) if !line.is_gap() => Some(line),
            _ => None,
        })
        .collect()
}
