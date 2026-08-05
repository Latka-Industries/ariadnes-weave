//! Turn print IR blocks into laid segments, images, and glyph sets.

use crate::error::WeaveError;
use crate::font::{FaceId, FaceRef, FontBag, shape_text_with_fallback, shaped_runs_width};
use crate::image_prep::{PreparedImage, prepare_image};
use crate::ir::{
    BreakHint, FigurePlacement, InlineStyle, PrintBlock, PrintDocument, PrintImage,
    SlideRegionContent, TableRow, TextRun,
};
use crate::knobs::LayoutKnobs;
use crate::profile::{self, ProfileMetrics};

use super::math::layout_math;
use super::types::{
    FaceMode, ForcedBreak, GlyphSets, LaidColumns, LaidItem, LaidLine, LaidSpan, LaidTable,
    LaidTableRow, LayoutDoc, LayoutSegment, PaintCategory, RunLayout, shape_and_record_spans,
};

pub(super) struct LayoutCtx<'a> {
    metrics: &'a ProfileMetrics,
    fonts: &'a FontBag,
    knobs: &'a LayoutKnobs,
    glyph_sets: &'a mut GlyphSets,
}

fn layout_ctx<'a>(
    metrics: &'a ProfileMetrics,
    fonts: &'a FontBag,
    knobs: &'a LayoutKnobs,
    glyph_sets: &'a mut GlyphSets,
) -> LayoutCtx<'a> {
    LayoutCtx {
        metrics,
        fonts,
        knobs,
        glyph_sets,
    }
}

/// Walk document blocks into layout segments (reading order + break hints).
pub(super) fn collect_layout(
    doc: &PrintDocument,
    metrics: &ProfileMetrics,
    fonts: &FontBag,
    knobs: &LayoutKnobs,
) -> Result<LayoutDoc, WeaveError> {
    let mut segments: Vec<(ForcedBreak, Vec<LaidItem>)> = vec![(ForcedBreak::None, Vec::new())];
    let mut images: Vec<PreparedImage> = Vec::new();
    let mut glyph_sets: GlyphSets = std::collections::BTreeMap::new();

    for block in &doc.blocks {
        layout_block(
            block,
            metrics,
            fonts,
            knobs,
            &mut segments,
            &mut images,
            &mut glyph_sets,
        )?;
    }

    Ok((segments, images, glyph_sets))
}

pub(super) fn layout_block(
    block: &PrintBlock,
    metrics: &ProfileMetrics,
    fonts: &FontBag,
    knobs: &LayoutKnobs,
    segments: &mut Vec<LayoutSegment>,
    images: &mut Vec<PreparedImage>,
    glyph_sets: &mut GlyphSets,
) -> Result<(), WeaveError> {
    match block {
        PrintBlock::Break(hint) => {
            if hint.forces_page_break() {
                segments.push((ForcedBreak::Always, Vec::new()));
            }
        }
        PrintBlock::Heading {
            level,
            runs,
            break_before,
        } => {
            let mut ctx = layout_ctx(metrics, fonts, knobs, glyph_sets);
            layout_heading(*level, runs, *break_before, &mut ctx, segments)?;
        }
        PrintBlock::Paragraph { runs } => {
            let mut ctx = layout_ctx(metrics, fonts, knobs, glyph_sets);
            let seg = segments.last_mut().expect("segment");
            push_styled_runs(
                &mut seg.1,
                runs,
                &mut ctx,
                body_layout(metrics, knobs, 0.0, PaintCategory::Text),
            )?;
        }
        PrintBlock::Quote { runs } => {
            let mut ctx = layout_ctx(metrics, fonts, knobs, glyph_sets);
            layout_quote(runs, &mut ctx, segments)?;
        }
        PrintBlock::Code { lang: _, text } => {
            let mut ctx = layout_ctx(metrics, fonts, knobs, glyph_sets);
            layout_code(text, &mut ctx, segments)?;
        }
        PrintBlock::List { ordered, items } => {
            let mut ctx = layout_ctx(metrics, fonts, knobs, glyph_sets);
            let seg = segments.last_mut().expect("segment");
            push_list_lines(&mut seg.1, *ordered, items, 0, &mut ctx)?;
        }
        PrintBlock::Table { rows } => {
            let mut ctx = layout_ctx(metrics, fonts, knobs, glyph_sets);
            let seg = segments.last_mut().expect("segment");
            push_table(&mut seg.1, rows, &mut ctx)?;
        }
        PrintBlock::Figure {
            image,
            alt,
            caption,
            placement,
        } => {
            PushFigureArgs {
                segments,
                images,
                image,
                alt,
                caption,
                placement: *placement,
                metrics,
                fonts,
                knobs,
                glyph_sets,
            }
            .run()?;
        }
        PrintBlock::Math { display, latex } => {
            layout_math(
                *display,
                latex,
                metrics,
                fonts,
                &knobs.math,
                segments,
                glyph_sets,
            )?;
        }
        PrintBlock::Slide { layout_id, regions } => {
            let mut ctx = layout_ctx(metrics, fonts, knobs, glyph_sets);
            layout_slide(layout_id, regions, &mut ctx, segments)?;
        }
    }
    Ok(())
}

