//! MVP PDF emit: Helvetica + single-page (or simple multi-page) stacking.
//!
//! No real line-breaking, font embedding, or manuscript policy — that is THI-294.

use pdf_writer::{Content, Name, Pdf, Rect, Ref, Str, TextStr};

use crate::error::WeaveError;
use crate::ir::{BreakHint, PrintBlock, PrintDocument, PrintProfileId, TextRun};

/// A4 width in PDF points.
const PAGE_W: f32 = 595.0;
/// A4 height in PDF points.
const PAGE_H: f32 = 842.0;
/// Default margin (1 inch).
const MARGIN: f32 = 72.0;
/// Approximate Helvetica average char width factor × font size.
const CHAR_WIDTH_FACTOR: f32 = 0.5;

/// Emit PDF bytes from a print document (MVP).
///
/// Accepts `print@0` or `manuscript@0`. Renders prose blocks with the PDF
/// standard Helvetica font. `Break(PageAlways | Page)` starts a new page;
/// other break hints are ignored. Inline styles are ignored for now.
pub fn emit_pdf(doc: &PrintDocument) -> Result<Vec<u8>, WeaveError> {
    validate_profile(&doc.profile)?;

    let lines = collect_lines(doc)?;
    let pages = paginate_lines(&lines);

    let mut pdf = Pdf::new();
    pdf.set_version(1, 7);

    let catalog_id = Ref::new(1);
    let page_tree_id = Ref::new(2);
    let font_id = Ref::new(3);
    let mut next_id = 4_i32;

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

    let font_name = Name(b"F1");
    pdf.type1_font(font_id).base_font(Name(b"Helvetica"));

    for (page_id, content_id, page_lines) in page_ids
        .iter()
        .copied()
        .zip(content_ids.iter().copied())
        .zip(pages.iter())
        .map(|((p, c), l)| (p, c, l))
    {
        {
            let mut page = pdf.page(page_id);
            page.media_box(Rect::new(0.0, 0.0, PAGE_W, PAGE_H));
            page.parent(page_tree_id);
            page.contents(content_id);
            page.resources().fonts().pair(font_name, font_id);
        }

        let content_bytes = build_page_content(page_lines, font_name);
        pdf.stream(content_id, &content_bytes);
    }

    // Document info (best-effort; title from meta).
    let info_id = Ref::new(next_id);
    pdf.document_info(info_id)
        .title(TextStr(&doc.meta.title))
        .creator(TextStr("ariadnes-weave"))
        .producer(TextStr(&format!("ariadnes-weave {}", crate::VERSION)));
    // Trailer /Info is set via Pdf::document_info; catalog already written.

    Ok(pdf.finish())
}

fn validate_profile(profile: &PrintProfileId) -> Result<(), WeaveError> {
    match (profile.name.as_str(), profile.version) {
        ("print" | "manuscript", 0) => Ok(()),
        _ => Err(WeaveError::UnsupportedProfile {
            name: profile.name.clone(),
            version: profile.version,
        }),
    }
}

fn push_run_lines(
    seg: &mut Vec<LaidLine>,
    runs: &[TextRun],
    font_size: f32,
    leading: f32,
    gap_after: f32,
) -> Result<(), WeaveError> {
    let text = runs_to_text(runs)?;
    let wrapped = wrap_text(&text, font_size, content_width());
    for line in wrapped {
        seg.push(LaidLine {
            text: line,
            font_size,
            leading,
        });
    }
    if gap_after > 0.0 {
        seg.push(LaidLine {
            text: String::new(),
            font_size: 6.0,
            leading: gap_after,
        });
    }
    Ok(())
}

