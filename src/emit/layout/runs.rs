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
use super::hyphen::{hyphen_fit, split_trailing_space};

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
    // Headings default to bold (same as bundled `resolve_face`).
    let mut style = run.style;
    if matches!(mode, FaceMode::Heading) && !style.code && !style.strong && !style.emphasis {
        style.strong = true;
    }

    // Explicit `\font{id}{…}` / TextRun.face wins: never substitute Liberation when
    // a bold/italic variant of the pin is missing (icon packs are regular-only).
    if let Some(id) = run.face.as_deref() {
        return resolve_named_pin(
            id, style, metrics, mode, fonts, /*allow_liberation*/ false,
        );
    }

    let category_pin = knobs
        .prose
        .category_font_pin(style.cite, font_category(mode, paint));
    if let Some(id) = category_pin {
        return resolve_named_pin(
            id, style, metrics, mode, fonts, /*allow_liberation*/ true,
        );
    }
    Ok(resolve_face(style, metrics, mode))
}

fn resolve_named_pin(
    id: &str,
    style: InlineStyle,
    metrics: &ProfileMetrics,
    mode: FaceMode,
    fonts: &FontBag,
    allow_liberation_style_fallback: bool,
) -> Result<FaceRef, WeaveError> {
    #[cfg(feature = "os-fonts")]
    let os_key = crate::os_fonts::os_pin_key(id, style);
    #[cfg(feature = "os-fonts")]
    if let Some(face) = fonts.resolve_pin(&os_key) {
        return Ok(face);
    }
    for candidate in style_aware_pin_ids(id, style) {
        if let Some(face) = fonts.resolve_pin(&candidate) {
            return Ok(face);
        }
    }
    if allow_liberation_style_fallback && (style.code || style.strong || style.emphasis) {
        return Ok(resolve_face(style, metrics, mode));
    }
    if let Some(face) = fonts.resolve_pin(id) {
        return Ok(face);
    }
    match fonts.resolve_mode() {
        crate::options::FontResolveMode::BundledOnly => {
            Err(WeaveError::Font(format!("unknown pinned face `{id}`")))
        }
        crate::options::FontResolveMode::OsWithFallback => Ok(resolve_face(style, metrics, mode)),
    }
}

/// Pin id candidates for a base family + style (`lato-bold`, `lato-italic`, …).
fn style_aware_pin_ids(base: &str, style: InlineStyle) -> Vec<String> {
    if style.code {
        return Vec::new();
    }
    match (style.strong, style.emphasis) {
        (true, true) => vec![format!("{base}-bolditalic"), format!("{base}-bold-italic")],
        (true, false) => vec![format!("{base}-bold")],
        (false, true) => vec![format!("{base}-italic")],
        (false, false) => Vec::new(),
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

    for run in runs {
        append_styled_run(
            out,
            &mut current_spans,
            &mut current_width,
            run,
            ctx,
            layout,
            max_width,
        )?;
    }

    flush_styled_line(&mut current_spans, out, layout, max_width, true);
    let content_end = out.len();
    apply_widow_orphan(
        &mut out[start..content_end],
        ctx.knobs.prose.wrap.orphan_lines,
        ctx.knobs.prose.wrap.widow_lines,
    );
    if layout.gap_after > 0.0 {
        out.push(LaidItem::Text(LaidLine::gap(layout.gap_after)));
    }
    Ok(())
}

fn flush_styled_line(
    spans: &mut Vec<LaidSpan>,
    dest: &mut Vec<LaidItem>,
    layout: RunLayout,
    max_width: f32,
    last: bool,
) {
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
        dest_id: None,
    }));
}

