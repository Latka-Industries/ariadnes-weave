//! Deck slide region layout (`title-body`, `two-column`, …).

use crate::error::WeaveError;
use crate::font::{FaceId, FaceRef};
use crate::ir::{InlineStyle, SlideRegionContent, TextRun};
use crate::knobs::LayoutKnobs;

use super::super::types::{
    FaceMode, ForcedBreak, LaidColumns, LaidItem, LaidLine, LayoutSegment, PaintCategory, RunLayout,
};
use super::LayoutCtx;
use super::runs::push_styled_runs;
use super::segment_has_content;

pub(super) fn layout_slide(
    layout_id: &str,
    regions: &[SlideRegionContent],
    ctx: &mut LayoutCtx,
    segments: &mut Vec<LayoutSegment>,
) -> Result<(), WeaveError> {
    if segment_has_content(segments) {
        segments.push((ForcedBreak::Always, Vec::new()));
    }

    let seg = segments.last_mut().expect("segment");
    if ctx.metrics.is_deck {
        seg.1
            .push(LaidItem::Text(LaidLine::gap(ctx.knobs.deck.slide.top_gap)));
    }

    if regions.is_empty() {
        seg.1.push(LaidItem::Text(LaidLine::shaped(
            ctx.fonts,
            FaceRef::Bundled(FaceId::SansItalic),
            "[empty slide]",
            ctx.metrics.body_size,
            ctx.metrics.body_leading,
            ctx.glyph_sets,
            ctx.knobs.prose.text_fill_rgb01(),
        )?));
        segments.push((ForcedBreak::Always, Vec::new()));
        return Ok(());
    }

    match parse_slide_layout(layout_id) {
        SlideLayout::TwoColumn => {
            layout_slide_two_column(&mut seg.1, regions, ctx)?;
        }
        layout @ (SlideLayout::TitleSubtitleBody | SlideLayout::TitleBody) => {
            layout_slide_stacked(
                &mut seg.1,
                regions,
                ctx,
                matches!(layout, SlideLayout::TitleSubtitleBody),
            )?;
        }
    }

    segments.push((ForcedBreak::Always, Vec::new()));
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SlideLayout {
    /// Full-width vertical stack (default / unknown `layout_id`).
    TitleBody,
    /// Same stack, fixed order title → subtitle → body.
    TitleSubtitleBody,
    /// Optional title band + two equal columns (`left` / `right`).
    TwoColumn,
}

fn parse_slide_layout(layout_id: &str) -> SlideLayout {
    match layout_id.trim().to_ascii_lowercase().as_str() {
        "two-column" | "two_column" | "title-two-column" | "title_two_column" => {
            SlideLayout::TwoColumn
        }
        "title-subtitle-body" | "title_subtitle_body" => SlideLayout::TitleSubtitleBody,
        _ => SlideLayout::TitleBody,
    }
}

fn layout_slide_stacked(
    out: &mut Vec<LaidItem>,
    regions: &[SlideRegionContent],
    ctx: &mut LayoutCtx,
    force_order: bool,
) -> Result<(), WeaveError> {
    let titles: Vec<&SlideRegionContent> = regions
        .iter()
        .filter(|r| is_title_slot(&r.slot.to_ascii_lowercase()))
        .collect();
    let rest: Vec<&SlideRegionContent> = if force_order {
        let mut subs = Vec::new();
        let mut bodies = Vec::new();
        let mut other = Vec::new();
        for r in regions {
            let slot = r.slot.to_ascii_lowercase();
            if is_title_slot(&slot) {
                continue;
            }
            if slot_name_is(&slot, "subtitle") {
                subs.push(r);
            } else if is_body_slot(&slot) {
                bodies.push(r);
            } else {
                other.push(r);
            }
        }
        subs.extend(bodies);
        subs.extend(other);
        subs
    } else {
        regions
            .iter()
            .filter(|r| !is_title_slot(&r.slot.to_ascii_lowercase()))
            .collect()
    };

    push_slide_title_regions(out, &titles, ctx)?;
    push_slide_body_regions(out, &rest, ctx)?;
    Ok(())
}

fn layout_slide_two_column(
    out: &mut Vec<LaidItem>,
    regions: &[SlideRegionContent],
    ctx: &mut LayoutCtx,
) -> Result<(), WeaveError> {
    let mut titles = Vec::new();
    let mut subtitles = Vec::new();
    let mut left = Vec::new();
    let mut right = Vec::new();
    for r in regions {
        let slot = r.slot.to_ascii_lowercase();
        if is_title_slot(&slot) {
            titles.push(r);
        } else if slot_name_is(&slot, "subtitle") {
            subtitles.push(r);
        } else if is_left_column_slot(&slot) {
            left.push(r);
        } else if is_right_column_slot(&slot) {
            right.push(r);
        } else {
            // body/content/text (and unknown) → left column by default
            left.push(r);
        }
    }

    push_slide_title_regions(out, &titles, ctx)?;
    if !subtitles.is_empty() {
        push_slide_body_regions(out, &subtitles, ctx)?;
    }

    let gap = if ctx.metrics.is_deck {
        ctx.knobs.deck.columns.gap
    } else {
        ctx.knobs.deck.columns.gap_non_deck
    };
    let content_w = ctx.metrics.content_width();
    let col_w = ((content_w - gap) / 2.0).max(ctx.knobs.deck.columns.min_width);
    let left_lines = wrap_slide_column(&left, col_w, ctx)?;
    let right_lines = wrap_slide_column(&right, col_w, ctx)?;
    out.push(LaidItem::Columns(LaidColumns {
        columns: vec![left_lines, right_lines],
        col_widths: vec![col_w, col_w],
        gap,
        gap_after: if ctx.metrics.is_deck {
            ctx.knobs.deck.columns.gap_after
        } else {
            ctx.knobs.deck.columns.gap_after_non_deck
        },
    }));
    Ok(())
}

fn is_left_column_slot(slot: &str) -> bool {
    slot_name_is(slot, "left")
        || slot_name_is(slot, "col1")
        || slot_name_is(slot, "body-left")
        || slot_name_is(slot, "body_left")
}

fn is_right_column_slot(slot: &str) -> bool {
    slot_name_is(slot, "right")
        || slot_name_is(slot, "col2")
        || slot_name_is(slot, "body-right")
        || slot_name_is(slot, "body_right")
}

fn wrap_slide_column(
    regions: &[&SlideRegionContent],
    col_w: f32,
    ctx: &mut LayoutCtx,
) -> Result<Vec<LaidLine>, WeaveError> {
    let mut items = Vec::new();
    for region in regions {
        push_styled_runs(
            &mut items,
            &[slide_run(region.text.clone(), InlineStyle::default())],
            ctx,
            run_layout_body(
                ctx.metrics.body_size,
                ctx.knobs.deck.columns.region_gap_after,
                Some(col_w),
                ctx.knobs,
            ),
        )?;
    }
    Ok(items
        .into_iter()
        .filter_map(|item| match item {
            LaidItem::Text(line) => Some(line),
            _ => None,
        })
        .collect())
}

fn push_slide_title_regions(
    out: &mut Vec<LaidItem>,
    titles: &[&SlideRegionContent],
    ctx: &mut LayoutCtx,
) -> Result<(), WeaveError> {
    let title_scale = if ctx.metrics.is_deck {
        ctx.knobs.deck.title.scale
    } else {
        ctx.knobs.deck.title.scale_non_deck
    };
    let title_gap = if ctx.metrics.is_deck {
        ctx.knobs.deck.title.gap_after
    } else {
        ctx.knobs.deck.title.gap_after_non_deck
    };
    for region in titles {
        push_styled_runs(
            out,
            &[slide_run(
                region.text.clone(),
                InlineStyle {
                    strong: true,
                    ..InlineStyle::default()
                },
            )],
            ctx,
            run_layout_heading_size(ctx.metrics.body_size * title_scale, title_gap),
        )?;
    }
    Ok(())
}

fn push_slide_body_regions(
    out: &mut Vec<LaidItem>,
    regions: &[&SlideRegionContent],
    ctx: &mut LayoutCtx,
) -> Result<(), WeaveError> {
    for region in regions {
        let slot = region.slot.to_ascii_lowercase();
        let (size, gap, strong) = if slot_name_is(&slot, "subtitle") {
            (
                ctx.metrics.body_size * ctx.knobs.deck.subtitle.size_factor,
                ctx.knobs.deck.subtitle.gap_after,
                false,
            )
        } else {
            (ctx.metrics.body_size, ctx.knobs.deck.body.gap_after, false)
        };
        if !matches!(slot.as_str(), "body" | "content" | "text") && !region.slot.is_empty() {
            push_styled_runs(
                out,
                &[slide_run(
                    format!("{}:", region.slot),
                    InlineStyle {
                        strong: true,
                        ..InlineStyle::default()
                    },
                )],
                ctx,
                RunLayout {
                    font_size: ctx.metrics.body_size * ctx.knobs.deck.body.list_size_factor,
                    leading: ctx.metrics.body_leading,
                    gap_after: ctx.knobs.deck.body.region_gap_after,
                    glue_last_content: true,
                    mode: FaceMode::Body,
                    indent: 0.0,
                    max_width: None,
                    paint: PaintCategory::Text,
                    hard_break_overflow: true,
                    text_align: crate::knobs::FigureAlign::Left,
                },
            )?;
        }
        push_styled_runs(
            out,
            &[slide_run(
                region.text.clone(),
                InlineStyle {
                    strong,
                    emphasis: slot_name_is(&slot, "subtitle"),
                    ..InlineStyle::default()
                },
            )],
            ctx,
            run_layout_body(size, gap, None, ctx.knobs),
        )?;
    }
    Ok(())
}

/// Match a slide slot name exactly, or as a dotted suffix (`main.title` → `title`).
fn slot_name_is(slot: &str, name: &str) -> bool {
    slot == name || slot.rsplit_once('.').is_some_and(|(_, tail)| tail == name)
}

fn is_title_slot(slot: &str) -> bool {
    slot_name_is(slot, "title") || slot_name_is(slot, "heading")
}

fn is_body_slot(slot: &str) -> bool {
    matches!(slot, "body" | "content" | "text") || slot_name_is(slot, "body")
}

fn slide_run(text: impl Into<String>, style: InlineStyle) -> TextRun {
    TextRun {
        text: text.into(),
        style,
        face: None,
    }
}

fn run_layout_body(size: f32, gap: f32, max_width: Option<f32>, knobs: &LayoutKnobs) -> RunLayout {
    RunLayout {
        font_size: size,
        leading: size * knobs.prose.wrap.body_leading_factor,
        gap_after: gap,
        glue_last_content: false,
        mode: FaceMode::Body,
        indent: 0.0,
        max_width,
        paint: PaintCategory::Text,
        hard_break_overflow: true,
        text_align: crate::knobs::FigureAlign::Left,
    }
}

fn run_layout_heading_size(size: f32, gap: f32) -> RunLayout {
    RunLayout {
        font_size: size,
        leading: size * 1.2,
        gap_after: gap,
        glue_last_content: false,
        mode: FaceMode::Heading,
        indent: 0.0,
        max_width: None,
        paint: PaintCategory::Text,
        hard_break_overflow: true,
        text_align: crate::knobs::FigureAlign::Left,
    }
}
