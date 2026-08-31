//! PDF object graph: fonts, images, page dicts, and subset GID remapping.

use std::collections::BTreeMap;

use pdf_writer::types::{ActionType, AnnotationType, BorderType};
use pdf_writer::{Finish, Name, Pdf, Rect, Ref, Str};

use crate::error::WeaveError;
use crate::font::{FaceRef, FontBag, FontObjIds, prepare_subset, write_embedded_font};
use crate::image_prep::PreparedImage;
use crate::knobs::LayoutKnobs;
use crate::profile::ProfileMetrics;

use super::paint::{
    BuildPageContent, PageLink, PageLinkTarget, build_page_content, image_resource_name,
};
use super::types::{GlyphSets, LaidItem, LaidLine, SubsetMap};

/// Subset each face that contributed glyphs during layout.
pub(super) fn prepare_subsets(
    fonts: &FontBag,
    glyph_sets: &GlyphSets,
) -> Result<SubsetMap, WeaveError> {
    let mut subsets = SubsetMap::new();
    for (&face, set) in glyph_sets {
        subsets.insert(face, prepare_subset(fonts, face, set)?);
    }
    Ok(subsets)
}

/// Embed Type0 subsets; returns face → Type0 object ref (`BTreeMap` for determinism).
pub(super) fn embed_fonts(
    pdf: &mut Pdf,
    fonts: &FontBag,
    subsets: &SubsetMap,
    next_id: &mut i32,
) -> Result<BTreeMap<FaceRef, Ref>, WeaveError> {
    let mut font_refs = BTreeMap::new();
    for (&face, subset) in subsets {
        let ids = FontObjIds {
            type0: Ref::new(*next_id),
            cid: Ref::new(*next_id + 1),
            descriptor: Ref::new(*next_id + 2),
            cmap: Ref::new(*next_id + 3),
            data: Ref::new(*next_id + 4),
        };
        *next_id += 5;
        write_embedded_font(pdf, fonts, face, &subset.data, &subset.glyph_set, ids)?;
        font_refs.insert(face, ids.type0);
    }
    Ok(font_refs)
}

pub(super) fn alloc_image_refs(
    images: &[PreparedImage],
    next_id: &mut i32,
) -> Vec<(Ref, Option<Ref>)> {
    let mut image_refs = Vec::with_capacity(images.len());
    for img in images {
        let image_id = alloc_ref(next_id);
        let mask_id = if img.mask.is_some() {
            Some(alloc_ref(next_id))
        } else {
            None
        };
        image_refs.push((image_id, mask_id));
    }
    image_refs
}

pub(super) fn alloc_page_refs(page_count: usize, next_id: &mut i32) -> (Vec<Ref>, Vec<Ref>) {
    let mut page_ids = Vec::with_capacity(page_count);
    let mut content_ids = Vec::with_capacity(page_count);
    for _ in 0..page_count {
        page_ids.push(alloc_ref(next_id));
        content_ids.push(alloc_ref(next_id));
    }
    (page_ids, content_ids)
}

/// Next PDF object id, bumping the allocator.
#[inline]
pub(super) fn alloc_ref(next_id: &mut i32) -> Ref {
    let id = Ref::new(*next_id);
    *next_id += 1;
    id
}

/// Resolve an internal dest id to its page object ref.
pub(super) fn page_ref_for_dest(
    dest_pages: &BTreeMap<String, usize>,
    page_ids: &[Ref],
    dest_id: &str,
) -> Option<Ref> {
    dest_pages
        .get(dest_id)
        .and_then(|&idx| page_ids.get(idx).copied())
}

