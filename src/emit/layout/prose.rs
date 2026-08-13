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
    dest_id: Option<&str>,
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
    let start = seg.1.len();
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
    )?;
    if let Some(id) = dest_id {
        // Tag the first content line so pagination can map dest → page.
        for item in &mut seg.1[start..] {
            if let LaidItem::Text(line) = item
                && !line.is_gap()
            {
                line.dest_id = Some(id.to_owned());
                break;
            }
        }
    }
    Ok(())
}

/// TOC line: nested title + optional dotted leaders; page digits flush-right.
///
/// Nesting uses list `indent_per_depth` (not `[indent].step`, often `0`) on the
/// **title** only. The page column stays on the content right edge for every
/// level (classic TOC).
pub(super) fn push_toc_entry(
    out: &mut Vec<LaidItem>,
    title: &[TextRun],
    page_label: Option<&str>,
    dest_id: Option<&str>,
    indent: u32,
    leaders: bool,
    ctx: &mut LayoutCtx,
) -> Result<(), WeaveError> {
    let nest_step = ctx.knobs.prose.list.indent_per_depth.max(12.0);
    #[allow(clippy::cast_precision_loss)]
    let nest = nest_step * indent as f32;
    let font_size = ctx.metrics.body_size;
    let leading = ctx.metrics.body_leading;
    let full = ctx.metrics.content_width();
    let fill = ctx.knobs.prose.text_fill_rgb01();
    let link_dest = dest_id;
    let face = FaceRef::Bundled(FaceId::SansRegular);
    let col_gap = 6.0_f32;

    let mut title_spans: Vec<LaidSpan> = Vec::new();
    let mut title_w = 0.0_f32;
    for run in title {
        let run_face = resolve_run_face(
            run,
            ctx.metrics,
            FaceMode::Body,
            ctx.fonts,
            ctx.knobs,
            PaintCategory::Text,
        )?;
        let (run_spans, w) = shape_and_record_spans(
            ctx.fonts,
            run_face,
            &run.text,
            font_size,
            ctx.glyph_sets,
            fill,
            false,
            None,
            link_dest,
            0.0,
        )?;
        title_spans.extend(run_spans);
        title_w += w;
    }

    // Known label, or "0" placeholder when dest will resolve after a layout pass.
    let page_text = page_label
        .map(str::to_owned)
        .or_else(|| dest_id.map(|_| "0".into()))
        .unwrap_or_default();

    let mut page_spans: Vec<LaidSpan> = Vec::new();
    let mut page_w = 0.0_f32;
    if !page_text.is_empty() {
        let (ps, w) = shape_and_record_spans(
            ctx.fonts,
            face,
            &page_text,
            font_size,
            ctx.glyph_sets,
            fill,
            false,
            None,
            link_dest,
            0.0,
        )?;
        page_spans = ps;
        page_w = w;
    }

    // Fixed page column (≥ two digits) so "2" and "12" share a right edge.
    let page_slot = if page_w > 0.0 {
        let (_, two_w) = shape_and_record_spans(
            ctx.fonts,
            face,
            "88",
            font_size,
            ctx.glyph_sets,
            fill,
            false,
            None,
            None,
            0.0,
        )?;
        page_w.max(two_w)
    } else {
        0.0
    };

    // Full-width band: nest only the title; page column stays on the outer edge.
    let left_w = if page_slot > 0.0 {
        (full - col_gap - page_slot).max(ctx.knobs.prose.wrap.min_width)
    } else {
        full
    };
    let title_measure = (left_w - nest).max(ctx.knobs.prose.wrap.min_width);

    let mut left_spans = title_spans;
    let gap = (title_measure - title_w).max(0.0);
    if gap > 0.0 {
        let fill_char = if leaders { "." } else { " " };
        let (unit_spans, unit_w) = shape_and_record_spans(
            ctx.fonts,
            face,
            fill_char,
            font_size,
            ctx.glyph_sets,
            fill,
            false,
            None,
            None,
            0.0,
        )?;
        if unit_w > 0.0 {
            let n = (gap / unit_w).floor() as usize;
            for _ in 0..n {
                left_spans.extend(unit_spans.iter().cloned());
            }
        }
    }

    let left_line = LaidLine {
        spans: left_spans,
        leading,
        glue_after: false,
        indent: nest,
        measure: title_measure,
        text_align: TextAlign::Left,
        dest_id: None,
    };

    if page_slot <= 0.0 {
        out.push(LaidItem::Text(left_line));
        out.push(LaidItem::Text(LaidLine::gap(
            ctx.knobs.prose.paragraph.gap_after,
        )));
        return Ok(());
    }

    // Page digits only — column paint right-aligns within `page_slot`.
    let right_line = LaidLine {
        spans: page_spans,
        leading,
        glue_after: false,
        indent: 0.0,
        measure: page_slot,
        text_align: TextAlign::Right,
        dest_id: None,
    };

    out.push(LaidItem::Columns(LaidColumns {
        columns: vec![vec![left_line], vec![right_line]],
        col_widths: vec![left_w, page_slot],
        gap: col_gap,
        gap_after: ctx.knobs.prose.paragraph.gap_after,
        indent: 0.0,
    }));
    Ok(())
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
    band_indent: u32,
    depth: usize,
    ctx: &mut LayoutCtx,
) -> Result<(), WeaveError> {
    for (i, item) in items.iter().enumerate() {
        push_one_list_item(out, ordered, item, i, band_indent, depth, ctx)?;
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
    band_indent: u32,
    depth: usize,
    ctx: &mut LayoutCtx,
) -> Result<(), WeaveError> {
    let marker = if ordered {
        format!("{}. ", index + 1)
    } else {
        "• ".into()
    };
    #[allow(clippy::cast_precision_loss)]
    let nest = ctx.knobs.prose.list.indent_per_depth * depth as f32;
    let base_indent = ctx.knobs.prose.indent.pts(band_indent) + nest;
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
                indent: child_indent,
            } => {
                // Nested lists keep the parent's band unless explicitly set.
                let band = if *child_indent > 0 {
                    *child_indent
                } else {
                    band_indent
                };
                push_list_lines(out, *child_ordered, child_items, band, depth + 1, ctx)?;
            }
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
    indent_level: u32,
    ctx: &mut LayoutCtx,
) -> Result<(), WeaveError> {
    if panes.is_empty() {
        return Ok(());
    }
    let gap_after = row_gap_after(ctx);
    let geom = RowGeom {
        measure: ctx.metrics.content_width(),
        body: ctx.metrics.body_size,
        leading: ctx.metrics.body_leading,
        min_w: ctx.knobs.prose.wrap.min_width,
        gap: 6.0,
        row_indent: ctx.knobs.prose.indent.pts(indent_level),
        gap_after,
    };

    if panes.len() == 1 {
        return push_single_pane_row(out, &panes[0], ctx, &geom);
    }

    let laid = layout_row_columns(panes, ctx, &geom)?;
    if laid.columns.iter().all(Vec::is_empty) {
        return Ok(());
    }

    let flex_n = panes.len() - 1;
    // Flush-right only (all leading panes empty).
    if laid.columns[..flex_n].iter().all(Vec::is_empty) {
        let mut columns = laid.columns;
        push_flush_right_lines(
            out,
            columns.pop().unwrap_or_default(),
            geom.measure,
            geom.gap_after,
        );
        return Ok(());
    }

    let LaidRowColumns {
        mut columns,
        mut col_widths,
        last_w,
    } = laid;
    // Leading panes only (empty last): drop the empty trailing column.
    if columns[panes.len() - 1].is_empty() && last_w == 0.0 {
        columns.pop();
        col_widths.pop();
        if columns.len() == 1 {
            push_indented_lines(
                out,
                columns.pop().unwrap_or_default(),
                geom.row_indent,
                geom.gap_after,
            );
            return Ok(());
        }
    }

    out.push(LaidItem::Columns(LaidColumns {
        columns,
        col_widths,
        gap: geom.gap,
        gap_after: geom.gap_after,
        indent: geom.row_indent,
    }));
    Ok(())
}

