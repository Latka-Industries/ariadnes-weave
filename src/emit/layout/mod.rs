//! Turn print IR blocks into laid segments, images, and glyph sets.

mod columns;
mod figure;
mod hyphen;
mod ops;
mod prose;
mod runs;
mod slide;
mod table;

use crate::error::WeaveError;
use crate::font::FontBag;
use crate::image_prep::PreparedImage;
use crate::ir::{BreakHint, NoteKind, PrintBlock, PrintDocument, TextRun};
use crate::knobs::{LayoutKnobs, TextAlign};
use crate::profile::ProfileMetrics;

use super::math::layout_math;
use super::notes::NoteBook;
use super::types::{ForcedBreak, GlyphSets, LaidItem, LayoutDoc, LayoutSegment, PaintCategory};

use columns::{LayoutColumnsArgs, layout_columns};
use figure::PushFigureArgs;
use ops::layout_layout_ops;
use prose::{
    TocEntryParts, layout_callout, layout_code, layout_heading, layout_quote, push_list_lines,
    push_row, push_toc_entry,
};
use runs::{body_layout, layout_ctx, push_styled_runs};
use slide::layout_slide;
use table::push_table;

pub(super) struct LayoutCtx<'a> {
    pub(super) metrics: &'a ProfileMetrics,
    pub(super) fonts: &'a FontBag,
    pub(super) knobs: &'a LayoutKnobs,
    pub(super) glyph_sets: &'a mut GlyphSets,
    pub(super) notes: &'a mut super::notes::NoteBook,
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
    let mut notes = NoteBook::collect(doc);

    for block in &doc.blocks {
        layout_block(
            block,
            metrics,
            fonts,
            knobs,
            &mut segments,
            &mut images,
            &mut glyph_sets,
            &mut notes,
            None,
        )?;
    }

    assign_line_numbers(&mut segments, metrics, fonts, knobs, &mut glyph_sets)?;

    dump_endnotes(
        &notes,
        metrics,
        fonts,
        knobs,
        &mut segments,
        &mut images,
        &mut glyph_sets,
    )?;
    layout_footnote_bodies(&mut notes, metrics, fonts, knobs, &mut glyph_sets)?;

    Ok((segments, images, glyph_sets, notes))
}

/// Stamp 1-based line numbers on non-gap body lines when `[body].line_numbers`.
fn assign_line_numbers(
    segments: &mut [LayoutSegment],
    metrics: &ProfileMetrics,
    fonts: &FontBag,
    knobs: &LayoutKnobs,
    glyph_sets: &mut GlyphSets,
) -> Result<(), WeaveError> {
    if !knobs.prose.body.line_numbers {
        return Ok(());
    }
    let font_size = metrics.body_size * knobs.prose.body.line_number_size_factor;
    let face = crate::font::FaceRef::Bundled(crate::font::FaceId::SansRegular);
    let fill = knobs.prose.text_fill_rgb01();
    let mut n = 1_u32;
    for (_, items) in segments.iter_mut() {
        for item in items.iter_mut() {
            item.for_each_content_line_mut(&mut |line| {
                if line.is_gap() {
                    return;
                }
                line.line_no = Some(n);
                n += 1;
            });
        }
    }
    let mut failed = None;
    for (_, items) in segments.iter_mut() {
        for item in items.iter_mut() {
            item.for_each_content_line_mut(&mut |line| {
                if failed.is_some() {
                    return;
                }
                let Some(no) = line.line_no else {
                    return;
                };
                let text = no.to_string();
                match super::types::shape_and_record_spans(super::types::ShapeSpans {
                    fonts,
                    face,
                    text: &text,
                    font_size,
                    glyph_sets,
                    fill,
                    underline: false,
                    link_uri: None,
                    link_dest: None,
                    baseline_shift: 0.0,
                    note_id: None,
                }) {
                    Ok((spans, _)) => line.gutter_spans = spans,
                    Err(e) => failed = Some(e),
                }
            });
        }
    }
    if let Some(e) = failed {
        return Err(e);
    }
    Ok(())
}

fn dump_endnotes(
    notes: &NoteBook,
    metrics: &ProfileMetrics,
    fonts: &FontBag,
    knobs: &LayoutKnobs,
    segments: &mut Vec<LayoutSegment>,
    images: &mut Vec<PreparedImage>,
    glyph_sets: &mut GlyphSets,
) -> Result<(), WeaveError> {
    let ids = notes.referenced_ids(NoteKind::Endnote);
    if ids.is_empty() {
        return Ok(());
    }
    let extra = {
        let mut blocks = vec![PrintBlock::heading(
            1,
            vec![TextRun::plain("Notes")],
            BreakHint::None,
        )];
        for id in &ids {
            blocks.push(PrintBlock::paragraph(notes.labeled_runs(id)));
        }
        blocks
    };
    let mut unused = NoteBook::default();
    for block in &extra {
        layout_block(
            block,
            metrics,
            fonts,
            knobs,
            segments,
            images,
            glyph_sets,
            &mut unused,
            None,
        )?;
    }
    Ok(())
}

