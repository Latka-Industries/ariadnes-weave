//! PDF `/Outlines` tree from heading destinations (THI-393).

use std::collections::BTreeMap;

use pdf_writer::{Pdf, Ref, TextStr};

use crate::ir::{PrintBlock, PrintDocument, TextRun};

use super::pdf_write::alloc_ref;

/// One outline bookmark derived from a heading with a resolvable `dest_id`.
#[derive(Debug, Clone)]
pub(super) struct OutlineEntry {
    level: u8,
    title: String,
    page_idx: usize,
}

/// Collect heading outline entries in document order.
///
/// Skips headings without `dest_id`, empty titles, or unresolved pages.
pub(super) fn collect_outline_entries(
    doc: &PrintDocument,
    dest_pages: &BTreeMap<String, usize>,
) -> Vec<OutlineEntry> {
    let mut out = Vec::new();
    for block in &doc.blocks {
        let PrintBlock::Heading {
            level,
            runs,
            dest_id: Some(dest_id),
            ..
        } = block
        else {
            continue;
        };
        let Some(&page_idx) = dest_pages.get(dest_id) else {
            continue;
        };
        let title = runs_title(runs);
        if title.is_empty() {
            continue;
        }
        out.push(OutlineEntry {
            level: (*level).max(1),
            title,
            page_idx,
        });
    }
    out
}

fn runs_title(runs: &[TextRun]) -> String {
    let raw: String = runs.iter().map(|r| r.text.as_str()).collect();
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Tree links for one outline item (indices into the entries slice).
#[derive(Debug, Clone, Default)]
struct OutlineLinks {
    parent: Option<usize>,
    prev: Option<usize>,
    next: Option<usize>,
    first_child: Option<usize>,
    last_child: Option<usize>,
    /// Total descendant count (for `/Count`).
    descendants: i32,
}

/// Append `child` as the next sibling under `parent` (or start the child list).
fn append_child(links: &mut [OutlineLinks], parent: usize, child: usize) {
    if let Some(prev) = links[parent].last_child {
        links[prev].next = Some(child);
        links[child].prev = Some(prev);
    } else {
        links[parent].first_child = Some(child);
    }
    links[parent].last_child = Some(child);
}

/// Append a top-level outline item, linking siblings via `roots`.
fn append_root(links: &mut [OutlineLinks], roots: &mut Vec<usize>, child: usize) {
    if let Some(&prev) = roots.last() {
        links[prev].next = Some(child);
        links[child].prev = Some(prev);
    }
    roots.push(child);
}

fn build_outline_links(entries: &[OutlineEntry]) -> (Vec<usize>, Vec<OutlineLinks>) {
    let n = entries.len();
    let mut links = vec![OutlineLinks::default(); n];
    let mut roots: Vec<usize> = Vec::new();
    // Stack of (level, index) for open ancestors.
    let mut stack: Vec<(u8, usize)> = Vec::new();

    for (i, entry) in entries.iter().enumerate() {
        while stack.last().is_some_and(|&(lvl, _)| lvl >= entry.level) {
            stack.pop();
        }
        if let Some(&(_, parent_idx)) = stack.last() {
            links[i].parent = Some(parent_idx);
            append_child(&mut links, parent_idx, i);
        } else {
            append_root(&mut links, &mut roots, i);
        }
        stack.push((entry.level, i));
    }

    // Bottom-up descendant counts.
    for i in (0..n).rev() {
        let mut count = 0_i32;
        let mut child = links[i].first_child;
        while let Some(c) = child {
            count += 1 + links[c].descendants;
            child = links[c].next;
        }
        links[i].descendants = count;
    }

    (roots, links)
}

/// Write the outline object tree; returns the root `/Outlines` ref (catalog wires it).
pub(super) fn write_outline(
    pdf: &mut Pdf,
    page_ids: &[Ref],
    entries: &[OutlineEntry],
    next_id: &mut i32,
) -> Option<Ref> {
    if entries.is_empty() {
        return None;
    }

    let root_id = alloc_ref(next_id);
    let item_ids: Vec<Ref> = (0..entries.len()).map(|_| alloc_ref(next_id)).collect();

    let (roots, links) = build_outline_links(entries);
    let first_root = item_ids[roots[0]];
    let last_root = item_ids[*roots.last().expect("non-empty roots")];
    let visible: i32 = entries.len().try_into().unwrap_or(i32::MAX);

    {
        let mut outline = pdf.outline(root_id);
        outline.first(first_root).last(last_root).count(visible);
    }

    for (i, entry) in entries.iter().enumerate() {
        let link = &links[i];
        let parent_ref = link.parent.map_or(root_id, |p| item_ids[p]);
        let mut item = pdf.outline_item(item_ids[i]);
        item.title(TextStr(&entry.title));
        item.parent(parent_ref);
        if let Some(prev) = link.prev {
            item.prev(item_ids[prev]);
        }
        if let Some(next) = link.next {
            item.next(item_ids[next]);
        }
        if let Some(first) = link.first_child {
            item.first(item_ids[first]);
        }
        if let Some(last) = link.last_child {
            item.last(item_ids[last]);
        }
        if link.descendants > 0 {
            // Positive count → open (all descendants visible).
            item.count(link.descendants);
        }
        if let Some(&page_ref) = page_ids.get(entry.page_idx) {
            item.dest().page(page_ref).fit();
        }
    }

    Some(root_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{BreakHint, PrintMeta, PrintProfileId, TextRun};

    fn heading(level: u8, title: &str, dest: &str) -> PrintBlock {
        PrintBlock::heading_dest(level, vec![TextRun::plain(title)], BreakHint::None, dest)
    }

    #[test]
    fn collects_nested_headings_with_pages() {
        let doc = PrintDocument {
            meta: PrintMeta {
                title: "T".into(),
                doc_kind: "note".into(),
                language: None,
                source_doc_id: None,
            },
            profile: PrintProfileId::print_v0(),
            blocks: vec![
                heading(1, "A", "a"),
                heading(2, "A.1", "a1"),
                heading(1, "B", "b"),
            ],
        };
        let pages = BTreeMap::from([("a".into(), 0usize), ("a1".into(), 0), ("b".into(), 1)]);
        let entries = collect_outline_entries(&doc, &pages);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].title, "A");
        assert_eq!(entries[1].level, 2);
        assert_eq!(entries[2].page_idx, 1);

        let (roots, links) = build_outline_links(&entries);
        assert_eq!(roots, vec![0, 2]);
        assert_eq!(links[0].first_child, Some(1));
        assert_eq!(links[0].descendants, 1);
        assert_eq!(links[1].parent, Some(0));
        assert_eq!(links[2].prev, Some(0));
    }
}
