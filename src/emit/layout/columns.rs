//! Continuous multi-column body flow (THI-391 / THI-416).

use crate::error::WeaveError;
use crate::font::FontBag;
use crate::image_prep::PreparedImage;
use crate::ir::PrintBlock;
use crate::knobs::{LayoutKnobs, TextAlign};
use crate::profile::ProfileMetrics;

use super::super::notes::NoteBook;
use super::super::types::{ForcedBreak, GlyphSets, LaidColumns, LaidItem, LayoutSegment};
use super::layout_block;

/// Arguments for [`layout_columns`].
pub(super) struct LayoutColumnsArgs<'a> {
    pub count: u8,
    pub gap: Option<u16>,
    pub children: &'a [PrintBlock],
    pub inherit_align: Option<TextAlign>,
    pub metrics: &'a ProfileMetrics,
    pub fonts: &'a FontBag,
    pub knobs: &'a LayoutKnobs,
    pub segments: &'a mut Vec<LayoutSegment>,
    pub images: &'a mut Vec<PreparedImage>,
    pub glyph_sets: &'a mut GlyphSets,
    pub notes: &'a mut NoteBook,
}

/// True when a child must span full measure (flush column band first).
fn spans_full_measure(block: &PrintBlock) -> bool {
    match block {
        PrintBlock::Paragraph { .. }
        | PrintBlock::Quote { .. }
        | PrintBlock::Code { .. }
        | PrintBlock::List { .. }
        | PrintBlock::Heading { .. }
        | PrintBlock::Callout { .. }
        | PrintBlock::Table { .. }
        | PrintBlock::Math { .. } => false,
        PrintBlock::TocEntry { .. }
        | PrintBlock::Row { .. }
        | PrintBlock::Figure { .. }
        | PrintBlock::Slide { .. }
        | PrintBlock::Layout { .. }
        | PrintBlock::Columns { .. }
        | PrintBlock::Note { .. }
        | PrintBlock::Break(_) => true,
    }
}

/// Lay out a [`PrintBlock::Columns`] region into page bands of [`LaidItem::Columns`].
pub(super) fn layout_columns(args: LayoutColumnsArgs<'_>) -> Result<(), WeaveError> {
    let LayoutColumnsArgs {
        count,
        gap,
        children,
        inherit_align,
        metrics,
        fonts,
        knobs,
        segments,
        images,
        glyph_sets,
        notes,
    } = args;

    let n = usize::from(count.clamp(2, 6));
    let gap_pt = gap.map_or(knobs.prose.body_columns.gap, f32::from);
    let full_w = metrics.content_width();
    let col_w = ((full_w - (n.saturating_sub(1) as f32) * gap_pt) / n as f32)
        .max(knobs.prose.wrap.min_width);
    // Match paginate usable height so each full band fills one page.
    let max_h = (metrics.content_height()
        - knobs.page.chrome_reserve()
        - knobs.page.footnote_reserve(notes.has_footnote_defs()))
    .max(72.0);

    // Narrow measure via faked page width so existing wrap paths use col_w.
    let mut col_metrics = *metrics;
    col_metrics.page_w = col_w + 2.0 * metrics.margin;

    let mut flow: Vec<LaidItem> = Vec::new();

    for child in children {
        if spans_full_measure(child) {
            flush_flow_bands(&mut flow, n, gap_pt, col_w, max_h, segments);
            layout_block(
                child,
                metrics,
                fonts,
                knobs,
                segments,
                images,
                glyph_sets,
                notes,
                inherit_align,
            )?;
        } else {
            let mut temp: Vec<LayoutSegment> = vec![(ForcedBreak::None, Vec::new())];
            layout_block(
                child,
                &col_metrics,
                fonts,
                knobs,
                &mut temp,
                images,
                glyph_sets,
                notes,
                inherit_align,
            )?;
            for (forced, items) in temp {
                if matches!(forced, ForcedBreak::Always) && !flow.is_empty() {
                    flush_flow_bands(&mut flow, n, gap_pt, col_w, max_h, segments);
                    segments.push((ForcedBreak::Always, Vec::new()));
                }
                flow.extend(items);
            }
        }
    }
    flush_flow_bands(&mut flow, n, gap_pt, col_w, max_h, segments);
    Ok(())
}

fn flush_flow_bands(
    flow: &mut Vec<LaidItem>,
    n: usize,
    gap: f32,
    col_w: f32,
    max_h: f32,
    segments: &mut [LayoutSegment],
) {
    if flow.is_empty() {
        return;
    }
    let items = std::mem::take(flow);
    if items.is_empty() {
        return;
    }
    let bands = pack_items_into_columns(items, n, gap, col_w, max_h);
    let seg = segments.last_mut().expect("segment");
    seg.1.extend(bands);
}