fn push_list_lines(
    seg: &mut Vec<LaidLine>,
    ordered: bool,
    items: &[crate::ir::ListItem],
    depth: usize,
) -> Result<(), WeaveError> {
    for (i, item) in items.iter().enumerate() {
        let marker = if ordered {
            format!("{}. ", i + 1)
        } else {
            // ASCII hyphen — Helvetica MVP has no Unicode bullet.
            "- ".into()
        };
        let indent = "  ".repeat(depth);
        let body = runs_to_text(&item.runs)?;
        let line = format!("{indent}{marker}{body}");
        let font_size = 11.0;
        for wrapped in wrap_text(&line, font_size, content_width()) {
            seg.push(LaidLine {
                text: wrapped,
                font_size,
                leading: font_size * 1.35,
            });
        }
        for child in &item.children {
            match child {
                PrintBlock::List {
                    ordered: child_ordered,
                    items: child_items,
                } => push_list_lines(seg, *child_ordered, child_items, depth + 1)?,
                other => {
                    return Err(WeaveError::UnsupportedBlock(match other {
                        PrintBlock::Heading { .. } => "heading_in_list",
                        PrintBlock::Paragraph { .. } => "paragraph_in_list",
                        PrintBlock::Code { .. } => "code_in_list",
                        PrintBlock::Quote { .. } => "quote_in_list",
                        PrintBlock::Break(_) => "break_in_list",
                        PrintBlock::List { .. } => unreachable!(),
                    }));
                }
            }
        }
    }
    seg.push(LaidLine {
        text: String::new(),
        font_size: 6.0,
        leading: 8.0,
    });
    Ok(())
}

#[derive(Debug, Clone)]
struct LaidLine {
    text: String,
    font_size: f32,
    leading: f32,
}

fn collect_lines(doc: &PrintDocument) -> Result<Vec<(ForcedBreak, Vec<LaidLine>)>, WeaveError> {
    // Segments separated by forced page breaks.
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
                let font_size = heading_size(*level);
                let seg = segments.last_mut().expect("at least one segment");
                push_run_lines(&mut seg.1, runs, font_size, font_size * 1.35, 8.0)?;
            }
            PrintBlock::Paragraph { runs } => {
                let seg = segments.last_mut().expect("at least one segment");
                push_run_lines(&mut seg.1, runs, 11.0, 11.0 * 1.4, 10.0)?;
            }
            PrintBlock::Quote { runs } => {
                let seg = segments.last_mut().expect("at least one segment");
                // MVP: prefix with a simple quote marker.
                let body = runs_to_text(runs)?;
                let quoted = format!("\"{body}\"");
                let font_size = 11.0;
                for line in wrap_text(&quoted, font_size, content_width()) {
                    seg.1.push(LaidLine {
                        text: line,
                        font_size,
                        leading: font_size * 1.4,
                    });
                }
                seg.1.push(LaidLine {
                    text: String::new(),
                    font_size: 6.0,
                    leading: 10.0,
                });
            }
            PrintBlock::Code { lang: _, text } => {
                let seg = segments.last_mut().expect("at least one segment");
                let font_size = 9.0;
                for line in text.lines() {
                    // ASCII-only MVP path.
                    for ch in line.chars() {
                        if !is_winansi_char(ch) {
                            return Err(WeaveError::UnencodableText(line.to_owned()));
                        }
                    }
                    seg.1.push(LaidLine {
                        text: line.to_owned(),
                        font_size,
                        leading: font_size * 1.25,
                    });
                }
                seg.1.push(LaidLine {
                    text: String::new(),
                    font_size: 6.0,
                    leading: 10.0,
                });
            }
            PrintBlock::List { ordered, items } => {
                let seg = segments.last_mut().expect("at least one segment");
                push_list_lines(&mut seg.1, *ordered, items, 0)?;
            }
        }
    }

    Ok(segments)
}

#[derive(Debug, Clone, Copy)]
enum ForcedBreak {
    None,
    Always,
}

