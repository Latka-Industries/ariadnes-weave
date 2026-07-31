//! Liberation TTF emit from print IR (Type0 + rustybuzz shaping).

use std::collections::BTreeMap;

use pdf_writer::{Content, Name, Pdf, Rect, Ref, Str, TextStr};

use crate::error::WeaveError;
use crate::font::{
    FaceId, FontObjIds, PreparedSubset, ShapedGlyph, collect_glyph_set, encode_gids,
    note_shaped_glyphs, prepare_subset, resource_name, shape_text, shaped_width,
    write_embedded_font,
};
use crate::image_prep::{PreparedImage, prepare_image};
use crate::ir::{
    BreakHint, FigurePlacement, InlineStyle, PrintBlock, PrintDocument, PrintImage, TextRun,
};
use crate::profile::{self, ProfileMetrics};

type GlyphSet = BTreeMap<u16, String>;
type GlyphSets = BTreeMap<FaceId, GlyphSet>;
type SubsetMap = BTreeMap<FaceId, PreparedSubset>;

/// Emit PDF bytes from a print document.
///
/// # Errors
///
/// Returns [`WeaveError`] if the profile is unsupported, a block kind is not
/// implemented, font subsetting/embedding fails, or an image cannot be decoded.
pub fn emit_pdf(doc: &PrintDocument) -> Result<Vec<u8>, WeaveError> {
    let metrics = profile::resolve_metrics(&doc.profile)?;
    let (segments, images, mut glyph_sets) = collect_layout(doc, &metrics)?;

    // Footer always uses Sans Regular.
    collect_glyph_set(
        FaceId::SansRegular,
        "0123456789 /",
        glyph_sets.entry(FaceId::SansRegular).or_default(),
    );

    let mut pages = paginate_items(&segments, metrics.content_height());

    let mut subsets = SubsetMap::new();
    for (&face_id, set) in &glyph_sets {
        subsets.insert(face_id, prepare_subset(face_id, set)?);
    }
    remap_pages(&mut pages, &subsets);

    let mut pdf = Pdf::new();
    pdf.set_version(1, 7);

    let catalog_id = Ref::new(1);
    let page_tree_id = Ref::new(2);
    let mut next_id = 3_i32;

    let mut font_refs: BTreeMap<FaceId, Ref> = BTreeMap::new();
    for (&face_id, subset) in &subsets {
        let ids = FontObjIds {
            type0: Ref::new(next_id),
            cid: Ref::new(next_id + 1),
            descriptor: Ref::new(next_id + 2),
            cmap: Ref::new(next_id + 3),
            data: Ref::new(next_id + 4),
        };
        next_id += 5;
        write_embedded_font(&mut pdf, face_id, &subset.data, &subset.glyph_set, ids)?;
        font_refs.insert(face_id, ids.type0);
    }

    let mut image_refs: Vec<(Ref, Option<Ref>)> = Vec::with_capacity(images.len());
    for img in &images {
        let image_id = Ref::new(next_id);
        next_id += 1;
        let mask_id = if img.mask.is_some() {
            let id = Ref::new(next_id);
            next_id += 1;
            Some(id)
        } else {
            None
        };
        image_refs.push((image_id, mask_id));
    }

    let mut page_ids = Vec::with_capacity(pages.len());
    let mut content_ids = Vec::with_capacity(pages.len());
    for _ in &pages {
        page_ids.push(Ref::new(next_id));
        next_id += 1;
        content_ids.push(Ref::new(next_id));
        next_id += 1;
    }

    pdf.catalog(catalog_id).pages(page_tree_id);
    pdf.pages(page_tree_id)
        .kids(page_ids.iter().copied())
        .count(page_ids.len() as i32);

    for (img, (image_id, mask_id)) in images.iter().zip(image_refs.iter()) {
        {
            let mut image = pdf.image_xobject(*image_id, &img.samples);
            image.filter(img.filter);
            image.width(img.width as i32);
            image.height(img.height as i32);
            image.color_space().device_rgb();
            image.bits_per_component(8);
            if let Some(mid) = mask_id {
                image.s_mask(*mid);
            }
        }
        if let (Some(mid), Some(mask_samples)) = (mask_id, &img.mask) {
            let mut s_mask = pdf.image_xobject(*mid, mask_samples);
            s_mask.filter(img.filter);
            s_mask.width(img.width as i32);
            s_mask.height(img.height as i32);
            s_mask.color_space().device_gray();
            s_mask.bits_per_component(8);
        }
    }

    let page_count = pages.len().max(1);
    for (page_idx, ((page_id, content_id), page_items)) in page_ids
        .iter()
        .copied()
        .zip(content_ids.iter().copied())
        .zip(pages.iter())
        .enumerate()
    {
        let used_images: Vec<usize> = page_items
            .iter()
            .filter_map(|item| match item {
                LaidItem::Image { img_idx, .. } => Some(*img_idx),
                LaidItem::Text(_) => None,
            })
            .collect();

        {
            let mut page = pdf.page(page_id);
            page.media_box(Rect::new(0.0, 0.0, metrics.page_w, metrics.page_h));
            page.parent(page_tree_id);
            page.contents(content_id);
            let mut resources = page.resources();
            {
                let mut fonts = resources.fonts();
                for (face_id, type0) in &font_refs {
                    fonts.pair(Name(resource_name(*face_id)), *type0);
                }
            }
            if !used_images.is_empty() {
                let mut xobjs = resources.x_objects();
                for idx in &used_images {
                    let name = image_resource_name(*idx);
                    xobjs.pair(Name(&name), image_refs[*idx].0);
                }
            }
        }

        let content_bytes =
            build_page_content(page_items, &metrics, page_idx + 1, page_count, &subsets)?;
        pdf.stream(content_id, &content_bytes);
    }

    let info_id = Ref::new(next_id);
    pdf.document_info(info_id)
        .title(TextStr(&doc.meta.title))
        .creator(TextStr("ariadnes-weave"))
        .producer(TextStr(&format!("ariadnes-weave {}", crate::VERSION)));

    Ok(pdf.finish())
}