/// Pack laid items into page-height column bands (fill col 0, then 1, …).
fn pack_items_into_columns(
    items: Vec<LaidItem>,
    n: usize,
    gap: f32,
    col_w: f32,
    max_h: f32,
) -> Vec<LaidItem> {
    let mut out = Vec::new();
    let mut cols: Vec<Vec<LaidItem>> = vec![Vec::new(); n];
    let mut heights = vec![0.0_f32; n];
    let mut col_i = 0_usize;

    let flush_band =
        |cols: &mut Vec<Vec<LaidItem>>, heights: &mut [f32], out: &mut Vec<LaidItem>| {
            if cols.iter().all(Vec::is_empty) {
                return;
            }
            out.push(LaidItem::Columns(LaidColumns {
                columns: std::mem::replace(cols, vec![Vec::new(); n]),
                col_widths: vec![col_w; n],
                gap,
                gap_after: 0.0,
                indent: 0.0,
            }));
            heights.fill(0.0);
        };

    for item in items {
        let h = item.height();
        if heights[col_i] + h > max_h && !cols[col_i].is_empty() {
            col_i += 1;
            if col_i >= n {
                flush_band(&mut cols, &mut heights, &mut out);
                col_i = 0;
            }
        }
        cols[col_i].push(item);
        heights[col_i] += h;
    }
    flush_band(&mut cols, &mut heights, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::super::super::types::{LaidCallout, LaidLine};
    use super::*;
    use crate::ir::{BreakHint, NoteKind, TableRow, TextRun};

    fn gap_item(leading: f32) -> LaidItem {
        LaidItem::Text(LaidLine::gap(leading))
    }

    #[test]
    fn packs_into_two_columns_then_next_band() {
        let items = vec![gap_item(40.0), gap_item(40.0), gap_item(40.0)];
        let bands = pack_items_into_columns(items, 2, 12.0, 200.0, 100.0);
        assert_eq!(bands.len(), 1);
        let LaidItem::Columns(cols) = &bands[0] else {
            panic!("expected Columns");
        };
        assert_eq!(cols.columns[0].len(), 2);
        assert_eq!(cols.columns[1].len(), 1);
    }

    #[test]
    fn fills_next_page_band_when_columns_full() {
        let items: Vec<_> = (0..5).map(|_| gap_item(40.0)).collect();
        let bands = pack_items_into_columns(items, 2, 10.0, 180.0, 80.0);
        assert_eq!(bands.len(), 2);
        let LaidItem::Columns(a) = &bands[0] else {
            panic!("band0");
        };
        assert_eq!(a.columns[0].len(), 2);
        assert_eq!(a.columns[1].len(), 2);
        let LaidItem::Columns(b) = &bands[1] else {
            panic!("band1");
        };
        assert_eq!(b.columns[0].len(), 1);
        assert!(b.columns[1].is_empty());
    }

    #[test]
    fn packing_keeps_callout_in_column() {
        let callout = LaidItem::Callout(LaidCallout {
            lines: vec![LaidLine::gap(20.0)],
            gap_after: 0.0,
            indent: 0.0,
            rule_thickness: 1.5,
        });
        let items = vec![gap_item(40.0), callout, gap_item(40.0)];
        let bands = pack_items_into_columns(items, 2, 12.0, 200.0, 50.0);
        let has_callout = bands.iter().any(|band| {
            let LaidItem::Columns(cols) = band else {
                return false;
            };
            cols.columns
                .iter()
                .flatten()
                .any(|item| matches!(item, LaidItem::Callout(_)))
        });
        assert!(
            has_callout,
            "THI-416 must not drop titled bands from columns"
        );
    }

    #[test]
    fn paragraph_heading_callout_math_table_stay_in_column() {
        assert!(!spans_full_measure(&PrintBlock::paragraph(vec![
            TextRun::plain("x")
        ])));
        assert!(!spans_full_measure(&PrintBlock::heading(
            2,
            vec![TextRun::plain("Title")],
            BreakHint::None,
        )));
        assert!(!spans_full_measure(&PrintBlock::callout(
            "note",
            vec![TextRun::plain("Note")],
            vec![TextRun::plain("body")],
        )));
        assert!(!spans_full_measure(&PrintBlock::Math {
            display: true,
            latex: "x".into(),
        }));
        assert!(!spans_full_measure(&PrintBlock::table(vec![TableRow {
            cells: vec!["a".into()],
        }])));
    }

    #[test]
    fn figure_toc_row_nested_columns_still_span() {
        assert!(spans_full_measure(&PrintBlock::toc_entry(
            vec![TextRun::plain("A")],
            None,
            None,
            0,
        )));
        assert!(spans_full_measure(&PrintBlock::row(vec![
            vec![TextRun::plain("L")],
            vec![TextRun::plain("R")],
        ])));
        assert!(spans_full_measure(&PrintBlock::columns(
            2,
            None,
            vec![PrintBlock::paragraph(vec![TextRun::plain("x")])],
        )));
        assert!(spans_full_measure(&PrintBlock::Break(
            BreakHint::PageAlways
        )));
        assert!(spans_full_measure(&PrintBlock::Note {
            id: "n1".into(),
            note_kind: NoteKind::Footnote,
            runs: vec![TextRun::plain("f")],
        }));
    }
}
