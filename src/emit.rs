//! Helvetica-family PDF emit from print IR.
//!
//! Still MVP: no TTF embedding / rustybuzz shaping. Real manuscript policy and
//! rich layout deepen in THI-294 / THI-291.

use pdf_writer::{Content, Name, Pdf, Rect, Ref, Str, TextStr};

use crate::error::WeaveError;
use crate::image_prep::{PreparedImage, prepare_image};
use crate::ir::{
    BreakHint, FigurePlacement, InlineStyle, PrintBlock, PrintDocument, PrintImage, TextRun,
};
use crate::profile::{self, ProfileMetrics};

/// Standard Type-1 face used for a laid span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Face {
    Regular,
    Bold,
    Oblique,
    BoldOblique,
    Courier,
}

impl Face {
    fn from_style(style: &InlineStyle) -> Self {
        if style.code {
            return Self::Courier;
        }
        match (style.strong, style.emphasis) {
            (true, true) => Self::BoldOblique,
            (true, false) => Self::Bold,
            (false, true) => Self::Oblique,
            (false, false) => Self::Regular,
        }
    }

    fn pdf_name(self) -> &'static [u8] {
        match self {
            Self::Regular => b"Helvetica",
            Self::Bold => b"Helvetica-Bold",
            Self::Oblique => b"Helvetica-Oblique",
            Self::BoldOblique => b"Helvetica-BoldOblique",
            Self::Courier => b"Courier",
        }
    }

    fn resource_name(self) -> &'static [u8] {
        match self {
            Self::Regular => b"F1",
            Self::Bold => b"F2",
            Self::Oblique => b"F3",
            Self::BoldOblique => b"F4",
            Self::Courier => b"F5",
        }
    }
}

/// Emit PDF bytes from a print document.
pub fn emit_pdf(doc: &PrintDocument) -> Result<Vec<u8>, WeaveError> {
    let metrics = profile::resolve_metrics(&doc.profile)?;
    let (segments, images) = collect_layout(doc, &metrics)?;
    let pages = paginate_items(&segments, metrics.content_height());

    let mut pdf = Pdf::new();
    pdf.set_version(1, 7);

    let catalog_id = Ref::new(1);
    let page_tree_id = Ref::new(2);
    let font_regular = Ref::new(3);
    let font_bold = Ref::new(4);
    let font_oblique = Ref::new(5);
    let font_bold_oblique = Ref::new(6);
    let font_courier = Ref::new(7);
    let mut next_id = 8_i32;

    // Image (+ optional mask) refs.
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

    pdf.type1_font(font_regular)
        .base_font(Name(Face::Regular.pdf_name()));
    pdf.type1_font(font_bold)
        .base_font(Name(Face::Bold.pdf_name()));
    pdf.type1_font(font_oblique)
        .base_font(Name(Face::Oblique.pdf_name()));
    pdf.type1_font(font_bold_oblique)
        .base_font(Name(Face::BoldOblique.pdf_name()));
    pdf.type1_font(font_courier)
        .base_font(Name(Face::Courier.pdf_name()));

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
                fonts.pair(Name(Face::Regular.resource_name()), font_regular);
                fonts.pair(Name(Face::Bold.resource_name()), font_bold);
                fonts.pair(Name(Face::Oblique.resource_name()), font_oblique);
                fonts.pair(Name(Face::BoldOblique.resource_name()), font_bold_oblique);
                fonts.pair(Name(Face::Courier.resource_name()), font_courier);
            }
            if !used_images.is_empty() {
                let mut xobjs = resources.x_objects();
                for idx in &used_images {
                    let name = image_resource_name(*idx);
                    xobjs.pair(Name(&name), image_refs[*idx].0);
                }
            }
        }

        let content_bytes = build_page_content(
            page_items,
            &metrics,
            page_idx + 1,
            page_count,
            &image_refs,
        );
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

#[derive(Debug, Clone)]
struct LaidSpan {
    text: String,
    face: Face,
    font_size: f32,
}

#[derive(Debug, Clone)]
struct LaidLine {
    spans: Vec<LaidSpan>,
    leading: f32,
    glue_after: bool,
}

impl LaidLine {
    fn gap(leading: f32) -> Self {
        Self {
            spans: Vec::new(),
            leading,
            glue_after: false,
        }
    }

