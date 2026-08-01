//! Turn print IR blocks into laid segments, images, and glyph sets.

use crate::error::WeaveError;
use crate::font::{
    FaceId, FaceRef, FontBag, collect_glyph_set, note_shaped_glyphs, shape_text, shaped_width,
};
use crate::image_prep::{PreparedImage, prepare_image};
use crate::ir::{
    BreakHint, FigurePlacement, InlineStyle, PrintBlock, PrintDocument, PrintImage,
    SlideRegionContent, TableRow, TextRun,
};
use crate::profile::{self, ProfileMetrics};

use super::math::layout_math;
use super::types::{
    FaceMode, ForcedBreak, GlyphSets, LaidItem, LaidLine, LaidSpan, LaidTable, LaidTableRow,
    LayoutDoc, LayoutSegment, RunLayout,
};

/// Walk document blocks into layout segments (reading order + break hints).
pub(super) fn collect_layout(
    doc: &PrintDocument,
    metrics: &ProfileMetrics,
    fonts: &FontBag,
) -> Result<LayoutDoc, WeaveError> {
    let mut segments: Vec<(ForcedBreak, Vec<LaidItem>)> = vec![(ForcedBreak::None, Vec::new())];
    let mut images: Vec<PreparedImage> = Vec::new();
    let mut glyph_sets: GlyphSets = std::collections::BTreeMap::new();

    for block in &doc.blocks {
        layout_block(
            block,
            metrics,
            fonts,
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
    segments: &mut Vec<LayoutSegment>,
    images: &mut Vec<PreparedImage>,
    glyph_sets: &mut GlyphSets,
) -> Result<(), WeaveError> {
    match block {
        PrintBlock::Break(hint) => {
            if matches!(hint, BreakHint::Page | BreakHint::PageAlways) {
                segments.push((ForcedBreak::Always, Vec::new()));
            }
        }
        PrintBlock::Heading {
            level,
            runs,
            break_before,
        } => layout_heading(
            *level,
            runs,
            *break_before,
            metrics,
            fonts,
            segments,
            glyph_sets,
        )?,
        PrintBlock::Paragraph { runs } => {
            let seg = segments.last_mut().expect("segment");
            push_styled_runs(
                &mut seg.1,
                runs,
                metrics,
                fonts,
                glyph_sets,
                body_layout(metrics, 0.0),
            )?;
        }
        PrintBlock::Quote { runs } => layout_quote(runs, metrics, fonts, segments, glyph_sets)?,
        PrintBlock::Code { lang: _, text } => {
            layout_code(text, metrics, fonts, segments, glyph_sets)?;
        }
        PrintBlock::List { ordered, items } => {
            let seg = segments.last_mut().expect("segment");
            push_list_lines(&mut seg.1, *ordered, items, 0, metrics, fonts, glyph_sets)?;
        }
        PrintBlock::Table { rows } => {
            let seg = segments.last_mut().expect("segment");
            push_table(&mut seg.1, rows, metrics, fonts, glyph_sets)?;
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
                glyph_sets,
            }
            .run()?;
        }
        PrintBlock::Math { display, latex } => {
            layout_math(*display, latex, metrics, fonts, segments, glyph_sets)?;
        }
        PrintBlock::Slide { layout_id, regions } => {
            layout_slide(layout_id, regions, metrics, fonts, segments, glyph_sets)?;
        }
    }
    Ok(())
}

fn body_layout(metrics: &ProfileMetrics, indent: f32) -> RunLayout {
    RunLayout {
        font_size: metrics.body_size,
        leading: metrics.body_leading,
        gap_after: 10.0,
        glue_last_content: false,
        mode: FaceMode::Body,
        indent,
    }
}

fn layout_heading(
    level: u8,
    runs: &[TextRun],
    break_before: BreakHint,
    metrics: &ProfileMetrics,
    fonts: &FontBag,
    segments: &mut Vec<LayoutSegment>,
    glyph_sets: &mut GlyphSets,
) -> Result<(), WeaveError> {
    let profile_h1_break = metrics.force_h1_page_break && level == 1;
    let hint_break = matches!(break_before, BreakHint::Page | BreakHint::PageAlways);
    if (profile_h1_break || hint_break)
        && segments.last().is_some_and(|(_, items)| !items.is_empty())
    {
        segments.push((ForcedBreak::Always, Vec::new()));
    }
    let font_size = profile::heading_size(level, metrics);
    let glue = matches!(break_before, BreakHint::KeepWithNext) || level <= 2;
    let seg = segments.last_mut().expect("segment");
    push_styled_runs(
        &mut seg.1,
        runs,
        metrics,
        fonts,
        glyph_sets,
        RunLayout {
            font_size,
            leading: font_size * 1.35,
            gap_after: 8.0,
            glue_last_content: glue,
            mode: FaceMode::Heading,
            indent: 0.0,
        },
    )
}

fn layout_quote(
    runs: &[TextRun],
    metrics: &ProfileMetrics,
    fonts: &FontBag,
    segments: &mut [LayoutSegment],
    glyph_sets: &mut GlyphSets,
) -> Result<(), WeaveError> {
    let seg = segments.last_mut().expect("segment");
    let mut quoted = vec![TextRun {
        text: "\"".into(),
        style: InlineStyle {
            emphasis: true,
            ..InlineStyle::default()
        },
        face: None,
    }];
    quoted.extend(runs.iter().cloned());
    quoted.push(TextRun {
        text: "\"".into(),
        style: InlineStyle {
            emphasis: true,
            ..InlineStyle::default()
        },
        face: None,
    });
    push_styled_runs(
        &mut seg.1,
        &quoted,
        metrics,
        fonts,
        glyph_sets,
        body_layout(metrics, 18.0),
    )
}

fn layout_code(
    text: &str,
    metrics: &ProfileMetrics,
    fonts: &FontBag,
    segments: &mut [LayoutSegment],
    glyph_sets: &mut GlyphSets,
) -> Result<(), WeaveError> {
    let seg = segments.last_mut().expect("segment");
    let font_size = metrics.code_size;
    let leading = font_size * 1.25;
    for line in text.lines() {
        seg.1.push(LaidItem::Text(LaidLine::shaped(
            fonts,
            FaceRef::Bundled(FaceId::MonoRegular),
            line,
            font_size,
            leading,
            glyph_sets,
        )?));
    }
    seg.1.push(LaidItem::Text(LaidLine::gap(10.0)));
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
            )?;
            line.glue_after = !caption.is_empty() || float_near;
            seg.1.push(LaidItem::Text(line));
            if caption.is_empty() {
                seg.1.push(LaidItem::Text(LaidLine::gap(10.0)));
            } else {
                push_styled_runs(
                    &mut seg.1,
                    caption,
                    metrics,
                    fonts,
                    glyph_sets,
                    RunLayout {
                        font_size: metrics.body_size,
                        leading: metrics.body_leading,
                        gap_after: 10.0,
                        glue_last_content: false,
                        mode: FaceMode::Body,
                        indent: 0.0,
                    },
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
            seg.1.push(LaidItem::Text(LaidLine::gap(6.0)));
        } else {
            push_styled_runs(
                &mut seg.1,
                caption,
                metrics,
                fonts,
                glyph_sets,
                RunLayout {
                    font_size: metrics.body_size,
                    leading: metrics.body_leading,
                    gap_after: 10.0,
                    glue_last_content: false,
                    mode: FaceMode::Body,
                    indent: 0.0,
                },
            )?;
        }
        Ok(())
    }
}