struct RowGeom {
    measure: f32,
    body: f32,
    leading: f32,
    min_w: f32,
    gap: f32,
    row_indent: f32,
    gap_after: f32,
}

struct LaidRowColumns {
    columns: Vec<Vec<LaidLine>>,
    col_widths: Vec<f32>,
    last_w: f32,
}

/// Dense CV rows stay on the baseline grid; entry separation is the list trailer.
fn row_gap_after(ctx: &LayoutCtx) -> f32 {
    if ctx.metrics.dense_headings {
        0.5
    } else {
        (ctx.knobs.prose.paragraph.gap_after * 0.5).max(2.0)
    }
}

fn push_single_pane_row(
    out: &mut Vec<LaidItem>,
    pane: &[TextRun],
    ctx: &mut LayoutCtx,
    geom: &RowGeom,
) -> Result<(), WeaveError> {
    let mut items = Vec::new();
    if !pane.is_empty() {
        let max_w = (geom.measure - geom.row_indent).max(geom.min_w);
        push_styled_runs(
            &mut items,
            pane,
            ctx,
            row_pane_layout(geom.body, geom.leading, geom.row_indent, max_w),
        )?;
    }
    push_indented_lines(
        out,
        take_content_lines(items),
        geom.row_indent,
        geom.gap_after,
    );
    Ok(())
}