fn segment_has_content(segments: &[LayoutSegment]) -> bool {
    segments.last().is_some_and(|(_, items)| !items.is_empty())
}

fn body_layout(
    metrics: &ProfileMetrics,
    knobs: &LayoutKnobs,
    indent: f32,
    paint: PaintCategory,
) -> RunLayout {
    RunLayout {
        font_size: metrics.body_size,
        leading: metrics.body_leading,
        gap_after: knobs.prose.paragraph.gap_after,
        glue_last_content: false,
        mode: FaceMode::Body,
        indent,
        max_width: None,
        paint,
    }
}

fn layout_heading(
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

fn layout_quote(
    runs: &[TextRun],
    ctx: &mut LayoutCtx,
    segments: &mut [LayoutSegment],
) -> Result<(), WeaveError> {
    let seg = segments.last_mut().expect("segment");
    let body_italic = ctx.knobs.prose.quote.italic;
    let body = runs.iter().cloned().map(|mut run| {
        run.style.emphasis |= body_italic;
        run
    });
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

fn layout_code(
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

fn resolve_face(style: InlineStyle, metrics: &ProfileMetrics, mode: FaceMode) -> FaceRef {
    let id = match mode {
        FaceMode::Heading => {
            let mut s = style;
            if !s.code && !s.strong && !s.emphasis {
                s.strong = true;
            }
            FaceId::from_style(&s, false)
        }
        FaceMode::Body => FaceId::from_style(&style, metrics.serif_body),
    };
    FaceRef::Bundled(id)
}

fn resolve_run_face(
    run: &TextRun,
    metrics: &ProfileMetrics,
    mode: FaceMode,
    fonts: &FontBag,
) -> Result<FaceRef, WeaveError> {
    if let Some(id) = &run.face {
        #[cfg(feature = "os-fonts")]
        let os_key = crate::os_fonts::os_pin_key(id, run.style);
        #[cfg(feature = "os-fonts")]
        if let Some(face) = fonts.resolve_pin(&os_key) {
            return Ok(face);
        }
        if let Some(face) = fonts.resolve_pin(id) {
            return Ok(face);
        }
        return match fonts.resolve_mode() {
            crate::options::FontResolveMode::BundledOnly => {
                Err(WeaveError::Font(format!("unknown pinned face `{id}`")))
            }
            crate::options::FontResolveMode::OsWithFallback => {
                // Missing OS face → sealed Liberation for this run's style.
                Ok(resolve_face(run.style, metrics, mode))
            }
        };
    }
    Ok(resolve_face(run.style, metrics, mode))
}

/// Inputs for laying out a [`PrintBlock::Figure`].
pub(super) struct PushFigureArgs<'a> {
    pub segments: &'a mut [LayoutSegment],
    pub images: &'a mut Vec<PreparedImage>,
    pub image: &'a PrintImage,
    pub alt: &'a str,
    pub caption: &'a [TextRun],
    pub placement: FigurePlacement,
    pub metrics: &'a ProfileMetrics,
    pub fonts: &'a FontBag,
    pub knobs: &'a LayoutKnobs,
    pub glyph_sets: &'a mut GlyphSets,
}

impl PushFigureArgs<'_> {
    /// Decode/fit the image (or alt placeholder), then append caption lines.
    ///
    /// [`FigurePlacement::FloatNear`] glues the figure to the preceding content
    /// and to its caption so pagination prefers to keep them together.
    pub(super) fn run(self) -> Result<(), WeaveError> {
        let Self {
            segments,
            images,
            image,
            alt,
            caption,
            placement,
            metrics,
            fonts,
            knobs,
            glyph_sets,
        } = self;
        let float_near = matches!(placement, FigurePlacement::FloatNear);
        let seg = segments.last_mut().expect("segment");
        if float_near {
            // Keep the figure with the preceding block when possible.
            if let Some(prev) = seg
                .1
                .iter_mut()
                .rev()
                .find(|item| !matches!(item, LaidItem::Text(line) if line.spans.is_empty()))
            {
                prev.set_glue_after(true);
            }
        }

        let Ok(prepared) = prepare_image(image) else {
            let label = if alt.is_empty() {
                "[figure]".into()
            } else {
                format!("[figure: {alt}]")
            };
            let mut line = LaidLine::shaped(
                fonts,
                FaceRef::Bundled(FaceId::SansItalic),
                &label,
                metrics.body_size,
                metrics.body_leading,
                glyph_sets,
                knobs.prose.text_fill_rgb01(),
            )?;
            line.glue_after = !caption.is_empty() || float_near;
            seg.1.push(LaidItem::Text(line));
            if caption.is_empty() {
                seg.1.push(LaidItem::Text(LaidLine::gap(
                    knobs.prose.figure.alt_gap_after,
                )));
            } else {
                push_styled_runs(
                    &mut seg.1,
                    caption,
                    &mut layout_ctx(metrics, fonts, knobs, glyph_sets),
                    body_layout(metrics, knobs, 0.0, PaintCategory::Text),
                )?;
            }
            return Ok(());
        };

        let (w, h) = prepared.fit_width(metrics.content_width());
        let img_idx = images.len();
        images.push(prepared);

        seg.1.push(LaidItem::Image {
            img_idx,
            width: w,
            height: h,
            glue_after: !caption.is_empty() || float_near,
        });
        if caption.is_empty() {
            seg.1
                .push(LaidItem::Text(LaidLine::gap(knobs.prose.figure.gap_after)));
        } else {
            push_styled_runs(
                &mut seg.1,
                caption,
                &mut layout_ctx(metrics, fonts, knobs, glyph_sets),
                body_layout(metrics, knobs, 0.0, PaintCategory::Text),
            )?;
        }
        Ok(())
    }
}

fn push_table(
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
        )?;
        if current_width + w > max_width && !current.is_empty() {
            lines.push(LaidLine {
                spans: std::mem::take(&mut current),
                leading,
                glue_after: false,
                indent: 0.0,
                center: false,
            });
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
                )?;
                lines.push(LaidLine {
                    spans,
                    leading,
                    glue_after: false,
                    indent: 0.0,
                    center: false,
                });
            }
            continue;
        }
        current.extend(spans);
        current_width += w;
    }
    if !current.is_empty() {
        lines.push(LaidLine {
            spans: current,
            leading,
            glue_after: false,
            indent: 0.0,
            center: false,
        });
    }
    Ok(lines)
}

