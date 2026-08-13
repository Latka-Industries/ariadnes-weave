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
use crate::ir::{PrintBlock, PrintDocument};
use crate::knobs::LayoutKnobs;
use crate::profile::ProfileMetrics;

use super::math::layout_math;
use super::types::{ForcedBreak, GlyphSets, LaidItem, LayoutDoc, LayoutSegment, PaintCategory};

use columns::{LayoutColumnsArgs, layout_columns};
use figure::PushFigureArgs;
use ops::layout_layout_ops;
use prose::{
    TocEntryParts, layout_code, layout_heading, layout_quote, push_list_lines, push_row,
    push_toc_entry,
};
use runs::{body_layout, layout_ctx, push_styled_runs};
use slide::layout_slide;
use table::push_table;

pub(super) struct LayoutCtx<'a> {
    pub(super) metrics: &'a ProfileMetrics,
    pub(super) fonts: &'a FontBag,
    pub(super) knobs: &'a LayoutKnobs,
    pub(super) glyph_sets: &'a mut GlyphSets,
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

fn layout_block(
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
            dest_id,
        } => {
            let mut ctx = layout_ctx(metrics, fonts, knobs, glyph_sets);
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
        }
        .run()?,
        PrintBlock::Paragraph { runs, indent } => {
            let mut ctx = layout_ctx(metrics, fonts, knobs, glyph_sets);
            let seg = segments.last_mut().expect("segment");
            let band = knobs.prose.indent.pts(*indent);
            push_styled_runs(
                &mut seg.1,
                runs,
                &mut ctx,
                body_layout(metrics, knobs, band, PaintCategory::Text),
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
        PrintBlock::List {
            ordered,
            items,
            indent,
        } => {
            let mut ctx = layout_ctx(metrics, fonts, knobs, glyph_sets);
            let seg = segments.last_mut().expect("segment");
            push_list_lines(&mut seg.1, *ordered, items, *indent, 0, &mut ctx)?;
        }
        other => {
            layout_structure_block(other, metrics, fonts, knobs, segments, images, glyph_sets)?
        }
    }
    Ok(())
}

fn layout_structure_block(
    block: &PrintBlock,
    metrics: &ProfileMetrics,
    fonts: &FontBag,
    knobs: &LayoutKnobs,
    segments: &mut Vec<LayoutSegment>,
    images: &mut Vec<PreparedImage>,
    glyph_sets: &mut GlyphSets,
) -> Result<(), WeaveError> {
    match block {
        PrintBlock::Table { rows, dest_id } => {
            let mut ctx = layout_ctx(metrics, fonts, knobs, glyph_sets);
            let seg = segments.last_mut().expect("segment");
            push_table(&mut seg.1, rows, dest_id.as_deref(), &mut ctx)?;
        }
        PrintBlock::Row { panes, indent } => {
            let mut ctx = layout_ctx(metrics, fonts, knobs, glyph_sets);
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
            let mut ctx = layout_ctx(metrics, fonts, knobs, glyph_sets);
            layout_slide(layout_id, regions, &mut ctx, segments)?;
        }
        PrintBlock::Layout { ops } => {
            let mut ctx = layout_ctx(metrics, fonts, knobs, glyph_sets);
            layout_layout_ops(ops, &mut ctx, segments)?;
        }
        PrintBlock::Columns {
            count,
            gap,
            children,
        } => layout_columns(LayoutColumnsArgs {
            count: *count,
            gap: *gap,
            children,
            metrics,
            fonts,
            knobs,
            segments,
            images,
            glyph_sets,
        })?,
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
}

impl PushTocEntryArgs<'_> {
    fn run(self) -> Result<(), WeaveError> {
        let mut ctx = layout_ctx(self.metrics, self.fonts, self.knobs, self.glyph_sets);
        let seg = self.segments.last_mut().expect("segment");
        push_toc_entry(
            &mut seg.1,
            TocEntryParts {
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
        PrintBlock::Table { .. } => "table",
        PrintBlock::Row { .. } => "row",
        PrintBlock::TocEntry { .. } => "toc_entry",
        PrintBlock::Figure { .. } => "figure",
        PrintBlock::Math { .. } => "math",
        PrintBlock::Slide { .. } => "slide",
        PrintBlock::Layout { .. } => "layout",
        PrintBlock::Columns { .. } => "columns",
        PrintBlock::Break(_) => "break",
    }
}