    fn plain(text: impl Into<String>, face: Face, font_size: f32, leading: f32) -> Self {
        Self {
            spans: vec![LaidSpan {
                text: text.into(),
                face,
                font_size,
            }],
            leading,
            glue_after: false,
        }
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
type LayoutDoc = (Vec<LayoutSegment>, Vec<PreparedImage>);

fn collect_layout(doc: &PrintDocument, metrics: &ProfileMetrics) -> Result<LayoutDoc, WeaveError> {
    let mut segments: Vec<(ForcedBreak, Vec<LaidItem>)> = vec![(ForcedBreak::None, Vec::new())];
    let mut images: Vec<PreparedImage> = Vec::new();

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
                if matches!(break_before, BreakHint::Page | BreakHint::PageAlways) {
                    segments.push((ForcedBreak::Always, Vec::new()));
                }
                let font_size = profile::heading_size(*level, metrics);
                let glue =
                    matches!(break_before, BreakHint::KeepWithNext) || *level <= 2;
                let seg = segments.last_mut().expect("segment");
                push_styled_runs(
                    &mut seg.1,
                    runs,
                    font_size,
                    font_size * 1.35,
                    8.0,
                    glue,
                    metrics,
                )?;
            }
            PrintBlock::Paragraph { runs } => {
                let seg = segments.last_mut().expect("segment");
                push_styled_runs(
                    &mut seg.1,
                    runs,
                    metrics.body_size,
                    metrics.body_leading,
                    10.0,
                    false,
                    metrics,
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
                    metrics.body_size,
                    metrics.body_leading,
                    10.0,
                    false,
                    metrics,
                )?;
            }
            PrintBlock::Code { lang: _, text } => {
                let seg = segments.last_mut().expect("segment");
                let font_size = metrics.code_size;
                let leading = font_size * 1.25;
                for line in text.lines() {
                    ensure_encodable(line)?;
                    seg.1.push(LaidItem::Text(LaidLine::plain(
                        line,
                        Face::Courier,
                        font_size,
                        leading,
                    )));
                }
                seg.1.push(LaidItem::Text(LaidLine::gap(10.0)));
            }
            PrintBlock::List { ordered, items } => {
                let seg = segments.last_mut().expect("segment");
                push_list_lines(&mut seg.1, *ordered, items, 0, metrics)?;
            }
            PrintBlock::Table { rows } => {
                let seg = segments.last_mut().expect("segment");
                push_table_lines(&mut seg.1, rows, metrics)?;
            }
            PrintBlock::Figure {
                image,
                alt,
                caption,
                placement,
            } => {
                let _ = placement; // FloatNear treated as Flow for now.
                let _ = FigurePlacement::Flow;
                push_figure(
                    &mut segments,
                    &mut images,
                    image,
                    alt,
                    caption,
                    metrics,
                )?;
            }
            PrintBlock::Math { display: _, latex } => {
                let seg = segments.last_mut().expect("segment");
                let line = format!("[math] {latex}");
                ensure_encodable(&line)?;
                seg.1.push(LaidItem::Text(LaidLine::plain(
                    line,
                    Face::Courier,
                    metrics.code_size,
                    metrics.code_size * 1.25,
                )));
                seg.1.push(LaidItem::Text(LaidLine::gap(10.0)));
            }
            PrintBlock::Slide { layout_id, regions } => {
                let seg = segments.last_mut().expect("segment");
                let line = format!("[slide:{layout_id} regions={}]", regions.len());
                ensure_encodable(&line)?;
                seg.1.push(LaidItem::Text(LaidLine::plain(
                    line,
                    Face::Oblique,
                    metrics.body_size,
                    metrics.body_leading,
                )));
                segments.push((ForcedBreak::Always, Vec::new()));
            }
        }
    }

    Ok((segments, images))
}

