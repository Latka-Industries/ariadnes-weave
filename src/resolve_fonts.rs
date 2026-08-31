//! Build a [`FontBag`] from emit options (pins + optional OS lookup).

use crate::error::WeaveError;
use crate::font::FontBag;
use crate::ir::PrintDocument;
use crate::options::{EmitOptions, FontResolveMode};

/// Construct the face bag for one emit.
///
/// # Errors
///
/// * Invalid pinned TTF bytes
/// * [`FontResolveMode::OsWithFallback`] without `--features os-fonts`
pub fn build_font_bag(doc: &PrintDocument, opts: &EmitOptions) -> Result<FontBag, WeaveError> {
    let mut bag = FontBag::from_pinned(&opts.pinned_faces)?;
    bag.set_resolve_mode(opts.fonts);
    match opts.fonts {
        FontResolveMode::BundledOnly => Ok(bag),
        FontResolveMode::OsWithFallback => {
            #[cfg(feature = "os-fonts")]
            {
                hydrate_os_faces(doc, &mut bag);
                Ok(bag)
            }
            #[cfg(not(feature = "os-fonts"))]
            {
                let _ = doc;
                Err(WeaveError::Font(
                    "FontResolveMode::OsWithFallback requires `--features os-fonts`".into(),
                ))
            }
        }
    }
}

#[cfg(feature = "os-fonts")]
mod hydrate {
    use super::{FontBag, PrintDocument};
    use crate::ir::{InlineStyle, ListItem, PrintBlock, TextRun};
    use crate::os_fonts::{OsFontDb, os_pin_key};

    pub(super) fn hydrate_os_faces(doc: &PrintDocument, bag: &mut FontBag) {
        let db = OsFontDb::load_system();
        for (family, style) in named_faces(doc) {
            let key = os_pin_key(family, style);
            if bag.resolve_pin(&key).is_some() || bag.resolve_pin(family).is_some() {
                continue;
            }
            if let Some(bytes) = db.resolve_ttf(family, style) {
                // Ignore pin errors (e.g. unparseable) — layout will fall back.
                let _ = bag.pin_face(key, bytes);
            }
        }
    }

    fn named_faces(doc: &PrintDocument) -> Vec<(&str, InlineStyle)> {
        let mut out = Vec::new();
        for block in &doc.blocks {
            collect_block_faces(block, &mut out);
        }
        out
    }

    fn collect_block_faces<'a>(block: &'a PrintBlock, out: &mut Vec<(&'a str, InlineStyle)>) {
        match block {
            PrintBlock::Heading { runs, .. }
            | PrintBlock::Paragraph { runs, .. }
            | PrintBlock::Quote { runs, .. }
            | PrintBlock::TocEntry { title: runs, .. } => collect_run_faces(runs, out),
            PrintBlock::Callout { title, body, .. } => {
                collect_run_faces(title, out);
                collect_run_faces(body, out);
            }
            PrintBlock::Row { panes, .. } => {
                for pane in panes {
                    collect_run_faces(pane, out);
                }
            }
            PrintBlock::List { items, .. } => {
                for item in items {
                    collect_list_item_faces(item, out);
                }
            }
            PrintBlock::Figure { title, caption, .. } => {
                collect_run_faces(title, out);
                collect_run_faces(caption, out);
            }
            PrintBlock::Layout { ops } => {
                for op in ops {
                    if let crate::ir::LayoutOp::Place { runs, .. } = op {
                        collect_run_faces(runs, out);
                    }
                }
            }
            PrintBlock::Columns { children, .. } => {
                for child in children {
                    collect_block_faces(child, out);
                }
            }
            PrintBlock::Note { runs, .. } => collect_run_faces(runs, out),
            PrintBlock::Slide { .. }
            | PrintBlock::Code { .. }
            | PrintBlock::Table { .. }
            | PrintBlock::Math { .. }
            | PrintBlock::Break(_) => {}
        }
    }

    fn collect_list_item_faces<'a>(item: &'a ListItem, out: &mut Vec<(&'a str, InlineStyle)>) {
        collect_run_faces(&item.runs, out);
        for child in &item.children {
            collect_block_faces(child, out);
        }
    }

    fn collect_run_faces<'a>(runs: &'a [TextRun], out: &mut Vec<(&'a str, InlineStyle)>) {
        for run in runs {
            if let Some(face) = run.face.as_deref() {
                out.push((face, run.style));
            }
        }
    }
}

#[cfg(feature = "os-fonts")]
use hydrate::hydrate_os_faces;
