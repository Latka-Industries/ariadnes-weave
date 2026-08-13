//! Print IR → PDF emit pipeline.
//!
//! Stages: layout (`layout`) → pagination (`paginate`) → glyph subset remap
//! (`pdf_write`) → page content paint (`paint`) → PDF objects (`pdf_write`).
//! Heading `dest_id`s also drive `/Outlines` bookmarks (`outline`; THI-393).
//! Structured math layout lives in [`math`]; laid item types in [`types`].

mod layout;
mod math;
mod outline;
mod paginate;
mod paint;
mod pdf_write;
mod types;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use pdf_writer::{Pdf, Ref, TextStr};

use crate::error::WeaveError;
use crate::font::{FaceId, FaceRef, FontBag, collect_glyph_set};
use crate::ir::{PrintBlock, PrintDocument};
use crate::knobs::LayoutKnobs;
use crate::options::EmitOptions;
use crate::profile::{self, ProfileMetrics};
use crate::resolve_fonts::build_font_bag;

use layout::collect_layout;
use paginate::paginate_items;
use pdf_write::{
    WritePagesArgs, alloc_image_refs, alloc_page_refs, embed_fonts, prepare_subsets, remap_pages,
    write_image_xobjects,
};
use types::{LaidItem, LayoutDoc};

/// Emit PDF bytes from a print document using [`EmitOptions::default`]
/// ([`crate::FontResolveMode::BundledOnly`]).
///
/// # Errors
///
/// See [`emit_pdf_with`].
pub fn emit_pdf(doc: &PrintDocument) -> Result<Vec<u8>, WeaveError> {
    emit_pdf_with(doc, &EmitOptions::default())
}

/// Emit PDF bytes with explicit [`EmitOptions`].
///
/// Resolves the profile, lays out blocks, paginates, subsets used faces, and
/// writes a PDF 1.7 file with optional page chrome (header/footer).
///
/// When any [`PrintBlock::TocEntry`] has `page_label = None` and a `dest_id`,
/// emit runs a layout+paginate pass to resolve 1-based page digits, then
/// re-layouts and writes with `GoTo` destinations.
///
/// Supports [`crate::FontResolveMode::BundledOnly`] (default) and
/// [`crate::FontResolveMode::OsWithFallback`] (requires `--features os-fonts`).
///
/// # Errors
///
/// Returns [`WeaveError`] if the profile is unsupported, font
/// subsetting/embedding fails, an image cannot be decoded, or OS mode is
/// requested without the `os-fonts` feature.
pub fn emit_pdf_with(doc: &PrintDocument, opts: &EmitOptions) -> Result<Vec<u8>, WeaveError> {
    let fonts = build_font_bag(doc, opts)?;

    let metrics = profile::resolve_metrics(&doc.profile)?;
    let mut layout = opts.layout.clone();
    if doc.profile.name == "resume" {
        layout.densify_resume();
    }

    let mut work = doc.clone();
    if needs_toc_page_resolve(&work) {
        let (segments, _images, _glyph_sets) = collect_layout(&work, &metrics, &fonts, &layout)?;
        let pages = paginate_items(
            &segments,
            metrics.content_height(),
            layout.page.chrome_reserve(),
        );
        let dest_pages = collect_dest_pages(&pages);
        fill_toc_page_labels(&mut work, &dest_pages);
    }

    emit_laid_pdf(&work, &fonts, &metrics, &layout)
}

fn needs_toc_page_resolve(doc: &PrintDocument) -> bool {
    doc.blocks.iter().any(|b| {
        matches!(
            b,
            PrintBlock::TocEntry {
                page_label: None,
                dest_id: Some(_),
                ..
            }
        )
    })
}

/// Map heading `dest_id` → 0-based page index from laid pages.
fn collect_dest_pages(pages: &[Vec<LaidItem>]) -> BTreeMap<String, usize> {
    let mut map = BTreeMap::new();
    for (page_idx, items) in pages.iter().enumerate() {
        for item in items {
            if let LaidItem::Text(line) = item
                && let Some(id) = line.dest_id.as_ref()
            {
                map.entry(id.clone()).or_insert(page_idx);
            }
        }
    }
    map
}

fn fill_toc_page_labels(doc: &mut PrintDocument, dest_pages: &BTreeMap<String, usize>) {
    for block in &mut doc.blocks {
        if let PrintBlock::TocEntry {
            page_label,
            dest_id,
            ..
        } = block
            && page_label.is_none()
            && let Some(id) = dest_id.as_ref()
            && let Some(&page_idx) = dest_pages.get(id)
        {
            *page_label = Some((page_idx + 1).to_string());
        }
    }
}

fn emit_laid_pdf(
    doc: &PrintDocument,
    fonts: &FontBag,
    metrics: &ProfileMetrics,
    layout: &LayoutKnobs,
) -> Result<Vec<u8>, WeaveError> {
    let (segments, images, mut glyph_sets): LayoutDoc =
        collect_layout(doc, metrics, fonts, layout)?;

    let chrome_face = FaceRef::Bundled(FaceId::SansRegular);
    let mut pages = paginate_items(
        &segments,
        metrics.content_height(),
        layout.page.chrome_reserve(),
    );

    // Glyphs for every expanded chrome string (page digits + title + literals).
    let page_count = pages.len().max(1);
    let title = doc.meta.title.as_str();
    for page_no in 1..=page_count {
        for band in layout.page.bands() {
            if !band.enabled() {
                continue;
            }
            let text =
                crate::knobs::expand_chrome_format(band.format(), page_no, page_count, title);
            collect_glyph_set(
                fonts,
                chrome_face,
                &text,
                glyph_sets.entry(chrome_face).or_default(),
            );
        }
    }

    let dest_pages = collect_dest_pages(&pages);
    let outline_entries = outline::collect_outline_entries(doc, &dest_pages);

    let subsets = prepare_subsets(fonts, &glyph_sets)?;
    remap_pages(&mut pages, &subsets);

    let mut pdf = Pdf::new();
    pdf.set_version(1, 7);
    let catalog_id = Ref::new(1);
    let page_tree_id = Ref::new(2);
    let mut next_id = 3_i32;

    let font_refs = embed_fonts(&mut pdf, fonts, &subsets, &mut next_id)?;
    let image_refs = alloc_image_refs(&images, &mut next_id);
    let (page_ids, content_ids) = alloc_page_refs(pages.len(), &mut next_id);

    outline::write_outline_tree(
        &mut pdf,
        catalog_id,
        page_tree_id,
        &page_ids,
        &outline_entries,
        &mut next_id,
    );
    pdf.pages(page_tree_id)
        .kids(page_ids.iter().copied())
        .count(i32::try_from(page_ids.len()).unwrap_or(i32::MAX));

    write_image_xobjects(&mut pdf, &images, &image_refs);
    WritePagesArgs {
        pdf: &mut pdf,
        pages: &pages,
        metrics,
        page_tree_id,
        page_ids: &page_ids,
        content_ids: &content_ids,
        font_refs: &font_refs,
        fonts,
        image_refs: &image_refs,
        subsets: &subsets,
        knobs: layout,
        title,
        dest_pages: &dest_pages,
        next_id: &mut next_id,
    }
    .run()?;

    let info_id = Ref::new(next_id);
    pdf.document_info(info_id)
        .title(TextStr(&doc.meta.title))
        .creator(TextStr("ariadnes-weave"))
        .producer(TextStr(&format!("ariadnes-weave {}", crate::VERSION)));

    Ok(pdf.finish())
}