fn push_figure(
    segments: &mut [LayoutSegment],
    images: &mut Vec<PreparedImage>,
    image: &PrintImage,
    alt: &str,
    caption: &[TextRun],
    metrics: &ProfileMetrics,
) -> Result<(), WeaveError> {
    let prepared = match prepare_image(image) {
        Ok(p) => p,
        Err(_) => {
            // Fallback placeholder when decode fails.
            let seg = segments.last_mut().expect("segment");
            let label = if alt.is_empty() {
                "[figure]".into()
            } else {
                format!("[figure: {alt}]")
            };
            ensure_encodable(&label)?;
            seg.1.push(LaidItem::Text(LaidLine::plain(
                label,
                Face::Oblique,
                metrics.body_size,
                metrics.body_leading,
            )));
            if !caption.is_empty() {
                push_styled_runs(
                    &mut seg.1,
                    caption,
                    metrics.body_size,
                    metrics.body_leading,
                    10.0,
                    false,
                    metrics,
                )?;
            } else {
                seg.1.push(LaidItem::Text(LaidLine::gap(10.0)));
            }
            return Ok(());
        }
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
    if !caption.is_empty() {
        push_styled_runs(
            &mut seg.1,
            caption,
            metrics.body_size,
            metrics.body_leading,
            10.0,
            false,
            metrics,
        )?;
    } else {
        seg.1.push(LaidItem::Text(LaidLine::gap(6.0)));
    }
    Ok(())
}

fn push_table_lines(
    out: &mut Vec<LaidItem>,
    rows: &[crate::ir::TableRow],
    metrics: &ProfileMetrics,
) -> Result<(), WeaveError> {
    if rows.is_empty() {
        out.push(LaidItem::Text(LaidLine::plain(
            "[table]",
            Face::Oblique,
            metrics.body_size,
            metrics.body_leading,
        )));
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
    ensure_encodable(&rule)?;
    out.push(LaidItem::Text(LaidLine::plain(
        rule.clone(),
        Face::Courier,
        font_size,
        leading,
    )));

    for row in rows {
        let mut line = String::from("|");
        for (i, width) in widths.iter().enumerate() {
            let cell = row.cells.get(i).map(String::as_str).unwrap_or("");
            line.push(' ');
            line.push_str(cell);
            line.push_str(&" ".repeat(width.saturating_sub(cell.len())));
            line.push_str(" |");
        }
        ensure_encodable(&line)?;
        out.push(LaidItem::Text(LaidLine::plain(
            line,
            Face::Courier,
            font_size,
            leading,
        )));
        out.push(LaidItem::Text(LaidLine::plain(
            rule.clone(),
            Face::Courier,
            font_size,
            leading,
        )));
    }
    out.push(LaidItem::Text(LaidLine::gap(10.0)));
    Ok(())
}

fn push_styled_runs(
    out: &mut Vec<LaidItem>,
    runs: &[TextRun],
    font_size: f32,
    leading: f32,
    gap_after: f32,
    glue_last_content: bool,
    metrics: &ProfileMetrics,
) -> Result<(), WeaveError> {
    if runs.is_empty() {
        return Ok(());
    }

    let max_width = metrics.content_width();
    let mut current_spans: Vec<LaidSpan> = Vec::new();
    let mut current_width = 0.0_f32;

    let flush_line =
        |spans: &mut Vec<LaidSpan>, dest: &mut Vec<LaidItem>, glue: bool| -> Result<(), WeaveError> {
            if spans.is_empty() {
                return Ok(());
            }
            dest.push(LaidItem::Text(LaidLine {
                spans: std::mem::take(spans),
                leading,
                glue_after: glue,
            }));
            Ok(())
        };

    for run in runs {
        ensure_encodable(&run.text)?;
        let face = Face::from_style(&run.style);
        let mut remaining = run.text.as_str();
        while !remaining.is_empty() {
            let avail_chars = ((max_width - current_width)
                / (font_size * metrics.char_width_factor))
                .floor() as usize;
            if avail_chars == 0 && !current_spans.is_empty() {
                flush_line(&mut current_spans, out, false)?;
                current_width = 0.0;
                continue;
            }
            let take = avail_chars.max(1).min(remaining.len());
            let mut split_at = take;
            if take < remaining.len()
                && let Some(rel) = remaining[..split_at].rfind(char::is_whitespace)
                && rel > 0
            {
                split_at = rel;
            }
            if split_at == 0 {
                split_at = remaining.chars().next().map(|c| c.len_utf8()).unwrap_or(1);
            }
            let (chunk, rest) = remaining.split_at(split_at);
            let chunk = chunk.trim_end();
            remaining = rest.trim_start();
            if !chunk.is_empty() {
                let w = chunk.len() as f32 * font_size * metrics.char_width_factor;
                current_spans.push(LaidSpan {
                    text: chunk.to_owned(),
                    face,
                    font_size,
                });
                current_width += w;
            }
            if !remaining.is_empty() {
                flush_line(&mut current_spans, out, false)?;
                current_width = 0.0;
            }
        }
    }

    flush_line(&mut current_spans, out, glue_last_content)?;
    if gap_after > 0.0 {
        out.push(LaidItem::Text(LaidLine::gap(gap_after)));
    }
    Ok(())
}

fn push_list_lines(
    out: &mut Vec<LaidItem>,
    ordered: bool,
    items: &[crate::ir::ListItem],
    depth: usize,
    metrics: &ProfileMetrics,
) -> Result<(), WeaveError> {
    for (i, item) in items.iter().enumerate() {
        let marker = if ordered {
            format!("{}. ", i + 1)
        } else {
            "- ".into()
        };
        let indent = "  ".repeat(depth);
        let mut runs = vec![TextRun::plain(format!("{indent}{marker}"))];
        runs.extend(item.runs.iter().cloned());
        push_styled_runs(
            out,
            &runs,
            metrics.body_size,
            metrics.body_size * 1.35,
            0.0,
            false,
            metrics,
        )?;
        for child in &item.children {
            match child {
                PrintBlock::List {
                    ordered: child_ordered,
                    items: child_items,
                } => push_list_lines(out, *child_ordered, child_items, depth + 1, metrics)?,
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
    // Reserve footer band for page numbers.
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
    _image_refs: &[(Ref, Option<Ref>)],
) -> Vec<u8> {
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
                let mut x = metrics.margin;
                for span in &line.spans {
                    content.set_font(Name(span.face.resource_name()), span.font_size);
                    content.set_text_matrix([1.0, 0.0, 0.0, 1.0, x, y]);
                    content.show(Str(&winansi_bytes(&span.text)));
                    x += span.text.len() as f32 * span.font_size * metrics.char_width_factor;
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

    // Page number footer.
    let footer = format!("{page_no} / {page_count}");
    let footer_y = metrics.margin * 0.45;
    content.begin_text();
    content.set_font(Name(Face::Regular.resource_name()), 9.0);
    let footer_w = footer.len() as f32 * 9.0 * metrics.char_width_factor;
    let footer_x = (metrics.page_w - footer_w) / 2.0;
    content.set_text_matrix([1.0, 0.0, 0.0, 1.0, footer_x, footer_y]);
    content.show(Str(footer.as_bytes()));
    content.end_text();

    content.finish().into_vec()
}

fn ensure_encodable(text: &str) -> Result<(), WeaveError> {
    for ch in text.chars() {
        if !is_winansi_char(ch) {
            return Err(WeaveError::UnencodableText(text.to_owned()));
        }
    }
    Ok(())
}

fn is_winansi_char(ch: char) -> bool {
    ch == '\t' || ch == '\n' || ch == '\r' || (ch as u32 >= 0x20 && ch as u32 <= 0x7E)
}

fn winansi_bytes(text: &str) -> Vec<u8> {
    text.chars()
        .filter(|ch| *ch != '\n' && *ch != '\r')
        .map(|ch| if ch == '\t' { b' ' } else { ch as u8 })
        .collect()
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
                        "Owned print IR to PDF - MVP emit with Helvetica.",
                    )],
                },
            ],
        }
    }

    fn tiny_png_bytes() -> Vec<u8> {
        let img: ImageBuffer<Rgb<u8>, Vec<u8>> =
            ImageBuffer::from_fn(32, 24, |x, y| Rgb([x as u8 * 7, y as u8 * 9, 180]));
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, ImageFormat::Png).expect("encode png");
        buf.into_inner()
    }

    #[test]
    fn emits_pdf_magic() {
        let bytes = emit_pdf(&hello_doc()).expect("emit");
        assert!(bytes.starts_with(b"%PDF-"));
        assert!(bytes.windows(5).any(|w| w == b"%%EOF"));
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
        assert!(s.contains("Helvetica-Bold"));
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
        // Footer present.
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("1 / 1"));
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
