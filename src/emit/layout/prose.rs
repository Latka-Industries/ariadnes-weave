//! Heading, quote, code, and list layout.

use crate::error::WeaveError;
use crate::font::{FaceId, FaceRef};
use crate::ir::{BreakHint, InlineStyle, PrintBlock, TextRun};
use crate::knobs::TextAlign;
use crate::profile;

use super::super::types::{
    FaceMode, ForcedBreak, LaidColumns, LaidItem, LaidLine, LaidSpan, LayoutSegment, PaintCategory,
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
    let face = resolve_run_face(
        &marker_run,
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
    let (marker_spans, marker_w) = shape_and_record_spans(
        ctx.fonts,
        face,
        &marker,
        font_size,
        ctx.glyph_sets,
        fill,
        underline,
        None,
        0.0,
    )?;
    // Hanging width = real shaped marker (no inflated floor — that pushed wraps
    // right of the first-line text after a narrower bullet).
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
        marker_spans,
        base_indent,
        marker_w,
        max_width,
    );
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

fn prepend_list_marker(
    out: &mut [LaidItem],
    start: usize,
    item: &crate::ir::ListItem,
    marker_spans: Vec<LaidSpan>,
    base_indent: f32,
    marker_w: f32,
    max_width: f32,
) {
    // Hanging indent: marker on first line at `base_indent`; wraps stay under text
    // at `base_indent + marker_w` (already set via RunLayout.indent).
    if !item.runs.is_empty()
        && let Some(line) = out[start..].iter_mut().find_map(|laid| match laid {
            LaidItem::Text(line) if !line.is_gap() => Some(line),
            _ => None,
        })
    {
        line.spans.splice(0..0, marker_spans);
        line.indent = base_indent;
        line.measure = max_width + marker_w;
    } else if item.runs.is_empty()
        && let Some(LaidItem::Text(line)) = out.get_mut(start)
    {
        line.indent = base_indent;
    }
}

/// N-pane meta row: last pane natural-width flush-end; earlier panes share leftover.
pub(super) fn push_row(
    out: &mut Vec<LaidItem>,
    panes: &[Vec<TextRun>],
    ctx: &mut LayoutCtx,
) -> Result<(), WeaveError> {
    if panes.is_empty() {
        return Ok(());
    }
    let measure = ctx.metrics.content_width();
    let body = ctx.metrics.body_size;
    let leading = ctx.metrics.body_leading;
    let min_w = ctx.knobs.prose.wrap.min_width;
    let gap = 6.0_f32;
    // LaTeX `indentsection` / nested role: org ~14pt, role/degree ~30pt (2×parindent).
    // Indent from the first pane's styles (Tessera THI-387 will make this authored).
    let first = panes[0].as_slice();
    let row_indent = if ctx.metrics.dense_headings {
        let emph = first.iter().any(|r| r.style.emphasis);
        let strong = first.iter().any(|r| r.style.strong);
        if emph && !strong {
            30.0
        } else {
            14.0
        }
    } else {
        0.0
    };

    // Dense CV rows: stay on the 11.5pt baseline grid (LaTeX itemize/indentsection
    // have ~0 extra between org→role→bullets). Entry separation is the list trailer.
    let gap_after = if ctx.metrics.dense_headings {
        0.5
    } else {
        (ctx.knobs.prose.paragraph.gap_after * 0.5).max(2.0)
    };

    let n = panes.len();
    if n == 1 {
        let mut items = Vec::new();
        if !panes[0].is_empty() {
            push_styled_runs(
                &mut items,
                &panes[0],
                ctx,
                row_pane_layout(body, leading, row_indent, (measure - row_indent).max(min_w)),
            )?;
        }
        for mut line in take_content_lines(items) {
            line.indent = row_indent;
            out.push(LaidItem::Text(line));
        }
        out.push(LaidItem::Text(LaidLine::gap(gap_after)));
        return Ok(());
    }

    let last = panes[n - 1].as_slice();
    let last_w = if last.is_empty() {
        0.0
    } else {
        // Slack so italic loc/dates don't soft-wrap from float rounding.
        measure_runs_natural_width(last, ctx, body)? + 3.0
    };
    let flex_n = n - 1;
    let gaps_w = gap * flex_n as f32;
    let flex_budget = (measure - row_indent - last_w - gaps_w).max(min_w * flex_n as f32);
    let each_flex = flex_budget / flex_n as f32;

    let mut col_widths = Vec::with_capacity(n);
    let mut columns = Vec::with_capacity(n);
    for (i, pane) in panes.iter().enumerate() {
        let is_last = i + 1 == n;
        let col_w = if is_last {
            if pane.is_empty() {
                0.0
            } else {
                last_w.max(min_w)
            }
        } else {
            each_flex.max(min_w)
        };
        let mut items = Vec::new();
        if !pane.is_empty() {
            // Column band indent is on LaidColumns; per-line indent stays 0.
            push_styled_runs(
                &mut items,
                pane,
                ctx,
                row_pane_layout(body, leading, 0.0, col_w.max(min_w)),
            )?;
        }
        col_widths.push(col_w);
        columns.push(take_content_lines(items));
    }

    if columns.iter().all(Vec::is_empty) {
        return Ok(());
    }

    // Flush-right only (all leading panes empty).
    if columns[..flex_n].iter().all(Vec::is_empty) {
        for mut line in columns.pop().unwrap_or_default() {
            line.indent = (measure - line.width()).max(0.0);
            out.push(LaidItem::Text(line));
        }
        out.push(LaidItem::Text(LaidLine::gap(gap_after)));
        return Ok(());
    }

    // Leading panes only (empty last): drop the empty trailing column.
    if columns[n - 1].is_empty() && last_w == 0.0 {
        columns.pop();
        col_widths.pop();
        if columns.len() == 1 {
            for mut line in columns.pop().unwrap_or_default() {
                line.indent = row_indent;
                out.push(LaidItem::Text(line));
            }
            out.push(LaidItem::Text(LaidLine::gap(gap_after)));
            return Ok(());
        }
    }

    out.push(LaidItem::Columns(LaidColumns {
        columns,
        col_widths,
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

fn row_pane_layout(font_size: f32, leading: f32, indent: f32, max_width: f32) -> RunLayout {
    RunLayout {
        font_size,
        leading,
        gap_after: 0.0,
        glue_last_content: false,
        mode: FaceMode::Body,
        indent,
        max_width: Some(max_width),
        paint: PaintCategory::Text,
        hard_break_overflow: true,
        text_align: TextAlign::Left,
    }
}