fn paginate_lines(segments: &[(ForcedBreak, Vec<LaidLine>)]) -> Vec<Vec<LaidLine>> {
    let max_y_span = PAGE_H - 2.0 * MARGIN;
    let mut pages: Vec<Vec<LaidLine>> = Vec::new();
    let mut current: Vec<LaidLine> = Vec::new();
    let mut used = 0.0_f32;

    let flush = |pages: &mut Vec<Vec<LaidLine>>, current: &mut Vec<LaidLine>, used: &mut f32| {
        if !current.is_empty() {
            pages.push(std::mem::take(current));
            *used = 0.0;
        } else if pages.is_empty() {
            // Ensure at least one empty page for empty docs.
            pages.push(Vec::new());
        }
    };

    for (forced, lines) in segments {
        if matches!(forced, ForcedBreak::Always) && !current.is_empty() {
            flush(&mut pages, &mut current, &mut used);
        }
        for line in lines {
            if used + line.leading > max_y_span && !current.is_empty() {
                flush(&mut pages, &mut current, &mut used);
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

fn build_page_content(lines: &[LaidLine], font_name: Name<'_>) -> Vec<u8> {
    let mut content = Content::new();
    let mut y = PAGE_H - MARGIN;

    content.begin_text();
    for line in lines {
        y -= line.leading;
        if y < MARGIN {
            break;
        }
        content.set_font(font_name, line.font_size);
        // Absolute positioning via text matrix.
        content.set_text_matrix([1.0, 0.0, 0.0, 1.0, MARGIN, y]);
        if !line.text.is_empty() {
            let bytes = winansi_bytes(&line.text);
            content.show(Str(&bytes));
        }
    }
    content.end_text();
    content.finish().into_vec()
}

fn content_width() -> f32 {
    PAGE_W - 2.0 * MARGIN
}

fn heading_size(level: u8) -> f32 {
    match level {
        1 => 18.0,
        2 => 14.0,
        3 => 12.0,
        _ => 11.0,
    }
}

fn runs_to_text(runs: &[TextRun]) -> Result<String, WeaveError> {
    let mut out = String::new();
    for run in runs {
        // Reject text we cannot show with Helvetica WinAnsi (keep ASCII + Latin-1).
        for ch in run.text.chars() {
            if !is_winansi_char(ch) {
                return Err(WeaveError::UnencodableText(run.text.clone()));
            }
        }
        out.push_str(&run.text);
    }
    Ok(out)
}

fn is_winansi_char(ch: char) -> bool {
    // MVP: ASCII printable + common whitespace. Full WinAnsi later with TTF.
    ch == '\t' || ch == '\n' || ch == '\r' || (ch as u32 >= 0x20 && ch as u32 <= 0x7E)
}

fn winansi_bytes(text: &str) -> Vec<u8> {
    text.chars()
        .filter(|ch| *ch != '\n' && *ch != '\r')
        .map(|ch| if ch == '\t' { b' ' } else { ch as u8 })
        .collect()
}

fn wrap_text(text: &str, font_size: f32, max_width: f32) -> Vec<String> {
    let max_chars = ((max_width / (font_size * CHAR_WIDTH_FACTOR)).floor() as usize).max(1);
    if text.is_empty() {
        return vec![String::new()];
    }

    let mut lines = Vec::new();
    let mut remaining = text.trim_end();
    while !remaining.is_empty() {
        if remaining.len() <= max_chars {
            lines.push(remaining.to_string());
            break;
        }
        let mut split_at = max_chars;
        // Prefer breaking on whitespace.
        if let Some(rel) = remaining[..split_at].rfind(char::is_whitespace)
            && rel > 0
        {
            split_at = rel;
        }
        let (line, rest) = remaining.split_at(split_at);
        lines.push(line.trim_end().to_string());
        remaining = rest.trim_start();
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{InlineStyle, PrintMeta};

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
        assert!(
            bytes.starts_with(b"%PDF-"),
            "missing PDF magic, got {:?}",
            bytes.get(..16)
        );
        assert!(
            bytes.windows(5).any(|w| w == b"%%EOF"),
            "missing %%EOF trailer"
        );
    }

    #[test]
    fn rejects_unknown_profile() {
        let mut doc = hello_doc();
        doc.profile = PrintProfileId {
            name: "manuscript".into(),
            version: 1,
        };
        let err = emit_pdf(&doc).unwrap_err();
        assert!(matches!(err, WeaveError::UnsupportedProfile { .. }));
    }

    #[test]
    fn accepts_manuscript_v0() {
        let mut doc = hello_doc();
        doc.profile = PrintProfileId::manuscript_v0();
        let bytes = emit_pdf(&doc).expect("emit");
        assert!(bytes.starts_with(b"%PDF-"));
    }
}
