//! Heading, quote, code, and list layout.

use crate::error::WeaveError;
use crate::font::{FaceId, FaceRef};
use crate::ir::{BreakHint, InlineStyle, PrintBlock, TextRun};
use crate::profile;

use super::super::types::{
    FaceMode, ForcedBreak, LaidItem, LaidLine, LayoutSegment, PaintCategory, RunLayout,
};
use super::LayoutCtx;
use super::block_name;
use super::runs::{body_layout, push_styled_runs, with_knob_italic};
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
        let marker = if ordered {
            format!("{}. ", i + 1)
        } else {
            "• ".into()
        };
        let mut runs = vec![TextRun::plain(marker)];
        runs.extend(item.runs.iter().cloned());
        push_styled_runs(
            out,
            &runs,
            ctx,
            RunLayout {
                font_size: ctx.metrics.body_size,
                leading: ctx.metrics.body_size * ctx.knobs.prose.list.item_leading_factor,
                gap_after: 0.0,
                glue_last_content: false,
                mode: FaceMode::Body,
                indent: ctx.knobs.prose.list.indent_per_depth * depth as f32,
                max_width: None,
                paint: PaintCategory::Text,
            },
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
    }
    out.push(LaidItem::Text(LaidLine::gap(8.0)));
    Ok(())
}