pub(super) fn write_image_xobjects(
    pdf: &mut Pdf,
    images: &[PreparedImage],
    image_refs: &[(Ref, Option<Ref>)],
) {
    for (img, (image_id, mask_id)) in images.iter().zip(image_refs.iter()) {
        {
            let mut image = pdf.image_xobject(*image_id, &img.samples);
            image.filter(img.filter);
            image.width(i32::try_from(img.width).unwrap_or(i32::MAX));
            image.height(i32::try_from(img.height).unwrap_or(i32::MAX));
            image.color_space().device_rgb();
            image.bits_per_component(8);
            if let Some(mid) = mask_id {
                image.s_mask(*mid);
            }
        }
        if let (Some(mid), Some(mask_samples)) = (mask_id, &img.mask) {
            let mut s_mask = pdf.image_xobject(*mid, mask_samples);
            s_mask.filter(img.filter);
            s_mask.width(i32::try_from(img.width).unwrap_or(i32::MAX));
            s_mask.height(i32::try_from(img.height).unwrap_or(i32::MAX));
            s_mask.color_space().device_gray();
            s_mask.bits_per_component(8);
        }
    }
}

/// Arguments for writing all page dictionaries and content streams.
pub(super) struct WritePagesArgs<'a> {
    pub pdf: &'a mut Pdf,
    pub pages: &'a [Vec<LaidItem>],
    pub metrics: &'a ProfileMetrics,
    pub page_tree_id: Ref,
    pub page_ids: &'a [Ref],
    pub content_ids: &'a [Ref],
    pub font_refs: &'a BTreeMap<FaceRef, Ref>,
    pub fonts: &'a FontBag,
    pub image_refs: &'a [(Ref, Option<Ref>)],
    pub subsets: &'a SubsetMap,
    pub knobs: &'a LayoutKnobs,
    pub title: &'a str,
    pub headings: &'a [String],
    pub dest_pages: &'a BTreeMap<String, usize>,
    pub notes: &'a super::notes::NoteBook,
    pub next_id: &'a mut i32,
}

impl WritePagesArgs<'_> {
    /// Write each page dict + painted content stream + link annotations.
    pub(super) fn run(self) -> Result<(), WeaveError> {
        let Self {
            pdf,
            pages,
            metrics,
            page_tree_id,
            page_ids,
            content_ids,
            font_refs,
            fonts,
            image_refs,
            subsets,
            knobs,
            title,
            headings,
            dest_pages,
            notes,
            next_id,
        } = self;
        let page_count = pages.len().max(1);
        let mut footnote_carry = Vec::new();
        for (page_idx, ((page_id, content_id), page_items)) in page_ids
            .iter()
            .copied()
            .zip(content_ids.iter().copied())
            .zip(pages.iter())
            .enumerate()
        {
            let heading = headings.get(page_idx).map_or("", String::as_str);
            let (content_bytes, page_links) = build_page_content(BuildPageContent {
                items: page_items,
                metrics,
                page_no: page_idx + 1,
                page_count,
                title,
                heading,
                fonts,
                subsets,
                knobs,
                notes,
                footnote_carry: &mut footnote_carry,
            })?;
            let mut annot_ids = Vec::with_capacity(page_links.len());
            for _ in &page_links {
                annot_ids.push(alloc_ref(next_id));
            }
            WritePageDictArgs {
                pdf,
                page_id,
                content_id,
                page_tree_id,
                metrics,
                font_refs,
                fonts,
                image_refs,
                page_items,
                annot_ids: &annot_ids,
            }
            .run();
            for (annot_id, link) in annot_ids.iter().copied().zip(page_links.iter()) {
                write_link(pdf, annot_id, link, dest_pages, page_ids);
            }
            pdf.stream(content_id, &content_bytes);
        }
        Ok(())
    }
}

fn write_link(
    pdf: &mut Pdf,
    annot_id: Ref,
    link: &PageLink,
    dest_pages: &BTreeMap<String, usize>,
    page_ids: &[Ref],
) {
    let mut annotation = pdf.annotation(annot_id);
    annotation.subtype(AnnotationType::Link);
    annotation.rect(Rect::new(link.x0, link.y0, link.x1, link.y1));
    match &link.target {
        PageLinkTarget::Uri(uri) => {
            annotation
                .action()
                .action_type(ActionType::Uri)
                .uri(Str(uri.as_bytes()));
        }
        PageLinkTarget::Dest { id } => {
            if let Some(page_ref) = page_ref_for_dest(dest_pages, page_ids, id) {
                annotation
                    .action()
                    .action_type(ActionType::GoTo)
                    .destination()
                    .page(page_ref)
                    .fit();
            }
        }
    }
    // Invisible border — color comes from the painted text/icons.
    annotation
        .border_style()
        .width(0.0)
        .style(BorderType::Solid);
    annotation.finish();
}