fn image_resource_name(idx: usize) -> Vec<u8> {
    format!("Im{idx}").into_bytes()
}

fn remap_pages(pages: &mut [Vec<LaidItem>], subsets: &SubsetMap) {
    for page in pages {
        for item in page {
            if let LaidItem::Text(line) = item {
                for span in &mut line.spans {
                    if let Some(subset) = subsets.get(&span.face) {
                        for g in &mut span.glyphs {
                            *g = subset.remap_glyph(*g);
                        }
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
struct LaidSpan {
    face: FaceId,
    font_size: f32,
    glyphs: Vec<ShapedGlyph>,
}

#[derive(Debug, Clone)]
struct LaidLine {
    spans: Vec<LaidSpan>,
    leading: f32,
    glue_after: bool,
    /// Left indent inside the content box (points).
    indent: f32,
}

impl LaidLine {
    fn gap(leading: f32) -> Self {
        Self {
            spans: Vec::new(),
            leading,
            glue_after: false,
            indent: 0.0,
        }
    }

    fn shaped(
        face: FaceId,
        text: &str,
        font_size: f32,
        leading: f32,
        glyph_sets: &mut GlyphSets,
    ) -> Result<Self, WeaveError> {
        let glyphs = shape_text(face, text, font_size)?;
        let set = glyph_sets.entry(face).or_default();
        collect_glyph_set(face, text, set);
        note_shaped_glyphs(&glyphs, set);
        Ok(Self {
            spans: vec![LaidSpan {
                face,
                font_size,
                glyphs,
            }],
            leading,
            glue_after: false,
            indent: 0.0,
        })
    }
}

#[derive(Debug, Clone)]
enum LaidItem {
    Text(LaidLine),
    Image {
        img_idx: usize,
        width: f32,
        height: f32,
    },
}

impl LaidItem {
    fn height(&self) -> f32 {
        match self {
            Self::Text(line) => line.leading,
            Self::Image { height, .. } => *height + 8.0,
        }
    }

    fn glue_after(&self) -> bool {
        match self {
            Self::Text(line) => line.glue_after,
            Self::Image { .. } => false,
        }
    }
}

type LayoutSegment = (ForcedBreak, Vec<LaidItem>);
type LayoutDoc = (Vec<LayoutSegment>, Vec<PreparedImage>, GlyphSets);

fn collect_layout(doc: &PrintDocument, metrics: &ProfileMetrics) -> Result<LayoutDoc, WeaveError> {
    let mut segments: Vec<(ForcedBreak, Vec<LaidItem>)> = vec![(ForcedBreak::None, Vec::new())];
    let mut images: Vec<PreparedImage> = Vec::new();
    let mut glyph_sets: GlyphSets = BTreeMap::new();

    for block in &doc.blocks {
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
            } => {
                let profile_h1_break = metrics.force_h1_page_break && *level == 1;
                let hint_break = matches!(break_before, BreakHint::Page | BreakHint::PageAlways);
                if (profile_h1_break || hint_break)
                    && segments.last().is_some_and(|(_, items)| !items.is_empty())
                {
                    segments.push((ForcedBreak::Always, Vec::new()));
                }
                let font_size = profile::heading_size(*level, metrics);
                let glue = matches!(break_before, BreakHint::KeepWithNext) || *level <= 2;
                let seg = segments.last_mut().expect("segment");
                push_styled_runs(
                    &mut seg.1,
                    runs,
                    metrics,
                    &mut glyph_sets,
                    RunLayout {
                        font_size,
                        leading: font_size * 1.35,
                        gap_after: 8.0,
                        glue_last_content: glue,
                        mode: FaceMode::Heading,
                        indent: 0.0,
                    },
                )?;
            }
            PrintBlock::Paragraph { runs } => {
                let seg = segments.last_mut().expect("segment");
                push_styled_runs(
                    &mut seg.1,
                    runs,
                    metrics,
                    &mut glyph_sets,
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
            PrintBlock::Quote { runs } => {
                let seg = segments.last_mut().expect("segment");
                let mut quoted = vec![TextRun {
                    text: "\"".into(),
                    style: InlineStyle {
                        emphasis: true,
                        ..InlineStyle::default()
                    },
                }];
                quoted.extend(runs.iter().cloned());
                quoted.push(TextRun {
                    text: "\"".into(),
                    style: InlineStyle {
                        emphasis: true,
                        ..InlineStyle::default()
                    },
                });
                push_styled_runs(
                    &mut seg.1,
                    &quoted,
                    metrics,
                    &mut glyph_sets,
                    RunLayout {
                        font_size: metrics.body_size,
                        leading: metrics.body_leading,
                        gap_after: 10.0,
                        glue_last_content: false,
                        mode: FaceMode::Body,
                        indent: 18.0,
                    },
                )?;
            }
            PrintBlock::Code { lang: _, text } => {
                let seg = segments.last_mut().expect("segment");
                let font_size = metrics.code_size;
                let leading = font_size * 1.25;
                for line in text.lines() {
                    seg.1.push(LaidItem::Text(LaidLine::shaped(
                        FaceId::MonoRegular,
                        line,
                        font_size,
                        leading,
                        &mut glyph_sets,
                    )?));
                }
                seg.1.push(LaidItem::Text(LaidLine::gap(10.0)));
            }
            PrintBlock::List { ordered, items } => {
                let seg = segments.last_mut().expect("segment");
                push_list_lines(&mut seg.1, *ordered, items, 0, metrics, &mut glyph_sets)?;
            }
            PrintBlock::Table { rows } => {
                let seg = segments.last_mut().expect("segment");
                push_table_lines(&mut seg.1, rows, metrics, &mut glyph_sets)?;
            }
            PrintBlock::Figure {
                image,
                alt,
                caption,
                placement,
            } => {
                let _ = placement;
                let _ = FigurePlacement::Flow;
                push_figure(
                    &mut segments,
                    &mut images,
                    image,
                    alt,
                    caption,
                    metrics,
                    &mut glyph_sets,
                )?;
            }
            PrintBlock::Math { display: _, latex } => {
                let seg = segments.last_mut().expect("segment");
                let line = format!("[math] {latex}");
                seg.1.push(LaidItem::Text(LaidLine::shaped(
                    FaceId::MonoRegular,
                    &line,
                    metrics.code_size,
                    metrics.code_size * 1.25,
                    &mut glyph_sets,
                )?));
                seg.1.push(LaidItem::Text(LaidLine::gap(10.0)));
            }
            PrintBlock::Slide { layout_id, regions } => {
                let seg = segments.last_mut().expect("segment");
                let line = format!("[slide:{layout_id} regions={}]", regions.len());
                seg.1.push(LaidItem::Text(LaidLine::shaped(
                    FaceId::SansItalic,
                    &line,
                    metrics.body_size,
                    metrics.body_leading,
                    &mut glyph_sets,
                )?));
                segments.push((ForcedBreak::Always, Vec::new()));
            }
        }
    }

    Ok((segments, images, glyph_sets))
}

#[derive(Debug, Clone, Copy)]
enum FaceMode {
    Body,
    Heading,
}

fn resolve_face(style: InlineStyle, metrics: &ProfileMetrics, mode: FaceMode) -> FaceId {
    match mode {
        FaceMode::Heading => {
            let mut s = style;
            if !s.code && !s.strong && !s.emphasis {
                s.strong = true;
            }
            FaceId::from_style(&s, false)
        }
        FaceMode::Body => FaceId::from_style(&style, metrics.serif_body),
    }
}

#[derive(Clone, Copy)]
struct RunLayout {
    font_size: f32,
    leading: f32,
    gap_after: f32,
    glue_last_content: bool,
    mode: FaceMode,
    indent: f32,
}

fn push_figure(
    segments: &mut [LayoutSegment],
    images: &mut Vec<PreparedImage>,
    image: &PrintImage,
    alt: &str,
    caption: &[TextRun],
    metrics: &ProfileMetrics,
    glyph_sets: &mut GlyphSets,
) -> Result<(), WeaveError> {
    let Ok(prepared) = prepare_image(image) else {
        let seg = segments.last_mut().expect("segment");
        let label = if alt.is_empty() {
            "[figure]".into()
        } else {
            format!("[figure: {alt}]")
        };
        seg.1.push(LaidItem::Text(LaidLine::shaped(
            FaceId::SansItalic,
            &label,
            metrics.body_size,
            metrics.body_leading,
            glyph_sets,
        )?));
        if caption.is_empty() {
            seg.1.push(LaidItem::Text(LaidLine::gap(10.0)));
        } else {
            push_styled_runs(
                &mut seg.1,
                caption,
                metrics,
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

    let seg = segments.last_mut().expect("segment");
    seg.1.push(LaidItem::Image {
        img_idx,
        width: w,
        height: h,
    });
    if caption.is_empty() {
        seg.1.push(LaidItem::Text(LaidLine::gap(6.0)));
    } else {
        push_styled_runs(
            &mut seg.1,
            caption,
            metrics,
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

fn push_table_lines(
    out: &mut Vec<LaidItem>,
    rows: &[crate::ir::TableRow],
    metrics: &ProfileMetrics,
    glyph_sets: &mut GlyphSets,
) -> Result<(), WeaveError> {
    if rows.is_empty() {
        out.push(LaidItem::Text(LaidLine::shaped(
            FaceId::SansItalic,
            "[table]",
            metrics.body_size,
            metrics.body_leading,
            glyph_sets,
        )?));
        out.push(LaidItem::Text(LaidLine::gap(10.0)));
        return Ok(());
    }

    let cols = rows.iter().map(|r| r.cells.len()).max().unwrap_or(0);
    let mut widths = vec![0usize; cols];
    for row in rows {
        for (i, cell) in row.cells.iter().enumerate() {
            widths[i] = widths[i].max(cell.len());
        }
    }

    let font_size = metrics.code_size;
    let leading = font_size * 1.3;
    let rule = {
        let inner: usize = widths.iter().map(|w| w + 2).sum::<usize>() + cols.saturating_sub(1);
        format!("+{}+", "-".repeat(inner.max(1)))
    };
    out.push(LaidItem::Text(LaidLine::shaped(
        FaceId::MonoRegular,
        &rule,
        font_size,
        leading,
        glyph_sets,
    )?));

    for row in rows {
        let mut line = String::from("|");
        for (i, width) in widths.iter().enumerate() {
            let cell = row.cells.get(i).map_or("", String::as_str);
            line.push(' ');
            line.push_str(cell);
            line.push_str(&" ".repeat(width.saturating_sub(cell.len())));
            line.push_str(" |");
        }
        out.push(LaidItem::Text(LaidLine::shaped(
            FaceId::MonoRegular,
            &line,
            font_size,
            leading,
            glyph_sets,
        )?));
        out.push(LaidItem::Text(LaidLine::shaped(
            FaceId::MonoRegular,
            &rule,
            font_size,
            leading,
            glyph_sets,
        )?));
    }
    out.push(LaidItem::Text(LaidLine::gap(10.0)));
    Ok(())
}

fn push_styled_runs(
    out: &mut Vec<LaidItem>,
    runs: &[TextRun],
    metrics: &ProfileMetrics,
    glyph_sets: &mut GlyphSets,
    layout: RunLayout,
) -> Result<(), WeaveError> {
    if runs.is_empty() {
        return Ok(());
    }

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
        }));
    };

    for run in runs {
        let face = resolve_face(run.style, metrics, layout.mode);
        let mut remaining = run.text.as_str();
        while !remaining.is_empty() {
            let (chunk, rest) = next_wrap_chunk(remaining);
            remaining = rest;
            let chunk = chunk.trim_end();
            if chunk.is_empty() {
                continue;
            }
            let glyphs = shape_text(face, chunk, layout.font_size)?;
            let w = shaped_width(&glyphs);
            if current_width + w > max_width && !current_spans.is_empty() {
                flush_line(&mut current_spans, out, false);
                current_width = 0.0;
            }
            if w > max_width && current_spans.is_empty() {
                // Hard-break tokens wider than the content box (URLs, long code).
                for piece in hard_break_text(face, chunk, layout.font_size, max_width)? {
                    let glyphs = shape_text(face, &piece, layout.font_size)?;
                    let set = glyph_sets.entry(face).or_default();
                    collect_glyph_set(face, &piece, set);
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
            collect_glyph_set(face, chunk, set);
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
    if layout.gap_after > 0.0 {
        out.push(LaidItem::Text(LaidLine::gap(layout.gap_after)));
    }
    Ok(())
}

/// Split `text` into pieces that each fit within `max_width` points.
fn hard_break_text(
    face: FaceId,
    text: &str,
    font_size: f32,
    max_width: f32,
) -> Result<Vec<String>, WeaveError> {
    let mut pieces = Vec::new();
    let mut buf = String::new();
    for ch in text.chars() {
        let mut trial = buf.clone();
        trial.push(ch);
        let tw = shaped_width(&shape_text(face, &trial, font_size)?);
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

#[derive(Debug, Clone, Copy)]
enum ForcedBreak {
    None,
    Always,
}

fn paginate_items(
    segments: &[(ForcedBreak, Vec<LaidItem>)],
    max_y_span: f32,
) -> Vec<Vec<LaidItem>> {
    let max_y_span = (max_y_span - 18.0).max(72.0);
    let mut pages: Vec<Vec<LaidItem>> = Vec::new();
    let mut current: Vec<LaidItem> = Vec::new();
    let mut used = 0.0_f32;

    let flush = |pages: &mut Vec<Vec<LaidItem>>, current: &mut Vec<LaidItem>, used: &mut f32| {
        if !current.is_empty() {
            pages.push(std::mem::take(current));
            *used = 0.0;
        } else if pages.is_empty() {
            pages.push(Vec::new());
        }
    };

    for (forced, items) in segments {
        if matches!(forced, ForcedBreak::Always) && !current.is_empty() {
            flush(&mut pages, &mut current, &mut used);
        }
        for item in items {
            let h = item.height();
            if used + h > max_y_span && !current.is_empty() {
                let mut peeled: Vec<LaidItem> = Vec::new();
                while current.last().is_some_and(LaidItem::glue_after) {
                    let peeled_item = current.pop().expect("last");
                    used -= peeled_item.height();
                    peeled.push(peeled_item);
                }
                if current.is_empty() {
                    while let Some(l) = peeled.pop() {
                        used += l.height();
                        current.push(l);
                    }
                    flush(&mut pages, &mut current, &mut used);
                } else {
                    flush(&mut pages, &mut current, &mut used);
                    while let Some(l) = peeled.pop() {
                        used += l.height();
                        current.push(l);
                    }
                }
                if used + h > max_y_span && !current.is_empty() {
                    flush(&mut pages, &mut current, &mut used);
                }
            }
            used += h;
            current.push(item.clone());
        }
    }

    if current.is_empty() && pages.is_empty() {
        pages.push(Vec::new());
    } else if !current.is_empty() {
        pages.push(current);
    }

    pages
}

fn build_page_content(
    items: &[LaidItem],
    metrics: &ProfileMetrics,
    page_no: usize,
    page_count: usize,
    subsets: &SubsetMap,
) -> Result<Vec<u8>, WeaveError> {
    let mut content = Content::new();
    let mut y = metrics.page_h - metrics.margin;

    for item in items {
        match item {
            LaidItem::Text(line) => {
                y -= line.leading;
                if y < metrics.margin + 18.0 {
                    break;
                }
                if line.spans.is_empty() {
                    continue;
                }
                content.begin_text();
                let mut x = metrics.margin + line.indent;
                for span in &line.spans {
                    content.set_font(Name(resource_name(span.face)), span.font_size);
                    content.set_text_matrix([1.0, 0.0, 0.0, 1.0, x, y]);
                    let encoded = encode_gids(&span.glyphs);
                    content.show(Str(&encoded));
                    x += shaped_width(&span.glyphs);
                }
                content.end_text();
            }
            LaidItem::Image {
                img_idx,
                width,
                height,
            } => {
                y -= *height;
                if y < metrics.margin + 18.0 {
                    break;
                }
                let name = image_resource_name(*img_idx);
                content.save_state();
                content.transform([*width, 0.0, 0.0, *height, metrics.margin, y]);
                content.x_object(Name(&name));
                content.restore_state();
                y -= 8.0;
            }
        }
    }

    let footer = format!("{page_no} / {page_count}");
    let mut footer_glyphs = shape_text(FaceId::SansRegular, &footer, 9.0)?;
    if let Some(subset) = subsets.get(&FaceId::SansRegular) {
        for g in &mut footer_glyphs {
            *g = subset.remap_glyph(*g);
        }
    }
    let footer_w = shaped_width(&footer_glyphs);
    let footer_y = metrics.margin * 0.45;
    let footer_x = (metrics.page_w - footer_w) / 2.0;
    content.begin_text();
    content.set_font(Name(resource_name(FaceId::SansRegular)), 9.0);
    content.set_text_matrix([1.0, 0.0, 0.0, 1.0, footer_x, footer_y]);
    content.show(Str(&encode_gids(&footer_glyphs)));
    content.end_text();

    Ok(content.finish().into_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{InlineStyle, PrintMeta, PrintProfileId, TableRow};
    use image::{ImageBuffer, ImageFormat, Rgb};

    fn hello_doc() -> PrintDocument {
        PrintDocument {
            meta: PrintMeta {
                title: "Hello Weave".into(),
                doc_kind: "note".into(),
                language: Some("en".into()),
                source_doc_id: None,
            },
            profile: PrintProfileId::print_v0(),
            blocks: vec![
                PrintBlock::Heading {
                    level: 1,
                    runs: vec![TextRun {
                        text: "Hello, ariadnes-weave".into(),
                        style: InlineStyle::default(),
                    }],
                    break_before: BreakHint::None,
                },
                PrintBlock::Paragraph {
                    runs: vec![TextRun::plain(
                        "Owned print IR to PDF with Liberation Sans + rustybuzz.",
                    )],
                },
            ],
        }
    }

    fn tiny_png_bytes() -> Vec<u8> {
        let img: ImageBuffer<Rgb<u8>, Vec<u8>> =
            ImageBuffer::from_fn(32, 24, |x, y| Rgb([x as u8 * 7, y as u8 * 9, 180]));
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, ImageFormat::Png)
            .expect("encode png");
        buf.into_inner()
    }

    #[test]
    fn emits_pdf_magic() {
        let bytes = emit_pdf(&hello_doc()).expect("emit");
        assert!(bytes.starts_with(b"%PDF-"));
        assert!(bytes.windows(5).any(|w| w == b"%%EOF"));
    }

    #[test]
    fn subsetted_hello_pdf_is_compact() {
        let bytes = emit_pdf(&hello_doc()).expect("emit");
        // Full Liberation Sans alone is ~400KB; subset prose should stay well under that.
        assert!(
            bytes.len() < 80_000,
            "expected subsetted PDF < 80KB, got {}",
            bytes.len()
        );
    }

    #[test]
    fn hard_breaks_overlong_token() {
        let long = "A".repeat(80);
        let doc = PrintDocument {
            meta: PrintMeta {
                title: "Long".into(),
                doc_kind: "note".into(),
                language: None,
                source_doc_id: None,
            },
            profile: PrintProfileId::print_v0(),
            blocks: vec![PrintBlock::Paragraph {
                runs: vec![TextRun::plain(long)],
            }],
        };
        let bytes = emit_pdf(&doc).expect("emit overlong");
        assert!(bytes.starts_with(b"%PDF-"));
    }

    #[test]
    fn rejects_unknown_profile() {
        let mut doc = hello_doc();
        doc.profile = PrintProfileId {
            name: "manuscript".into(),
            version: 1,
        };
        assert!(matches!(
            emit_pdf(&doc).unwrap_err(),
            WeaveError::UnsupportedProfile { .. }
        ));
    }

    #[test]
    fn accepts_manuscript_v0() {
        let mut doc = hello_doc();
        doc.profile = PrintProfileId::manuscript_v0();
        assert!(emit_pdf(&doc).expect("emit").starts_with(b"%PDF-"));
    }

    #[test]
    fn styled_runs_embed_bold_font() {
        let mut doc = hello_doc();
        doc.blocks.push(PrintBlock::Paragraph {
            runs: vec![
                TextRun::plain("plain "),
                TextRun {
                    text: "bold".into(),
                    style: InlineStyle {
                        strong: true,
                        ..InlineStyle::default()
                    },
                },
            ],
        });
        let bytes = emit_pdf(&doc).expect("emit");
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("LiberationSans-Bold"));
        assert!(s.contains("/Subtype /Type0") || s.contains("/Subtype/Type0"));
    }

    #[test]
    fn emits_unicode_em_dash() {
        let doc = PrintDocument {
            meta: PrintMeta {
                title: "Dash".into(),
                doc_kind: "note".into(),
                language: None,
                source_doc_id: None,
            },
            profile: PrintProfileId::print_v0(),
            blocks: vec![PrintBlock::Paragraph {
                runs: vec![TextRun::plain("alpha — omega")],
            }],
        };
        let bytes = emit_pdf(&doc).expect("emit unicode");
        assert!(bytes.starts_with(b"%PDF-"));
    }

    #[test]
    fn table_renders_ascii_grid() {
        let doc = PrintDocument {
            meta: PrintMeta {
                title: "T".into(),
                doc_kind: "note".into(),
                language: None,
                source_doc_id: None,
            },
            profile: PrintProfileId::print_v0(),
            blocks: vec![PrintBlock::Table {
                rows: vec![
                    TableRow {
                        cells: vec!["A".into(), "B".into()],
                    },
                    TableRow {
                        cells: vec!["1".into(), "2".into()],
                    },
                ],
            }],
        };
        let bytes = emit_pdf(&doc).expect("emit");
        assert!(bytes.starts_with(b"%PDF-"));
        // Footer present as shaped text; ToUnicode should map digits.
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("LiberationSans") || s.contains("LiberationMono"));
    }

    #[test]
    fn figure_png_embeds_xobject() {
        let png = tiny_png_bytes();
        let doc = PrintDocument {
            meta: PrintMeta {
                title: "Fig".into(),
                doc_kind: "note".into(),
                language: None,
                source_doc_id: None,
            },
            profile: PrintProfileId::print_v0(),
            blocks: vec![
                PrintBlock::Heading {
                    level: 1,
                    runs: vec![TextRun::plain("With figure")],
                    break_before: BreakHint::None,
                },
                PrintBlock::Figure {
                    image: PrintImage {
                        bytes: png,
                        media_type: "image/png".into(),
                        width_px: Some(32),
                        height_px: Some(24),
                    },
                    alt: "swatch".into(),
                    caption: vec![TextRun::plain("A tiny PNG.")],
                    placement: FigurePlacement::Flow,
                },
            ],
        };
        let bytes = emit_pdf(&doc).expect("emit");
        assert!(bytes.starts_with(b"%PDF-"));
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("/Subtype /Image") || s.contains("/Subtype/Image"));
        std::fs::create_dir_all("tmp").ok();
        std::fs::write("tmp/figure_sample.pdf", &bytes).ok();
    }
}
