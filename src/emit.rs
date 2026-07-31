//! Helvetica-family PDF emit from print IR.
//!
//! Still MVP: no TTF embedding / rustybuzz shaping. Real manuscript policy and
//! rich layout deepen in THI-294 / THI-291.

use pdf_writer::{Content, Name, Pdf, Rect, Ref, Str, TextStr};

use crate::error::WeaveError;
use crate::ir::{BreakHint, InlineStyle, PrintBlock, PrintDocument, TextRun};
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
///
/// Accepts `print@0` / `manuscript@0`. Renders prose + placeholders for
/// table/figure/math/slide. Honors forced page breaks and a simple
/// keep-with-next glue for headings.
pub fn emit_pdf(doc: &PrintDocument) -> Result<Vec<u8>, WeaveError> {
    let metrics = profile::resolve_metrics(&doc.profile)?;
    let lines = collect_lines(doc, &metrics)?;
    let pages = paginate_lines(&lines, metrics.content_height());

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

    for (page_id, content_id, page_lines) in page_ids
        .iter()
        .copied()
        .zip(content_ids.iter().copied())
        .zip(pages.iter())
        .map(|((p, c), l)| (p, c, l))
    {
        {
            let mut page = pdf.page(page_id);
            page.media_box(Rect::new(0.0, 0.0, metrics.page_w, metrics.page_h));
            page.parent(page_tree_id);
            page.contents(content_id);
            let mut resources = page.resources();
            let mut fonts = resources.fonts();
            fonts.pair(Name(Face::Regular.resource_name()), font_regular);
            fonts.pair(Name(Face::Bold.resource_name()), font_bold);
            fonts.pair(Name(Face::Oblique.resource_name()), font_oblique);
            fonts.pair(Name(Face::BoldOblique.resource_name()), font_bold_oblique);
            fonts.pair(Name(Face::Courier.resource_name()), font_courier);
        }

        let content_bytes = build_page_content(page_lines, &metrics);
        pdf.stream(content_id, &content_bytes);
    }

    let info_id = Ref::new(next_id);
    pdf.document_info(info_id)
        .title(TextStr(&doc.meta.title))
        .creator(TextStr("ariadnes-weave"))
        .producer(TextStr(&format!("ariadnes-weave {}", crate::VERSION)));

    Ok(pdf.finish())
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
    /// If true, this line should stay with the following line when possible.
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

fn collect_lines(
    doc: &PrintDocument,
    metrics: &ProfileMetrics,
) -> Result<Vec<(ForcedBreak, Vec<LaidLine>)>, WeaveError> {
    let mut segments: Vec<(ForcedBreak, Vec<LaidLine>)> = vec![(ForcedBreak::None, Vec::new())];

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
                // Glue H1/H2 and explicit KeepWithNext to the following prose line.
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
                    seg.1.push(LaidLine::plain(line, Face::Courier, font_size, leading));
                }
                if text.ends_with('\n') || text.is_empty() {
                    // keep visual gap even for trailing newline / empty
                }
                seg.1.push(LaidLine::gap(10.0));
            }
            PrintBlock::List { ordered, items } => {
                let seg = segments.last_mut().expect("segment");
                push_list_lines(&mut seg.1, *ordered, items, 0, metrics)?;
            }
            PrintBlock::Table { rows } => {
                let seg = segments.last_mut().expect("segment");
                let summary = format!(
                    "[table: {} row{}]",
                    rows.len(),
                    if rows.len() == 1 { "" } else { "s" }
                );
                seg.1.push(LaidLine::plain(
                    summary,
                    Face::Oblique,
                    metrics.body_size,
                    metrics.body_leading,
                ));
                seg.1.push(LaidLine::gap(10.0));
            }
            PrintBlock::Figure { alt, caption, .. } => {
                let seg = segments.last_mut().expect("segment");
                let label = if alt.is_empty() {
                    "[figure]".into()
                } else {
                    format!("[figure: {alt}]")
                };
                ensure_encodable(&label)?;
                seg.1.push(LaidLine::plain(
                    label,
                    Face::Oblique,
                    metrics.body_size,
                    metrics.body_leading,
                ));
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
                    seg.1.push(LaidLine::gap(10.0));
                }
            }
            PrintBlock::Math { display, latex } => {
                let seg = segments.last_mut().expect("segment");
                let _ = display; // layout differs later; placeholder text is shared for now
                let line = format!("[math] {latex}");
                ensure_encodable(&line)?;
                seg.1.push(LaidLine::plain(
                    line,
                    Face::Courier,
                    metrics.code_size,
                    metrics.code_size * 1.25,
                ));
                seg.1.push(LaidLine::gap(10.0));
            }
            PrintBlock::Slide { layout_id, regions } => {
                let seg = segments.last_mut().expect("segment");
                let line = format!("[slide:{layout_id} regions={}]", regions.len());
                ensure_encodable(&line)?;
                seg.1.push(LaidLine::plain(
                    line,
                    Face::Oblique,
                    metrics.body_size,
                    metrics.body_leading,
                ));
                segments.push((ForcedBreak::Always, Vec::new()));
            }
        }
    }

    Ok(segments)
}