/// Arguments for one page dictionary (media box, resources, contents).
pub(super) struct WritePageDictArgs<'a> {
    pub pdf: &'a mut Pdf,
    pub page_id: Ref,
    pub content_id: Ref,
    pub page_tree_id: Ref,
    pub metrics: &'a ProfileMetrics,
    pub font_refs: &'a BTreeMap<FaceRef, Ref>,
    pub fonts: &'a FontBag,
    pub image_refs: &'a [(Ref, Option<Ref>)],
    pub page_items: &'a [LaidItem],
    pub annot_ids: &'a [Ref],
}

impl WritePageDictArgs<'_> {
    /// Emit the page object; only attaches image `XObjects` used on this page.
    pub(super) fn run(self) {
        let Self {
            pdf,
            page_id,
            content_id,
            page_tree_id,
            metrics,
            font_refs,
            fonts,
            image_refs,
            page_items,
            annot_ids,
        } = self;
        let used_images: Vec<usize> = page_items
            .iter()
            .filter_map(|item| match item {
                LaidItem::Image { img_idx, .. } => Some(*img_idx),
                LaidItem::Text(_)
                | LaidItem::Table(_)
                | LaidItem::Columns(_)
                | LaidItem::Callout(_)
                | LaidItem::Math(_)
                | LaidItem::Rule { .. } => None,
            })
            .collect();

        let mut page = pdf.page(page_id);
        page.media_box(Rect::new(0.0, 0.0, metrics.page_w, metrics.page_h));
        page.parent(page_tree_id);
        page.contents(content_id);
        if !annot_ids.is_empty() {
            page.annotations(annot_ids.iter().copied());
        }
        let mut resources = page.resources();
        {
            let mut font_res = resources.fonts();
            for (face, type0) in font_refs {
                let name = fonts.resource_name(*face);
                font_res.pair(Name(&name), *type0);
            }
        }
        if !used_images.is_empty() {
            let mut xobjs = resources.x_objects();
            for idx in &used_images {
                let name = image_resource_name(*idx);
                xobjs.pair(Name(&name), image_refs[*idx].0);
            }
        }
    }
}

/// Remap shaped GIDs from full-face space into subset GID space.
pub(super) fn remap_pages(pages: &mut [Vec<LaidItem>], subsets: &SubsetMap) {
    for page in pages {
        remap_items(page, subsets);
    }
}

pub(super) fn remap_items(items: &mut [LaidItem], subsets: &SubsetMap) {
    for item in items {
        match item {
            LaidItem::Text(line) => remap_line(line, subsets),
            LaidItem::Table(table) => {
                for row in &mut table.rows {
                    for cell in &mut row.cells {
                        for line in cell {
                            remap_line(line, subsets);
                        }
                    }
                }
            }
            LaidItem::Columns(cols) => {
                for column in &mut cols.columns {
                    for line in column {
                        remap_line(line, subsets);
                    }
                }
            }
            LaidItem::Callout(band) => {
                for line in &mut band.lines {
                    remap_line(line, subsets);
                }
            }
            LaidItem::Math(math) => {
                for el in &mut math.elements {
                    if let super::types::LaidMathEl::Text { face, glyphs, .. } = el {
                        remap_glyphs(*face, glyphs, subsets);
                    }
                }
            }
            LaidItem::Image { .. } | LaidItem::Rule { .. } => {}
        }
    }
}

fn remap_line(line: &mut LaidLine, subsets: &SubsetMap) {
    for span in &mut line.spans {
        remap_glyphs(span.face, &mut span.glyphs, subsets);
    }
    for span in &mut line.gutter_spans {
        remap_glyphs(span.face, &mut span.glyphs, subsets);
    }
}

fn remap_glyphs(
    face: crate::font::FaceRef,
    glyphs: &mut [crate::font::ShapedGlyph],
    subsets: &SubsetMap,
) {
    if let Some(subset) = subsets.get(&face) {
        for g in glyphs {
            *g = subset.remap_glyph(*g);
        }
    }
}