fn layout_row_columns(
    panes: &[Vec<TextRun>],
    ctx: &mut LayoutCtx,
    geom: &RowGeom,
) -> Result<LaidRowColumns, WeaveError> {
    let n = panes.len();
    let last = panes[n - 1].as_slice();
    let last_w = if last.is_empty() {
        0.0
    } else {
        // Slack so italic loc/dates don't soft-wrap from float rounding.
        measure_runs_natural_width(last, ctx, geom.body)? + 3.0
    };
    let flex_n = n - 1;
    // Pane counts are tiny; f32 is exact for these values.
    #[allow(clippy::cast_precision_loss)]
    let flex_n_f = flex_n as f32;
    let gaps_w = geom.gap * flex_n_f;
    let flex_budget = (geom.measure - geom.row_indent - last_w - gaps_w).max(geom.min_w * flex_n_f);
    let each_flex = flex_budget / flex_n_f;

    let mut col_widths = Vec::with_capacity(n);
    let mut columns = Vec::with_capacity(n);
    for (i, pane) in panes.iter().enumerate() {
        let is_last = i + 1 == n;
        let col_w = if is_last {
            if pane.is_empty() {
                0.0
            } else {
                last_w.max(geom.min_w)
            }
        } else {
            each_flex.max(geom.min_w)
        };
        let mut items = Vec::new();
        if !pane.is_empty() {
            // Column band indent is on LaidColumns; per-line indent stays 0.
            push_styled_runs(
                &mut items,
                pane,
                ctx,
                row_pane_layout(geom.body, geom.leading, 0.0, col_w.max(geom.min_w)),
            )?;
        }
        col_widths.push(col_w);
        columns.push(take_content_lines(items));
    }
    Ok(LaidRowColumns {
        columns,
        col_widths,
        last_w,
    })
}

fn push_indented_lines(out: &mut Vec<LaidItem>, lines: Vec<LaidLine>, indent: f32, gap_after: f32) {
    for mut line in lines {
        line.indent = indent;
        out.push(LaidItem::Text(line));
    }
    out.push(LaidItem::Text(LaidLine::gap(gap_after)));
}

fn push_flush_right_lines(
    out: &mut Vec<LaidItem>,
    lines: Vec<LaidLine>,
    measure: f32,
    gap_after: f32,
) {
    for mut line in lines {
        line.indent = (measure - line.width()).max(0.0);
        out.push(LaidItem::Text(line));
    }
    out.push(LaidItem::Text(LaidLine::gap(gap_after)));
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
