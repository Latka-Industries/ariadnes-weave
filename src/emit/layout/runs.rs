//! Styled-run wrapping, face resolution, and wrap helpers.

use crate::error::WeaveError;
use crate::font::{FaceId, FaceRef, FontBag, shape_text_with_fallback, shaped_runs_width};
use crate::ir::{InlineStyle, TextRun};
use crate::knobs::{
    CaptionBand, CaptionOverflow, FigureAlign, LayoutKnobs, ProseFontCategory, TextAlign,
};
use crate::profile::ProfileMetrics;

use super::super::types::{
    FaceMode, LaidItem, LaidLine, LaidSpan, PaintCategory, RunLayout, shape_and_record_spans,
};
use super::LayoutCtx;

pub(super) fn layout_ctx<'a>(
    metrics: &'a ProfileMetrics,
    fonts: &'a FontBag,
    knobs: &'a LayoutKnobs,
    glyph_sets: &'a mut super::super::types::GlyphSets,
) -> LayoutCtx<'a> {
    LayoutCtx {
        metrics,
        fonts,
        knobs,
        glyph_sets,
    }
}

pub(super) fn body_layout(
    metrics: &ProfileMetrics,
    knobs: &LayoutKnobs,
    indent: f32,
    paint: PaintCategory,
) -> RunLayout {
    let text_align = match paint {
        PaintCategory::Text => knobs.prose.paragraph.text_align,
        _ => TextAlign::Left,
    };
    RunLayout {
        font_size: metrics.body_size,
        leading: metrics.body_leading,
        gap_after: knobs.prose.paragraph.gap_after,
        glue_last_content: false,
        mode: FaceMode::Body,
        indent,
        max_width: None,
        paint,
        hard_break_overflow: true,
        text_align,
    }
}

/// Indent + wrap measure for a figure-width text band inside the content box.
fn figure_band_box(
    align: FigureAlign,
    content_w: f32,
    band_width: f32,
    min_width: f32,
) -> (f32, f32) {
    (
        align.offset_x(content_w, band_width),
        band_width.max(min_width),
    )
}

/// Caption layout: body size × `[caption].size_factor`, caption gap / paint.
///
/// `[caption].band` chooses match-figure vs full-measure; `[caption].overflow`
/// controls mid-token hard breaks; `[caption].text_align` is in-band (default
/// `follow` = figure `align`).
pub(super) fn caption_layout(
    metrics: &ProfileMetrics,
    knobs: &LayoutKnobs,
    band_width: f32,
) -> RunLayout {
    let font_size = metrics.body_size * knobs.prose.caption.size_factor;
    let content_w = metrics.content_width();
    let figure_align = knobs.prose.figure.align;
    let text_align = knobs.prose.caption.text_align.resolve(figure_align);
    let (indent, max_width) = match knobs.prose.caption.band {
        CaptionBand::MatchFigure => {
            let (indent, measure) = figure_band_box(
                figure_align,
                content_w,
                band_width,
                knobs.prose.wrap.min_width,
            );
            (indent, Some(measure))
        }
        CaptionBand::FullMeasure => (0.0, None),
    };
    RunLayout {
        font_size,
        leading: font_size * knobs.prose.caption.leading_factor,
        gap_after: knobs.prose.caption.gap_after,
        glue_last_content: false,
        mode: FaceMode::Body,
        indent,
        max_width,
        paint: PaintCategory::Caption,
        hard_break_overflow: matches!(knobs.prose.caption.overflow, CaptionOverflow::HardBreak),
        text_align,
    }
}

/// Figure title: band from `title_align`; in-band text from `title_text_align`.
pub(super) fn figure_title_layout(
    metrics: &ProfileMetrics,
    knobs: &LayoutKnobs,
    band_width: f32,
) -> RunLayout {
    let figure_align = knobs.prose.figure.align;
    let band_align = knobs.prose.figure.title_align.resolve(figure_align);
    let text_align = knobs.prose.figure.title_text_align.resolve(figure_align);
    let (indent, measure) = figure_band_box(
        band_align,
        metrics.content_width(),
        band_width,
        knobs.prose.wrap.min_width,
    );
    RunLayout {
        font_size: metrics.body_size,
        leading: metrics.body_leading,
        gap_after: knobs.prose.figure.gap_after_title,
        glue_last_content: true,
        mode: FaceMode::Body,
        indent,
        max_width: Some(measure),
        paint: PaintCategory::Text,
        hard_break_overflow: true,
        text_align,
    }
}