fn layout_slide(
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
    }
}

/// Wrap styled runs into lines, then apply widow/orphan glue and optional gap.
pub(super) fn push_styled_runs(
    out: &mut Vec<LaidItem>,
    runs: &[TextRun],
    ctx: &mut LayoutCtx,
    layout: RunLayout,
) -> Result<(), WeaveError> {
    if runs.is_empty() {
        return Ok(());
    }

    let start = out.len();
    let max_width = layout
        .max_width
        .unwrap_or_else(|| {
            (ctx.metrics.content_width() - layout.indent).max(ctx.knobs.prose.wrap.min_width)
        })
        .max(ctx.knobs.prose.wrap.min_width);
    let mut current_spans: Vec<LaidSpan> = Vec::new();
    let mut current_width = 0.0_f32;

    let flush_line = |spans: &mut Vec<LaidSpan>, dest: &mut Vec<LaidItem>, glue: bool| {
        if spans.is_empty() {
            return;
        }
        dest.push(LaidItem::Text(LaidLine {
            spans: std::mem::take(spans),
            leading: layout.leading,
            glue_after: glue,
            indent: layout.indent,
            center: false,
        }));
    };

    for run in runs {
        let face = resolve_run_face(run, ctx.metrics, layout.mode, ctx.fonts)?;
        let (fill, underline) = ctx
            .knobs
            .prose
            .run_paint_rgb01(run.style.cite, layout.paint.is_quote());
        let mut remaining = run.text.as_str();
        while !remaining.is_empty() {
            let (chunk, rest) = next_wrap_chunk(remaining);
            remaining = rest;
            // Keep trailing spaces so inter-word advances are shaped; drop
            // whitespace-only chunks at the start of a line.
            if current_spans.is_empty() && skip_wrap_chunk_at_line_start(chunk) {
                continue;
            }
            let (spans, w) = shape_and_record_spans(
                ctx.fonts,
                face,
                chunk,
                layout.font_size,
                ctx.glyph_sets,
                fill,
                underline,
            )?;
            if current_width + w > max_width && !current_spans.is_empty() {
                flush_line(&mut current_spans, out, false);
                current_width = 0.0;
                if skip_wrap_chunk_at_line_start(chunk) {
                    continue;
                }
            }
            if w > max_width && current_spans.is_empty() {
                // Hard-break tokens wider than the content box (URLs, long code).
                for piece in hard_break_text(ctx.fonts, face, chunk, layout.font_size, max_width)? {
                    let (spans, _) = shape_and_record_spans(
                        ctx.fonts,
                        face,
                        &piece,
                        layout.font_size,
                        ctx.glyph_sets,
                        fill,
                        underline,
                    )?;
                    current_spans.extend(spans);
                    flush_line(&mut current_spans, out, false);
                    current_width = 0.0;
                }
                continue;
            }
            current_spans.extend(spans);
            current_width += w;
        }
    }

    flush_line(&mut current_spans, out, layout.glue_last_content);
    let content_end = out.len();
    apply_widow_orphan(&mut out[start..content_end]);
    if layout.gap_after > 0.0 {
        out.push(LaidItem::Text(LaidLine::gap(layout.gap_after)));
    }
    Ok(())
}