fn push_table(
    out: &mut Vec<LaidItem>,
    rows: &[TableRow],
    metrics: &ProfileMetrics,
    fonts: &FontBag,
    glyph_sets: &mut GlyphSets,
) -> Result<(), WeaveError> {
    if rows.is_empty() {
        out.push(LaidItem::Text(LaidLine::shaped(
            fonts,
            FaceRef::Bundled(FaceId::SansItalic),
            "[table]",
            metrics.body_size,
            metrics.body_leading,
            glyph_sets,
        )?));
        out.push(LaidItem::Text(LaidLine::gap(10.0)));
        return Ok(());
    }

    let cols = rows.iter().map(|r| r.cells.len()).max().unwrap_or(0).max(1);
    let pad = 5.0_f32;
    let font_size = metrics.body_size;
    let leading = metrics.body_leading.min(font_size * 1.25);
    let face = FaceRef::Bundled(if metrics.serif_body {
        FaceId::SerifRegular
    } else {
        FaceId::SansRegular
    });
    let header_face = FaceRef::Bundled(if metrics.serif_body {
        FaceId::SerifBold
    } else {
        FaceId::SansBold
    });
    let col_width = metrics.content_width() / cols as f32;
    let inner_width = (col_width - pad * 2.0).max(24.0);
    let col_widths = vec![col_width; cols];

    let mut laid_rows = Vec::with_capacity(rows.len());
    for (row_idx, row) in rows.iter().enumerate() {
        let mut cells = Vec::with_capacity(cols);
        let mut row_h = pad * 2.0 + leading;
        for col in 0..cols {
            let text = row.cells.get(col).map_or("", String::as_str);
            let cell_face = if row_idx == 0 { header_face } else { face };
            let lines = wrap_plain_text(
                text,
                cell_face,
                font_size,
                leading,
                inner_width,
                fonts,
                glyph_sets,
            )?;
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
        gap_after: 12.0,
    }));
    Ok(())
}