fn layout_footnote_bodies(
    notes: &mut NoteBook,
    metrics: &ProfileMetrics,
    fonts: &FontBag,
    knobs: &LayoutKnobs,
    glyph_sets: &mut GlyphSets,
) -> Result<(), WeaveError> {
    let ids = notes.referenced_ids(NoteKind::Footnote);
    let layout = footnote_run_layout(metrics, knobs);
    for id in ids {
        let runs = notes.labeled_runs(&id);
        let mut items = Vec::new();
        let mut unused = NoteBook::default();
        let mut ctx = layout_ctx(metrics, fonts, knobs, glyph_sets, &mut unused);
        push_styled_runs(&mut items, &runs, &mut ctx, layout)?;
        notes.laid_footnotes.insert(id, items);
    }
    Ok(())
}

fn footnote_run_layout(metrics: &ProfileMetrics, knobs: &LayoutKnobs) -> super::types::RunLayout {
    let font_size = metrics.body_size * knobs.page.footnote.size_factor;
    super::types::RunLayout {
        font_size,
        leading: font_size * knobs.page.footnote.leading_factor,
        gap_after: 2.0,
        glue_last_content: false,
        mode: super::types::FaceMode::Body,
        indent: 0.0,
        max_width: None,
        paint: PaintCategory::Text,
        hard_break_overflow: true,
        text_align: TextAlign::Left,
    }
}

/// Pack `[paragraph].text_align`, overridden by inherit then explicit (THI-398).
fn resolved_prose_align(
    explicit: Option<TextAlign>,
    inherit: Option<TextAlign>,
    knobs: &LayoutKnobs,
) -> TextAlign {
    explicit
        .or(inherit)
        .unwrap_or(knobs.prose.paragraph.text_align)
}

/// Quotes stay flush-start unless a block or region sets align.
fn resolved_quote_align(explicit: Option<TextAlign>, inherit: Option<TextAlign>) -> TextAlign {
    explicit.or(inherit).unwrap_or(TextAlign::Left)
}