/// Keep at least two content lines together at paragraph start and end.
fn apply_widow_orphan(items: &mut [LaidItem]) {
    let idxs: Vec<usize> = items
        .iter()
        .enumerate()
        .filter_map(|(i, item)| match item {
            LaidItem::Text(line) if !line.spans.is_empty() => Some(i),
            _ => None,
        })
        .collect();
    if idxs.len() < 2 {
        return;
    }
    if let LaidItem::Text(line) = &mut items[idxs[0]] {
        line.glue_after = true;
    }
    let penultimate = idxs[idxs.len() - 2];
    if let LaidItem::Text(line) = &mut items[penultimate] {
        line.glue_after = true;
    }
}

/// Split `text` into pieces that each fit within `max_width` points.
fn hard_break_text(
    fonts: &FontBag,
    face: FaceRef,
    text: &str,
    font_size: f32,
    max_width: f32,
) -> Result<Vec<String>, WeaveError> {
    let mut pieces = Vec::new();
    let mut buf = String::new();
    for ch in text.chars() {
        let mut trial = buf.clone();
        trial.push(ch);
        let tw = shaped_runs_width(&shape_text_with_fallback(fonts, face, &trial, font_size)?);
        if tw > max_width && !buf.is_empty() {
            pieces.push(std::mem::take(&mut buf));
            buf.push(ch);
        } else {
            buf.push(ch);
        }
    }
    if !buf.is_empty() {
        pieces.push(buf);
    }
    if pieces.is_empty() {
        pieces.push(String::new());
    }
    Ok(pieces)
}

/// True when `chunk` should not start a new line (leading / orphan whitespace).
fn skip_wrap_chunk_at_line_start(chunk: &str) -> bool {
    chunk.is_empty() || chunk.chars().all(char::is_whitespace)
}

/// Take the next whitespace-delimited chunk (word + trailing spaces).
fn next_wrap_chunk(s: &str) -> (&str, &str) {
    let mut chars = s.char_indices();
    let Some((_, first)) = chars.next() else {
        return ("", "");
    };
    if first.is_whitespace() {
        let end = s.find(|c: char| !c.is_whitespace()).unwrap_or(s.len());
        return (&s[..end], &s[end..]);
    }
    let word_end = s.find(char::is_whitespace).unwrap_or(s.len());
    let after_ws = s[word_end..]
        .find(|c: char| !c.is_whitespace())
        .map_or(s.len(), |i| word_end + i);
    (&s[..after_ws], &s[after_ws..])
}

fn push_list_lines(
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

fn block_name(block: &PrintBlock) -> &'static str {
    match block {
        PrintBlock::Heading { .. } => "heading",
        PrintBlock::Paragraph { .. } => "paragraph",
        PrintBlock::List { .. } => "list",
        PrintBlock::Code { .. } => "code",
        PrintBlock::Quote { .. } => "quote",
        PrintBlock::Table { .. } => "table",
        PrintBlock::Figure { .. } => "figure",
        PrintBlock::Math { .. } => "math",
        PrintBlock::Slide { .. } => "slide",
        PrintBlock::Break(_) => "break",
    }
}
