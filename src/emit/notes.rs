//! Footnote / endnote book: defs, first-reference numbering, laid footnote bands.

use std::collections::{BTreeMap, BTreeSet};

use crate::ir::{NoteKind, PrintBlock, PrintDocument, TextRun};

use super::types::{LaidItem, LaidLine};

/// One note definition from [`PrintBlock::Note`].
#[derive(Debug, Clone)]
pub(super) struct NoteDef {
    pub kind: NoteKind,
    pub runs: Vec<TextRun>,
}

/// Collected notes, first-reference numbers, and pre-laid footnote bodies.
#[derive(Debug, Clone, Default)]
pub(super) struct NoteBook {
    pub defs: BTreeMap<String, NoteDef>,
    pub numbers: BTreeMap<String, u32>,
    pub order: Vec<String>,
    pub laid_footnotes: BTreeMap<String, Vec<LaidItem>>,
}

impl NoteBook {
    /// Walk the document (including columns / list children) for note defs.
    pub(super) fn collect(doc: &PrintDocument) -> Self {
        let mut book = Self::default();
        collect_block_notes(&doc.blocks, &mut book);
        book
    }

    pub(super) fn insert(&mut self, id: String, kind: NoteKind, runs: Vec<TextRun>) {
        self.defs.insert(id, NoteDef { kind, runs });
    }

    /// Assign a first-reference number, or return the existing one. Unknown ids skip.
    pub(super) fn assign(&mut self, id: &str) -> Option<u32> {
        if !self.defs.contains_key(id) {
            return None;
        }
        if let Some(&n) = self.numbers.get(id) {
            return Some(n);
        }
        let n = u32::try_from(self.order.len()).unwrap_or(u32::MAX) + 1;
        self.order.push(id.to_owned());
        self.numbers.insert(id.to_owned(), n);
        Some(n)
    }

    pub(super) fn has_footnote_defs(&self) -> bool {
        self.defs.values().any(|d| d.kind == NoteKind::Footnote)
    }

    pub(super) fn referenced_ids(&self, kind: NoteKind) -> Vec<String> {
        self.order
            .iter()
            .filter(|id| self.defs.get(*id).is_some_and(|d| d.kind == kind))
            .cloned()
            .collect()
    }

    /// Marker + body runs for a referenced note (for layout).
    pub(super) fn labeled_runs(&self, id: &str) -> Vec<TextRun> {
        let n = self.numbers.get(id).copied().unwrap_or(0);
        let mut runs = vec![TextRun::plain(format!("{n}. "))];
        if let Some(def) = self.defs.get(id) {
            runs.extend(def.runs.iter().cloned());
        }
        runs
    }
}

fn collect_block_notes(blocks: &[PrintBlock], book: &mut NoteBook) {
    for block in blocks {
        match block {
            PrintBlock::Note {
                id,
                note_kind,
                runs,
            } => {
                book.insert(id.clone(), *note_kind, runs.clone());
            }
            PrintBlock::Columns { children, .. } => collect_block_notes(children, book),
            PrintBlock::List { items, .. } => {
                for item in items {
                    collect_block_notes(&item.children, book);
                }
            }
            _ => {}
        }
    }
}

/// First-seen `note_id`s on a page, in reading order.
pub(super) fn page_note_ids(items: &[LaidItem]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut order = Vec::new();
    walk_items(items, &mut seen, &mut order);
    order
}

fn walk_items(items: &[LaidItem], seen: &mut BTreeSet<String>, order: &mut Vec<String>) {
    for item in items {
        match item {
            LaidItem::Text(line) => walk_line(line, seen, order),
            LaidItem::Callout(band) => {
                for line in &band.lines {
                    walk_line(line, seen, order);
                }
            }
            LaidItem::Columns(cols) => {
                for col in &cols.columns {
                    for line in col {
                        walk_line(line, seen, order);
                    }
                }
            }
            LaidItem::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        for line in cell {
                            walk_line(line, seen, order);
                        }
                    }
                }
            }
            LaidItem::Image { .. } | LaidItem::Math(_) | LaidItem::Rule { .. } => {}
        }
    }
}

fn walk_line(line: &LaidLine, seen: &mut BTreeSet<String>, order: &mut Vec<String>) {
    for span in &line.spans {
        if let Some(id) = &span.note_id
            && seen.insert(id.clone())
        {
            order.push(id.clone());
        }
    }
}