#[allow(clippy::too_many_arguments)]
fn layout_block(
    block: &PrintBlock,
    metrics: &ProfileMetrics,
    fonts: &FontBag,
    knobs: &LayoutKnobs,
    segments: &mut Vec<LayoutSegment>,
    images: &mut Vec<PreparedImage>,
    glyph_sets: &mut GlyphSets,
    notes: &mut NoteBook,
    inherit_align: Option<TextAlign>,
) -> Result<(), WeaveError> {
    match block {
        PrintBlock::Break(hint) => {
            if hint.forces_page_break() {
                segments.push((ForcedBreak::Always, Vec::new()));
            }
        }
        PrintBlock::Note { .. } => {}
        PrintBlock::Heading {
            level,
            runs,
            break_before,
            dest_id,
        } => {
            let mut ctx = layout_ctx(metrics, fonts, knobs, glyph_sets, notes);
            layout_heading(
                *level,
                runs,
                *break_before,
                dest_id.as_deref(),
                &mut ctx,
                segments,
            )?;
        }
        PrintBlock::TocEntry {
            title,
            page_label,
            dest_id,
            indent,
            leaders,
        } => PushTocEntryArgs {
            title,
            page_label: page_label.as_deref(),
            dest_id: dest_id.as_deref(),
            indent: *indent,
            leaders: *leaders,
            metrics,
            fonts,
            knobs,
            segments,
            glyph_sets,
            notes,
        }
        .run()?,
        PrintBlock::Paragraph {
            runs,
            indent,
            text_align,
        } => {
            let mut ctx = layout_ctx(metrics, fonts, knobs, glyph_sets, notes);
            let seg = segments.last_mut().expect("segment");
            let band = knobs.prose.indent.pts(*indent);
            let align = resolved_prose_align(*text_align, inherit_align, knobs);
            push_styled_runs(
                &mut seg.1,
                runs,
                &mut ctx,
                body_layout(metrics, knobs, band, PaintCategory::Text, align),
            )?;
        }
        PrintBlock::Quote { runs, text_align } => {
            let mut ctx = layout_ctx(metrics, fonts, knobs, glyph_sets, notes);
            layout_quote(
                runs,
                resolved_quote_align(*text_align, inherit_align),
                &mut ctx,
                segments,
            )?;
        }
        PrintBlock::Callout {
            title,
            body,
            callout_kind: _,
        } => {
            let mut ctx = layout_ctx(metrics, fonts, knobs, glyph_sets, notes);
            layout_callout(title, body, &mut ctx, segments)?;
        }
        PrintBlock::Code { lang: _, text } => {
            let mut ctx = layout_ctx(metrics, fonts, knobs, glyph_sets, notes);
            layout_code(text, &mut ctx, segments)?;
        }
        PrintBlock::List {
            ordered,
            items,
            indent,
            text_align,
        } => {
            let mut ctx = layout_ctx(metrics, fonts, knobs, glyph_sets, notes);
            let seg = segments.last_mut().expect("segment");
            let align = resolved_prose_align(*text_align, inherit_align, knobs);
            push_list_lines(&mut seg.1, *ordered, items, *indent, 0, align, &mut ctx)?;
        }
        other => layout_structure_block(
            other,
            metrics,
            fonts,
            knobs,
            segments,
            images,
            glyph_sets,
            notes,
            inherit_align,
        )?,
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn layout_structure_block(
    block: &PrintBlock,
    metrics: &ProfileMetrics,
    fonts: &FontBag,
    knobs: &LayoutKnobs,
    segments: &mut Vec<LayoutSegment>,
    images: &mut Vec<PreparedImage>,
    glyph_sets: &mut GlyphSets,
    notes: &mut NoteBook,
    inherit_align: Option<TextAlign>,
) -> Result<(), WeaveError> {
    match block {
        PrintBlock::Table { rows, dest_id } => {
            let mut ctx = layout_ctx(metrics, fonts, knobs, glyph_sets, notes);
            let seg = segments.last_mut().expect("segment");
            push_table(&mut seg.1, rows, dest_id.as_deref(), &mut ctx)?;
        }
        PrintBlock::Row { panes, indent } => {
            let mut ctx = layout_ctx(metrics, fonts, knobs, glyph_sets, notes);
            let seg = segments.last_mut().expect("segment");
            push_row(&mut seg.1, panes, *indent, &mut ctx)?;
        }
        PrintBlock::Figure {
            image,
            alt,
            title,
            caption,
            placement,
            dest_id,
        } => PushFigureArgs {
            segments,
            images,
            image,
            alt,
            title,
            caption,
            placement: *placement,
            dest_id: dest_id.as_deref(),
            metrics,
            fonts,
            knobs,
            glyph_sets,
            notes,
        }
        .run()?,
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
            let mut ctx = layout_ctx(metrics, fonts, knobs, glyph_sets, notes);
            layout_slide(layout_id, regions, &mut ctx, segments)?;
        }
        PrintBlock::Layout { ops } => {
            let mut ctx = layout_ctx(metrics, fonts, knobs, glyph_sets, notes);
            layout_layout_ops(ops, &mut ctx, segments)?;
        }
        PrintBlock::Columns {
            count,
            gap,
            children,
            text_align,
        } => layout_columns(LayoutColumnsArgs {
            count: *count,
            gap: *gap,
            children,
            inherit_align: text_align.or(inherit_align),
            metrics,
            fonts,
            knobs,
            segments,
            images,
            glyph_sets,
            notes,
        })?,
        PrintBlock::Note { .. } => {}
        _ => unreachable!("text-like blocks handled in layout_block"),
    }
    Ok(())
}

/// Inputs for laying out a [`PrintBlock::TocEntry`].
struct PushTocEntryArgs<'a> {
    title: &'a [crate::ir::TextRun],
    page_label: Option<&'a str>,
    dest_id: Option<&'a str>,
    indent: u32,
    leaders: bool,
    metrics: &'a ProfileMetrics,
    fonts: &'a FontBag,
    knobs: &'a LayoutKnobs,
    segments: &'a mut [LayoutSegment],
    glyph_sets: &'a mut GlyphSets,
    notes: &'a mut NoteBook,
}

impl PushTocEntryArgs<'_> {
    fn run(self) -> Result<(), WeaveError> {
        let mut ctx = layout_ctx(
            self.metrics,
            self.fonts,
            self.knobs,
            self.glyph_sets,
            self.notes,
        );
        let seg = self.segments.last_mut().expect("segment");
        push_toc_entry(
            &mut seg.1,
            &TocEntryParts {
                title: self.title,
                page_label: self.page_label,
                dest_id: self.dest_id,
                indent: self.indent,
                leaders: self.leaders,
            },
            &mut ctx,
        )
    }
}

fn segment_has_content(segments: &[LayoutSegment]) -> bool {
    segments.last().is_some_and(|(_, items)| !items.is_empty())
}

fn block_name(block: &PrintBlock) -> &'static str {
    match block {
        PrintBlock::Heading { .. } => "heading",
        PrintBlock::Paragraph { .. } => "paragraph",
        PrintBlock::List { .. } => "list",
        PrintBlock::Code { .. } => "code",
        PrintBlock::Quote { .. } => "quote",
        PrintBlock::Callout { .. } => "callout",
        PrintBlock::Table { .. } => "table",
        PrintBlock::Row { .. } => "row",
        PrintBlock::TocEntry { .. } => "toc_entry",
        PrintBlock::Figure { .. } => "figure",
        PrintBlock::Math { .. } => "math",
        PrintBlock::Slide { .. } => "slide",
        PrintBlock::Layout { .. } => "layout",
        PrintBlock::Columns { .. } => "columns",
        PrintBlock::Note { .. } => "note",
        PrintBlock::Break(_) => "break",
    }
}