#[allow(clippy::too_many_arguments)]
fn append_styled_run(
    out: &mut Vec<LaidItem>,
    current_spans: &mut Vec<LaidSpan>,
    current_width: &mut f32,
    run: &TextRun,
    ctx: &mut LayoutCtx,
    layout: RunLayout,
    max_width: f32,
) -> Result<(), WeaveError> {
    let face = resolve_run_face(
        run,
        ctx.metrics,
        layout.mode,
        ctx.fonts,
        ctx.knobs,
        layout.paint,
    )?;
    // Font Awesome (`fab`/`fas`): match LaTeX `fontawesome5` — `\faLinkedin` /
    // `\faGlobe` at the current text size on the shared baseline (no scale,
    // no raisebox, no invented gap). Author spaces in Tessprek like the .tex.
    let font_size = layout.font_size;
    let baseline_shift = 0.0_f32;
    let (fill, mut underline) = ctx.knobs.prose.run_paint_rgb01(
        run.style.cite || run.style.link || run.link_uri.is_some(),
        layout.paint,
        run.style.underline,
    );
    // Text links auto-underline only when `[link].underline` (default off).
    if run.link_uri.is_some() && run.face.is_none() && ctx.knobs.prose.link.underline {
        underline = true;
    }
    let link_uri = run.link_uri.as_deref();
    let hyphenate = ctx.knobs.prose.wrap.hyphenate;
    let mut queue = run.text.clone();
    while !queue.is_empty() {
        let (chunk_borrowed, rest_borrowed) = next_wrap_chunk(&queue);
        let chunk = chunk_borrowed.to_string();
        let rest = rest_borrowed.to_string();
        // Keep trailing spaces so inter-word advances are shaped; drop
        // whitespace-only chunks at the start of a line.
        if current_spans.is_empty() && skip_wrap_chunk_at_line_start(&chunk) {
            queue = rest;
            continue;
        }
        let (spans, w) = shape_and_record_spans(
            ctx.fonts,
            face,
            &chunk,
            font_size,
            ctx.glyph_sets,
            fill,
            underline,
            link_uri,
            None,
            baseline_shift,
        )?;
        if *current_width + w > max_width && !current_spans.is_empty() {
            let remain = (max_width - *current_width).max(0.0);
            if hyphenate
                && try_hyphen_flush(
                    out,
                    current_spans,
                    current_width,
                    ctx,
                    face,
                    &chunk,
                    remain,
                    font_size,
                    fill,
                    underline,
                    link_uri,
                    baseline_shift,
                    layout,
                    max_width,
                    &rest,
                    &mut queue,
                )?
            {
                continue;
            }
            flush_styled_line(current_spans, out, layout, max_width, false);
            *current_width = 0.0;
            if skip_wrap_chunk_at_line_start(&chunk) {
                queue = rest;
                continue;
            }
        }
        if w > max_width && current_spans.is_empty() {
            if hyphenate
                && try_hyphen_flush(
                    out,
                    current_spans,
                    current_width,
                    ctx,
                    face,
                    &chunk,
                    max_width,
                    font_size,
                    fill,
                    underline,
                    link_uri,
                    baseline_shift,
                    layout,
                    max_width,
                    &rest,
                    &mut queue,
                )?
            {
                continue;
            }
            if layout.hard_break_overflow {
                // Hard-break tokens wider than the content box (URLs, long code).
                for piece in hard_break_text(ctx.fonts, face, &chunk, font_size, max_width)? {
                    place_shaped_flush(
                        out,
                        current_spans,
                        current_width,
                        ctx,
                        face,
                        &piece,
                        font_size,
                        fill,
                        underline,
                        link_uri,
                        baseline_shift,
                        layout,
                        max_width,
                    )?;
                }
                queue = rest;
                continue;
            }
        }
        // soft_only: place an overlong token and let it stick out.
        current_spans.extend(spans);
        *current_width += w;
        queue = rest;
    }
    Ok(())
}

/// Shape `text`, append to the current line, flush, and zero width.
#[allow(clippy::too_many_arguments)]
fn place_shaped_flush(
    out: &mut Vec<LaidItem>,
    current_spans: &mut Vec<LaidSpan>,
    current_width: &mut f32,
    ctx: &mut LayoutCtx,
    face: FaceRef,
    text: &str,
    font_size: f32,
    fill: [f32; 3],
    underline: bool,
    link_uri: Option<&str>,
    baseline_shift: f32,
    layout: RunLayout,
    max_width: f32,
) -> Result<(), WeaveError> {
    let (spans, w) = shape_and_record_spans(
        ctx.fonts,
        face,
        text,
        font_size,
        ctx.glyph_sets,
        fill,
        underline,
        link_uri,
        None,
        baseline_shift,
    )?;
    current_spans.extend(spans);
    *current_width += w;
    flush_styled_line(current_spans, out, layout, max_width, false);
    *current_width = 0.0;
    Ok(())
}