fn wrap_plain_text(
    text: &str,
    face: FaceRef,
    font_size: f32,
    leading: f32,
    max_width: f32,
    fonts: &FontBag,
    glyph_sets: &mut GlyphSets,
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
        let glyphs = shape_text(fonts, face, chunk, font_size)?;
        let w = shaped_width(&glyphs);
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
            for piece in hard_break_text(fonts, face, chunk, font_size, max_width)? {
                let glyphs = shape_text(fonts, face, &piece, font_size)?;
                let set = glyph_sets.entry(face).or_default();
                collect_glyph_set(fonts, face, &piece, set);
                note_shaped_glyphs(&glyphs, set);
                lines.push(LaidLine {
                    spans: vec![LaidSpan {
                        face,
                        font_size,
                        glyphs,
                    }],
                    leading,
                    glue_after: false,
                    indent: 0.0,
                    center: false,
                });
            }
            continue;
        }
        let set = glyph_sets.entry(face).or_default();
        collect_glyph_set(fonts, face, chunk, set);
        note_shaped_glyphs(&glyphs, set);
        current.push(LaidSpan {
            face,
            font_size,
            glyphs,
        });
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
    metrics: &ProfileMetrics,
    fonts: &FontBag,
    segments: &mut Vec<LayoutSegment>,
    glyph_sets: &mut GlyphSets,
) -> Result<(), WeaveError> {
    let _ = layout_id;
    if segments.last().is_some_and(|(_, items)| !items.is_empty()) {
        segments.push((ForcedBreak::Always, Vec::new()));
    }

    let seg = segments.last_mut().expect("segment");
    let (titles, rest): (Vec<_>, Vec<_>) = regions.iter().partition(|r| {
        let slot = r.slot.to_ascii_lowercase();
        slot_name_is(&slot, "title") || slot_name_is(&slot, "heading")
    });

    if metrics.is_deck {
        seg.1.push(LaidItem::Text(LaidLine::gap(12.0)));
    }

    if titles.is_empty() && rest.is_empty() {
        seg.1.push(LaidItem::Text(LaidLine::shaped(
            fonts,
            FaceRef::Bundled(FaceId::SansItalic),
            "[empty slide]",
            metrics.body_size,
            metrics.body_leading,
            glyph_sets,
        )?));
    } else {
        push_slide_title_regions(&mut seg.1, &titles, metrics, fonts, glyph_sets)?;
        push_slide_body_regions(&mut seg.1, &rest, metrics, fonts, glyph_sets)?;
    }

    segments.push((ForcedBreak::Always, Vec::new()));
    Ok(())
}

fn push_slide_title_regions(
    out: &mut Vec<LaidItem>,
    titles: &[&SlideRegionContent],
    metrics: &ProfileMetrics,
    fonts: &FontBag,
    glyph_sets: &mut GlyphSets,
) -> Result<(), WeaveError> {
    let title_scale = if metrics.is_deck { 1.45 } else { 1.8 };
    let title_gap = if metrics.is_deck { 20.0 } else { 16.0 };
    for region in titles {
        push_styled_runs(
            out,
            &[TextRun {
                text: region.text.clone(),
                style: InlineStyle {
                    strong: true,
                    ..InlineStyle::default()
                },
                face: None,
            }],
            metrics,
            fonts,
            glyph_sets,
            RunLayout {
                font_size: metrics.body_size * title_scale,
                leading: metrics.body_size * title_scale * 1.2,
                gap_after: title_gap,
                glue_last_content: false,
                mode: FaceMode::Heading,
                indent: 0.0,
            },
        )?;
    }
    Ok(())
}

