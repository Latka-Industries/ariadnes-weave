//! Print IR → PDF emit pipeline.
//!
//! Stages: layout (`layout`) → pagination (`paginate`) → glyph subset remap
//! (`pdf_write`) → page content paint (`paint`) → PDF objects (`pdf_write`).
//! Structured math layout lives in [`math`]; laid item types in [`types`].

mod layout;
mod math;
mod paginate;
mod paint;
mod pdf_write;
mod types;

#[cfg(test)]
mod tests;

use pdf_writer::{Pdf, Ref, TextStr};

use crate::error::WeaveError;
use crate::font::{FaceId, FaceRef, collect_glyph_set};
use crate::ir::PrintDocument;
use crate::options::EmitOptions;
use crate::profile;
use crate::resolve_fonts::build_font_bag;

use layout::collect_layout;
use paginate::paginate_items;
use pdf_write::{
    WritePagesArgs, alloc_image_refs, alloc_page_refs, embed_fonts, prepare_subsets, remap_pages,
    write_image_xobjects,
};

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
/// writes a PDF 1.7 file with page-number footers.
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
    let (segments, images, mut glyph_sets) = collect_layout(doc, &metrics, &fonts, &layout)?;
    // Digits for page footers (`n / m`).
    let footer_face = FaceRef::Bundled(FaceId::SansRegular);
    collect_glyph_set(
        &fonts,
        footer_face,
        "0123456789 /",
        glyph_sets.entry(footer_face).or_default(),
    );

    let footer_reserve = if layout.page.footer.enabled {
        layout.page.content.bottom_clearance.max(18.0)
    } else {
        0.0
    };
    let mut pages = paginate_items(&segments, metrics.content_height(), footer_reserve);
    let subsets = prepare_subsets(&fonts, &glyph_sets)?;
    remap_pages(&mut pages, &subsets);

    let mut pdf = Pdf::new();
    pdf.set_version(1, 7);
    let catalog_id = Ref::new(1);
    let page_tree_id = Ref::new(2);
    let mut next_id = 3_i32;

    let font_refs = embed_fonts(&mut pdf, &fonts, &subsets, &mut next_id)?;
    let image_refs = alloc_image_refs(&images, &mut next_id);
    let (page_ids, content_ids) = alloc_page_refs(pages.len(), &mut next_id);

    pdf.catalog(catalog_id).pages(page_tree_id);
    pdf.pages(page_tree_id)
        .kids(page_ids.iter().copied())
        .count(i32::try_from(page_ids.len()).unwrap_or(i32::MAX));

    write_image_xobjects(&mut pdf, &images, &image_refs);
    WritePagesArgs {
        pdf: &mut pdf,
        pages: &pages,
        metrics: &metrics,
        page_tree_id,
        page_ids: &page_ids,
        content_ids: &content_ids,
        font_refs: &font_refs,
        fonts: &fonts,
        image_refs: &image_refs,
        subsets: &subsets,
        knobs: &layout,
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