/// Clone runs, OR-ing `emphasis` when `italic` is set (quote / caption knobs).
pub(super) fn with_knob_italic(runs: &[TextRun], italic: bool) -> Vec<TextRun> {
    runs.iter()
        .cloned()
        .map(|mut run| {
            run.style.emphasis |= italic;
            run
        })
        .collect()
}

pub(super) fn resolve_face(
    style: InlineStyle,
    metrics: &ProfileMetrics,
    mode: FaceMode,
) -> FaceRef {
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

fn font_category(mode: FaceMode, paint: PaintCategory) -> ProseFontCategory {
    match mode {
        FaceMode::Heading => ProseFontCategory::Heading,
        FaceMode::Body => ProseFontCategory::from(paint),
    }
}

pub(super) fn resolve_run_face(
    run: &TextRun,
    metrics: &ProfileMetrics,
    mode: FaceMode,
    fonts: &FontBag,
    knobs: &LayoutKnobs,
    paint: PaintCategory,
) -> Result<FaceRef, WeaveError> {
    let category_pin = knobs
        .prose
        .category_font_pin(run.style.cite, font_category(mode, paint));
    let effective = run.face.as_deref().or(category_pin);
    if let Some(id) = effective {
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

    let flush_line = |spans: &mut Vec<LaidSpan>, dest: &mut Vec<LaidItem>, last: bool| {
        if spans.is_empty() {
            return;
        }
        // Last soft-wrapped line of a justified block stays flush-left.
        let text_align = match layout.text_align {
            TextAlign::Justify if last => TextAlign::Left,
            other => other,
        };
        dest.push(LaidItem::Text(LaidLine {
            spans: std::mem::take(spans),
            leading: layout.leading,
            glue_after: last && layout.glue_last_content,
            indent: layout.indent,
            measure: max_width,
            text_align,
        }));
    };

    for run in runs {
        let face = resolve_run_face(
            run,
            ctx.metrics,
            layout.mode,
            ctx.fonts,
            ctx.knobs,
            layout.paint,
        )?;
        let (fill, underline) =
            ctx.knobs
                .prose
                .run_paint_rgb01(run.style.cite, layout.paint, run.style.underline);
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
            if w > max_width && current_spans.is_empty() && layout.hard_break_overflow {
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
            // soft_only: place an overlong token and let it stick out.
            current_spans.extend(spans);
            current_width += w;
        }
    }

    flush_line(&mut current_spans, out, true);
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
            LaidItem::Text(line) if !line.is_gap() => Some(i),
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
pub(super) fn hard_break_text(
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
pub(super) fn skip_wrap_chunk_at_line_start(chunk: &str) -> bool {
    chunk.is_empty() || chunk.chars().all(char::is_whitespace)
}

/// Take the next whitespace-delimited chunk (word + trailing spaces).
pub(super) fn next_wrap_chunk(s: &str) -> (&str, &str) {
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

/// Natural (unwrapped) advance width of styled runs at `font_size`.
pub(super) fn measure_runs_natural_width(
    runs: &[TextRun],
    ctx: &mut LayoutCtx,
    font_size: f32,
) -> Result<f32, WeaveError> {
    let mut width = 0.0_f32;
    for run in runs {
        let face = resolve_run_face(
            run,
            ctx.metrics,
            FaceMode::Body,
            ctx.fonts,
            ctx.knobs,
            PaintCategory::Text,
        )?;
        let shaped = shape_text_with_fallback(ctx.fonts, face, &run.text, font_size)?;
        width += shaped_runs_width(&shaped);
    }
    Ok(width)
}