/// If `chunk` can soft-hyphenate into `fit_width`, place the prefix, flush, and
/// rewrite `queue` to `right + trail + rest`. Returns `true` when a split landed.
#[allow(clippy::too_many_arguments)]
fn try_hyphen_flush(
    out: &mut Vec<LaidItem>,
    current_spans: &mut Vec<LaidSpan>,
    current_width: &mut f32,
    ctx: &mut LayoutCtx,
    face: FaceRef,
    chunk: &str,
    fit_width: f32,
    font_size: f32,
    fill: [f32; 3],
    underline: bool,
    link_uri: Option<&str>,
    baseline_shift: f32,
    layout: RunLayout,
    max_width: f32,
    rest: &str,
    queue: &mut String,
) -> Result<bool, WeaveError> {
    let (word, trail) = split_trailing_space(chunk);
    let Some((left, right)) = hyphen_fit(ctx.fonts, face, word, font_size, fit_width)? else {
        return Ok(false);
    };
    place_shaped_flush(
        out,
        current_spans,
        current_width,
        ctx,
        face,
        &left,
        font_size,
        fill,
        underline,
        link_uri,
        baseline_shift,
        layout,
        max_width,
    )?;
    *queue = format!("{right}{trail}{rest}");
    Ok(true)
}

/// Keep `orphan_lines` / `widow_lines` content lines together (CSS-like).
///
/// Values below 1 are treated as 1. Glues the first `orphan_lines - 1` content
/// lines and the `widow_lines - 1` lines before the last content line.
pub(super) fn apply_widow_orphan(items: &mut [LaidItem], orphan_lines: u32, widow_lines: u32) {
    let orphans = orphan_lines.max(1) as usize;
    let widows = widow_lines.max(1) as usize;
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
    // Orphans: keep the first N lines together → glue first N-1.
    for &i in idxs.iter().take(orphans.saturating_sub(1)) {
        if let LaidItem::Text(line) = &mut items[i] {
            line.glue_after = true;
        }
    }
    // Widows: keep the last N lines together → glue the N-1 before the last.
    if idxs.len() >= widows {
        let start = idxs.len() - widows;
        for &i in idxs.iter().skip(start).take(widows.saturating_sub(1)) {
            if let LaidItem::Text(line) = &mut items[i] {
                line.glue_after = true;
            }
        }
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

#[cfg(test)]
mod widow_orphan_tests {
    use super::{LaidItem, LaidLine, LaidSpan, apply_widow_orphan};
    use crate::font::{FaceId, FaceRef};
    use crate::knobs::TextAlign;

    fn content_line(glue: bool) -> LaidItem {
        LaidItem::Text(LaidLine {
            // Non-empty spans so `is_gap()` is false (gap = empty spans).
            spans: vec![LaidSpan {
                face: FaceRef::Bundled(FaceId::SansRegular),
                font_size: 11.0,
                glyphs: Vec::new(),
                fill: [0.0, 0.0, 0.0],
                underline: false,
                link_uri: None,
                link_dest: None,
                baseline_shift: 0.0,
            }],
            leading: 12.0,
            glue_after: glue,
            indent: 0.0,
            measure: 100.0,
            text_align: TextAlign::Left,
            dest_id: None,
        })
    }

    fn glue_flags(items: &[LaidItem]) -> Vec<bool> {
        items
            .iter()
            .map(|item| match item {
                LaidItem::Text(line) => line.glue_after,
                _ => false,
            })
            .collect()
    }

    #[test]
    fn default_two_two_glues_first_and_penultimate() {
        let mut items = vec![
            content_line(false),
            content_line(false),
            content_line(false),
            content_line(false),
        ];
        apply_widow_orphan(&mut items, 2, 2);
        assert_eq!(glue_flags(&items), vec![true, false, true, false]);
    }

    #[test]
    fn orphan_three_glues_first_two() {
        let mut items = vec![
            content_line(false),
            content_line(false),
            content_line(false),
            content_line(false),
        ];
        apply_widow_orphan(&mut items, 3, 1);
        assert_eq!(glue_flags(&items), vec![true, true, false, false]);
    }

    #[test]
    fn widow_three_glues_two_before_last() {
        let mut items = vec![
            content_line(false),
            content_line(false),
            content_line(false),
            content_line(false),
        ];
        apply_widow_orphan(&mut items, 1, 3);
        assert_eq!(glue_flags(&items), vec![false, true, true, false]);
    }

    #[test]
    fn values_below_one_treated_as_one() {
        let mut items = vec![
            content_line(false),
            content_line(false),
            content_line(false),
        ];
        apply_widow_orphan(&mut items, 0, 0);
        assert_eq!(glue_flags(&items), vec![false, false, false]);
    }
}