fn push_styled_runs(
    seg: &mut Vec<LaidLine>,
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

    let flush_line = |spans: &mut Vec<LaidSpan>,
                      out: &mut Vec<LaidLine>,
                      glue: bool|
     -> Result<(), WeaveError> {
        if spans.is_empty() {
            return Ok(());
        }
        out.push(LaidLine {
            spans: std::mem::take(spans),
            leading,
            glue_after: glue,
        });
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
                flush_line(&mut current_spans, seg, false)?;
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
            // Ensure we advance at least one char if no whitespace break.
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
                flush_line(&mut current_spans, seg, false)?;
                current_width = 0.0;
            }
        }
    }

    flush_line(&mut current_spans, seg, glue_last_content)?;
    if gap_after > 0.0 {
        // Gap itself should not break keep-with-next; glue stays on last content line.
        seg.push(LaidLine::gap(gap_after));
    }
    Ok(())
}

fn push_list_lines(
    seg: &mut Vec<LaidLine>,
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
            seg,
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
                } => push_list_lines(seg, *child_ordered, child_items, depth + 1, metrics)?,
                other => {
                    return Err(WeaveError::UnsupportedBlock(block_name(other)));
                }
            }
        }
    }
    seg.push(LaidLine::gap(8.0));
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

fn paginate_lines(segments: &[(ForcedBreak, Vec<LaidLine>)], max_y_span: f32) -> Vec<Vec<LaidLine>> {
    let mut pages: Vec<Vec<LaidLine>> = Vec::new();
    let mut current: Vec<LaidLine> = Vec::new();
    let mut used = 0.0_f32;

    let flush = |pages: &mut Vec<Vec<LaidLine>>, current: &mut Vec<LaidLine>, used: &mut f32| {
        if !current.is_empty() {
            pages.push(std::mem::take(current));
            *used = 0.0;
        } else if pages.is_empty() {
            pages.push(Vec::new());
        }
    };

    for (forced, lines) in segments {
        if matches!(forced, ForcedBreak::Always) && !current.is_empty() {
            flush(&mut pages, &mut current, &mut used);
        }
        for line in lines {
            if used + line.leading > max_y_span && !current.is_empty() {
                // Peel trailing glue_after chain so headings aren't orphaned.
                let mut peeled: Vec<LaidLine> = Vec::new();
                while current.last().is_some_and(|l| l.glue_after) {
                    let peeled_line = current.pop().expect("last");
                    used -= peeled_line.leading;
                    peeled.push(peeled_line);
                }
                // If everything was glue, force a break anyway.
                if current.is_empty() {
                    while let Some(l) = peeled.pop() {
                        used += l.leading;
                        current.push(l);
                    }
                    flush(&mut pages, &mut current, &mut used);
                } else {
                    flush(&mut pages, &mut current, &mut used);
                    while let Some(l) = peeled.pop() {
                        used += l.leading;
                        current.push(l);
                    }
                }
                // After peel+flush, line might still not fit on a fresh page — place it.
                if used + line.leading > max_y_span && !current.is_empty() {
                    flush(&mut pages, &mut current, &mut used);
                }
            }
            used += line.leading;
            current.push(line.clone());
        }
    }

    if current.is_empty() && pages.is_empty() {
        pages.push(Vec::new());
    } else if !current.is_empty() {
        pages.push(current);
    }

    pages
}

fn build_page_content(lines: &[LaidLine], metrics: &ProfileMetrics) -> Vec<u8> {
    let mut content = Content::new();
    let mut y = metrics.page_h - metrics.margin;

    content.begin_text();
    for line in lines {
        y -= line.leading;
        if y < metrics.margin {
            break;
        }
        if line.spans.is_empty() {
            continue;
        }
        let mut x = metrics.margin;
        for (i, span) in line.spans.iter().enumerate() {
            content.set_font(Name(span.face.resource_name()), span.font_size);
            if i == 0 {
                content.set_text_matrix([1.0, 0.0, 0.0, 1.0, x, y]);
            } else {
                // Relative move in text space from previous show end — use absolute matrix.
                content.set_text_matrix([1.0, 0.0, 0.0, 1.0, x, y]);
            }
            let bytes = winansi_bytes(&span.text);
            content.show(Str(&bytes));
            x += span.text.len() as f32 * span.font_size * metrics.char_width_factor;
        }
    }
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
    use crate::ir::{InlineStyle, PrintMeta, PrintProfileId};

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
                TextRun::plain(" and "),
                TextRun {
                    text: "emph".into(),
                    style: InlineStyle {
                        emphasis: true,
                        ..InlineStyle::default()
                    },
                },
            ],
        });
        let bytes = emit_pdf(&doc).expect("emit");
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("Helvetica-Bold"), "missing bold face");
        assert!(s.contains("Helvetica-Oblique"), "missing oblique face");
    }

    #[test]
    fn placeholders_for_rich_blocks() {
        let doc = PrintDocument {
            meta: PrintMeta {
                title: "Rich".into(),
                doc_kind: "note".into(),
                language: None,
                source_doc_id: None,
            },
            profile: PrintProfileId::print_v0(),
            blocks: vec![
                PrintBlock::Table {
                    rows: vec![crate::ir::TableRow {
                        cells: vec!["a".into(), "b".into()],
                    }],
                },
                PrintBlock::Math {
                    display: true,
                    latex: "E=mc^2".into(),
                },
            ],
        };
        let bytes = emit_pdf(&doc).expect("emit");
        assert!(bytes.starts_with(b"%PDF-"));
    }
}