fn push_slide_body_regions(
    out: &mut Vec<LaidItem>,
    regions: &[&SlideRegionContent],
    metrics: &ProfileMetrics,
    fonts: &FontBag,
    glyph_sets: &mut GlyphSets,
) -> Result<(), WeaveError> {
    for region in regions {
        let slot = region.slot.to_ascii_lowercase();
        let (size, gap, mode, strong) = if slot_name_is(&slot, "subtitle") {
            (metrics.body_size * 1.15, 14.0, FaceMode::Body, false)
        } else {
            (metrics.body_size, 12.0, FaceMode::Body, false)
        };
        if !matches!(slot.as_str(), "body" | "content" | "text") && !region.slot.is_empty() {
            push_styled_runs(
                out,
                &[TextRun {
                    text: format!("{}:", region.slot),
                    style: InlineStyle {
                        strong: true,
                        ..InlineStyle::default()
                    },
                    face: None,
                }],
                metrics,
                fonts,
                glyph_sets,
                RunLayout {
                    font_size: metrics.body_size * 0.85,
                    leading: metrics.body_leading,
                    gap_after: 2.0,
                    glue_last_content: true,
                    mode: FaceMode::Body,
                    indent: 0.0,
                },
            )?;
        }
        push_styled_runs(
            out,
            &[TextRun {
                text: region.text.clone(),
                style: InlineStyle {
                    strong,
                    emphasis: slot_name_is(&slot, "subtitle"),
                    ..InlineStyle::default()
                },
                face: None,
            }],
            metrics,
            fonts,
            glyph_sets,
            RunLayout {
                font_size: size,
                leading: size * 1.35,
                gap_after: gap,
                glue_last_content: false,
                mode,
                indent: 0.0,
            },
        )?;
    }
    Ok(())
}

/// Match a slide slot name exactly, or as a dotted suffix (`main.title` → `title`).
fn slot_name_is(slot: &str, name: &str) -> bool {
    slot == name || slot.rsplit_once('.').is_some_and(|(_, tail)| tail == name)
}

/// Wrap styled runs into lines, then apply widow/orphan glue and optional gap.
pub(super) fn push_styled_runs(
    out: &mut Vec<LaidItem>,
    runs: &[TextRun],
    metrics: &ProfileMetrics,
    fonts: &FontBag,
    glyph_sets: &mut GlyphSets,
    layout: RunLayout,
) -> Result<(), WeaveError> {
    if runs.is_empty() {
        return Ok(());
    }

    let start = out.len();
    let max_width = (metrics.content_width() - layout.indent).max(36.0);
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
        let face = resolve_run_face(run, metrics, layout.mode, fonts)?;
        let mut remaining = run.text.as_str();
        while !remaining.is_empty() {
            let (chunk, rest) = next_wrap_chunk(remaining);
            remaining = rest;
            // Keep trailing spaces so inter-word advances are shaped; drop
            // whitespace-only chunks at the start of a line.
            if current_spans.is_empty() && skip_wrap_chunk_at_line_start(chunk) {
                continue;
            }
            let glyphs = shape_text(fonts, face, chunk, layout.font_size)?;
            let w = shaped_width(&glyphs);
            if current_width + w > max_width && !current_spans.is_empty() {
                flush_line(&mut current_spans, out, false);
                current_width = 0.0;
                if skip_wrap_chunk_at_line_start(chunk) {
                    continue;
                }
            }
            if w > max_width && current_spans.is_empty() {
                // Hard-break tokens wider than the content box (URLs, long code).
                for piece in hard_break_text(fonts, face, chunk, layout.font_size, max_width)? {
                    let glyphs = shape_text(fonts, face, &piece, layout.font_size)?;
                    let set = glyph_sets.entry(face).or_default();
                    collect_glyph_set(fonts, face, &piece, set);
                    note_shaped_glyphs(&glyphs, set);
                    current_spans.push(LaidSpan {
                        face,
                        font_size: layout.font_size,
                        glyphs,
                    });
                    flush_line(&mut current_spans, out, false);
                    current_width = 0.0;
                }
                continue;
            }
            let set = glyph_sets.entry(face).or_default();
            collect_glyph_set(fonts, face, chunk, set);
            note_shaped_glyphs(&glyphs, set);
            current_spans.push(LaidSpan {
                face,
                font_size: layout.font_size,
                glyphs,
            });
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

/// Keep at least two content lines together at paragraph start/end.
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
        let tw = shaped_width(&shape_text(fonts, face, &trial, font_size)?);
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
    metrics: &ProfileMetrics,
    fonts: &FontBag,
    glyph_sets: &mut GlyphSets,
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
            metrics,
            fonts,
            glyph_sets,
            RunLayout {
                font_size: metrics.body_size,
                leading: metrics.body_size * 1.35,
                gap_after: 0.0,
                glue_last_content: false,
                mode: FaceMode::Body,
                indent: 18.0 * depth as f32,
            },
        )?;
        for child in &item.children {
            match child {
                PrintBlock::List {
                    ordered: child_ordered,
                    items: child_items,
                } => push_list_lines(
                    out,
                    *child_ordered,
                    child_items,
                    depth + 1,
                    metrics,
                    fonts,
                    glyph_sets,
                )?,
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
