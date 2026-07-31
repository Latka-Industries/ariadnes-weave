//! Liberation TTF emit from print IR (Type0 + rustybuzz shaping).

use std::collections::BTreeMap;

use pdf_writer::{Content, Name, Pdf, Rect, Ref, Str, TextStr};

use crate::error::WeaveError;
use crate::font::{
    FaceId, FontObjIds, PreparedSubset, ShapedGlyph, collect_glyph_set, encode_gids,
    note_shaped_glyphs, prepare_subset, resource_name, shape_text, shaped_width,
    write_embedded_font,
};
use crate::image_prep::{PreparedImage, prepare_image};
use crate::ir::{
    BreakHint, FigurePlacement, InlineStyle, PrintBlock, PrintDocument, PrintImage,
    SlideRegionContent, TableRow, TextRun,
};
use crate::profile::{self, ProfileMetrics};

type GlyphSet = BTreeMap<u16, String>;
type GlyphSets = BTreeMap<FaceId, GlyphSet>;
type SubsetMap = BTreeMap<FaceId, PreparedSubset>;

/// Emit PDF bytes from a print document.
///
/// # Errors
///
/// Returns [`WeaveError`] if the profile is unsupported, a block kind is not
/// implemented, font subsetting/embedding fails, or an image cannot be decoded.
pub fn emit_pdf(doc: &PrintDocument) -> Result<Vec<u8>, WeaveError> {
    let metrics = profile::resolve_metrics(&doc.profile)?;
    let (segments, images, mut glyph_sets) = collect_layout(doc, &metrics)?;
    collect_glyph_set(
        FaceId::SansRegular,
        "0123456789 /",
        glyph_sets.entry(FaceId::SansRegular).or_default(),
    );

    let mut pages = paginate_items(&segments, metrics.content_height());
    let subsets = prepare_subsets(&glyph_sets)?;
    remap_pages(&mut pages, &subsets);

    let mut pdf = Pdf::new();
    pdf.set_version(1, 7);
    let catalog_id = Ref::new(1);
    let page_tree_id = Ref::new(2);
    let mut next_id = 3_i32;

    let font_refs = embed_fonts(&mut pdf, &subsets, &mut next_id)?;
    let image_refs = alloc_image_refs(&images, &mut next_id);
    let (page_ids, content_ids) = alloc_page_refs(pages.len(), &mut next_id);

    pdf.catalog(catalog_id).pages(page_tree_id);
    pdf.pages(page_tree_id)
        .kids(page_ids.iter().copied())
        .count(i32::try_from(page_ids.len()).unwrap_or(i32::MAX));

    write_image_xobjects(&mut pdf, &images, &image_refs);
    write_pages(
        &mut pdf,
        &pages,
        &metrics,
        page_tree_id,
        &page_ids,
        &content_ids,
        &font_refs,
        &image_refs,
        &subsets,
    )?;

    let info_id = Ref::new(next_id);
    pdf.document_info(info_id)
        .title(TextStr(&doc.meta.title))
        .creator(TextStr("ariadnes-weave"))
        .producer(TextStr(&format!("ariadnes-weave {}", crate::VERSION)));

    Ok(pdf.finish())
}

fn prepare_subsets(glyph_sets: &GlyphSets) -> Result<SubsetMap, WeaveError> {
    let mut subsets = SubsetMap::new();
    for (&face_id, set) in glyph_sets {
        subsets.insert(face_id, prepare_subset(face_id, set)?);
    }
    Ok(subsets)
}

fn embed_fonts(
    pdf: &mut Pdf,
    subsets: &SubsetMap,
    next_id: &mut i32,
) -> Result<BTreeMap<FaceId, Ref>, WeaveError> {
    let mut font_refs = BTreeMap::new();
    for (&face_id, subset) in subsets {
        let ids = FontObjIds {
            type0: Ref::new(*next_id),
            cid: Ref::new(*next_id + 1),
            descriptor: Ref::new(*next_id + 2),
            cmap: Ref::new(*next_id + 3),
            data: Ref::new(*next_id + 4),
        };
        *next_id += 5;
        write_embedded_font(pdf, face_id, &subset.data, &subset.glyph_set, ids)?;
        font_refs.insert(face_id, ids.type0);
    }
    Ok(font_refs)
}

fn alloc_image_refs(images: &[PreparedImage], next_id: &mut i32) -> Vec<(Ref, Option<Ref>)> {
    let mut image_refs = Vec::with_capacity(images.len());
    for img in images {
        let image_id = Ref::new(*next_id);
        *next_id += 1;
        let mask_id = if img.mask.is_some() {
            let id = Ref::new(*next_id);
            *next_id += 1;
            Some(id)
        } else {
            None
        };
        image_refs.push((image_id, mask_id));
    }
    image_refs
}

fn alloc_page_refs(page_count: usize, next_id: &mut i32) -> (Vec<Ref>, Vec<Ref>) {
    let mut page_ids = Vec::with_capacity(page_count);
    let mut content_ids = Vec::with_capacity(page_count);
    for _ in 0..page_count {
        page_ids.push(Ref::new(*next_id));
        *next_id += 1;
        content_ids.push(Ref::new(*next_id));
        *next_id += 1;
    }
    (page_ids, content_ids)
}

fn write_image_xobjects(
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

#[allow(clippy::too_many_arguments)]
fn write_pages(
    pdf: &mut Pdf,
    pages: &[Vec<LaidItem>],
    metrics: &ProfileMetrics,
    page_tree_id: Ref,
    page_ids: &[Ref],
    content_ids: &[Ref],
    font_refs: &BTreeMap<FaceId, Ref>,
    image_refs: &[(Ref, Option<Ref>)],
    subsets: &SubsetMap,
) -> Result<(), WeaveError> {
    let page_count = pages.len().max(1);
    for (page_idx, ((page_id, content_id), page_items)) in page_ids
        .iter()
        .copied()
        .zip(content_ids.iter().copied())
        .zip(pages.iter())
        .enumerate()
    {
        write_page_dict(
            pdf,
            page_id,
            content_id,
            page_tree_id,
            metrics,
            font_refs,
            image_refs,
            page_items,
        );
        let content_bytes =
            build_page_content(page_items, metrics, page_idx + 1, page_count, subsets)?;
        pdf.stream(content_id, &content_bytes);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn write_page_dict(
    pdf: &mut Pdf,
    page_id: Ref,
    content_id: Ref,
    page_tree_id: Ref,
    metrics: &ProfileMetrics,
    font_refs: &BTreeMap<FaceId, Ref>,
    image_refs: &[(Ref, Option<Ref>)],
    page_items: &[LaidItem],
) {
    let used_images: Vec<usize> = page_items
        .iter()
        .filter_map(|item| match item {
            LaidItem::Image { img_idx, .. } => Some(*img_idx),
            LaidItem::Text(_) | LaidItem::Table(_) => None,
        })
        .collect();

    let mut page = pdf.page(page_id);
    page.media_box(Rect::new(0.0, 0.0, metrics.page_w, metrics.page_h));
    page.parent(page_tree_id);
    page.contents(content_id);
    let mut resources = page.resources();
    {
        let mut fonts = resources.fonts();
        for (face_id, type0) in font_refs {
            fonts.pair(Name(resource_name(*face_id)), *type0);
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

fn image_resource_name(idx: usize) -> Vec<u8> {
    format!("Im{idx}").into_bytes()
}

fn remap_pages(pages: &mut [Vec<LaidItem>], subsets: &SubsetMap) {
    for page in pages {
        for item in page {
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
                LaidItem::Image { .. } => {}
            }
        }
    }
}

fn remap_line(line: &mut LaidLine, subsets: &SubsetMap) {
    for span in &mut line.spans {
        if let Some(subset) = subsets.get(&span.face) {
            for g in &mut span.glyphs {
                *g = subset.remap_glyph(*g);
            }
        }
    }
}

#[derive(Debug, Clone)]
struct LaidSpan {
    face: FaceId,
    font_size: f32,
    glyphs: Vec<ShapedGlyph>,
}

#[derive(Debug, Clone)]
struct LaidLine {
    spans: Vec<LaidSpan>,
    leading: f32,
    glue_after: bool,
    /// Left indent inside the content box (points).
    indent: f32,
    /// Center the line within the content box (ignores indent).
    center: bool,
}

impl LaidLine {
    fn gap(leading: f32) -> Self {
        Self {
            spans: Vec::new(),
            leading,
            glue_after: false,
            indent: 0.0,
            center: false,
        }
    }

    fn shaped(
        face: FaceId,
        text: &str,
        font_size: f32,
        leading: f32,
        glyph_sets: &mut GlyphSets,
    ) -> Result<Self, WeaveError> {
        let glyphs = shape_text(face, text, font_size)?;
        let set = glyph_sets.entry(face).or_default();
        collect_glyph_set(face, text, set);
        note_shaped_glyphs(&glyphs, set);
        Ok(Self {
            spans: vec![LaidSpan {
                face,
                font_size,
                glyphs,
            }],
            leading,
            glue_after: false,
            indent: 0.0,
            center: false,
        })
    }

    fn width(&self) -> f32 {
        self.spans.iter().map(|s| shaped_width(&s.glyphs)).sum()
    }
}

#[derive(Debug, Clone)]
struct LaidTableRow {
    height: f32,
    cells: Vec<Vec<LaidLine>>,
}

#[derive(Debug, Clone)]
struct LaidTable {
    col_widths: Vec<f32>,
    rows: Vec<LaidTableRow>,
    pad: f32,
    gap_after: f32,
}

impl LaidTable {
    fn height(&self) -> f32 {
        self.rows.iter().map(|r| r.height).sum::<f32>() + self.gap_after
    }
}

#[derive(Debug, Clone)]
enum LaidItem {
    Text(LaidLine),
    Image {
        img_idx: usize,
        width: f32,
        height: f32,
        glue_after: bool,
    },
    Table(LaidTable),
}

impl LaidItem {
    fn height(&self) -> f32 {
        match self {
            Self::Text(line) => line.leading,
            Self::Image { height, .. } => *height + 8.0,
            Self::Table(table) => table.height(),
        }
    }

    fn glue_after(&self) -> bool {
        match self {
            Self::Text(line) => line.glue_after,
            Self::Image { glue_after, .. } => *glue_after,
            Self::Table(_) => false,
        }
    }

    fn set_glue_after(&mut self, glue: bool) {
        match self {
            Self::Text(line) => line.glue_after = glue,
            Self::Image { glue_after, .. } => *glue_after = glue,
            Self::Table(_) => {}
        }
    }
}

type LayoutSegment = (ForcedBreak, Vec<LaidItem>);
type LayoutDoc = (Vec<LayoutSegment>, Vec<PreparedImage>, GlyphSets);

fn collect_layout(doc: &PrintDocument, metrics: &ProfileMetrics) -> Result<LayoutDoc, WeaveError> {
    let mut segments: Vec<(ForcedBreak, Vec<LaidItem>)> = vec![(ForcedBreak::None, Vec::new())];
    let mut images: Vec<PreparedImage> = Vec::new();
    let mut glyph_sets: GlyphSets = BTreeMap::new();

    for block in &doc.blocks {
        layout_block(block, metrics, &mut segments, &mut images, &mut glyph_sets)?;
    }

    Ok((segments, images, glyph_sets))
}

fn layout_block(
    block: &PrintBlock,
    metrics: &ProfileMetrics,
    segments: &mut Vec<LayoutSegment>,
    images: &mut Vec<PreparedImage>,
    glyph_sets: &mut GlyphSets,
) -> Result<(), WeaveError> {
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
        } => layout_heading(*level, runs, *break_before, metrics, segments, glyph_sets)?,
        PrintBlock::Paragraph { runs } => {
            let seg = segments.last_mut().expect("segment");
            push_styled_runs(
                &mut seg.1,
                runs,
                metrics,
                glyph_sets,
                body_layout(metrics, 0.0),
            )?;
        }
        PrintBlock::Quote { runs } => layout_quote(runs, metrics, segments, glyph_sets)?,
        PrintBlock::Code { lang: _, text } => {
            layout_code(text, metrics, segments, glyph_sets)?;
        }
        PrintBlock::List { ordered, items } => {
            let seg = segments.last_mut().expect("segment");
            push_list_lines(&mut seg.1, *ordered, items, 0, metrics, glyph_sets)?;
        }
        PrintBlock::Table { rows } => {
            let seg = segments.last_mut().expect("segment");
            push_table(&mut seg.1, rows, metrics, glyph_sets)?;
        }
        PrintBlock::Figure {
            image,
            alt,
            caption,
            placement,
        } => {
            push_figure(
                segments, images, image, alt, caption, *placement, metrics, glyph_sets,
            )?;
        }
        PrintBlock::Math { display, latex } => {
            layout_math(*display, latex, metrics, segments, glyph_sets)?;
        }
        PrintBlock::Slide { layout_id, regions } => {
            layout_slide(layout_id, regions, metrics, segments, glyph_sets)?;
        }
    }
    Ok(())
}

fn body_layout(metrics: &ProfileMetrics, indent: f32) -> RunLayout {
    RunLayout {
        font_size: metrics.body_size,
        leading: metrics.body_leading,
        gap_after: 10.0,
        glue_last_content: false,
        mode: FaceMode::Body,
        indent,
    }
}

fn layout_heading(
    level: u8,
    runs: &[TextRun],
    break_before: BreakHint,
    metrics: &ProfileMetrics,
    segments: &mut Vec<LayoutSegment>,
    glyph_sets: &mut GlyphSets,
) -> Result<(), WeaveError> {
    let profile_h1_break = metrics.force_h1_page_break && level == 1;
    let hint_break = matches!(break_before, BreakHint::Page | BreakHint::PageAlways);
    if (profile_h1_break || hint_break)
        && segments.last().is_some_and(|(_, items)| !items.is_empty())
    {
        segments.push((ForcedBreak::Always, Vec::new()));
    }
    let font_size = profile::heading_size(level, metrics);
    let glue = matches!(break_before, BreakHint::KeepWithNext) || level <= 2;
    let seg = segments.last_mut().expect("segment");
    push_styled_runs(
        &mut seg.1,
        runs,
        metrics,
        glyph_sets,
        RunLayout {
            font_size,
            leading: font_size * 1.35,
            gap_after: 8.0,
            glue_last_content: glue,
            mode: FaceMode::Heading,
            indent: 0.0,
        },
    )
}

fn layout_quote(
    runs: &[TextRun],
    metrics: &ProfileMetrics,
    segments: &mut [LayoutSegment],
    glyph_sets: &mut GlyphSets,
) -> Result<(), WeaveError> {
    let seg = segments.last_mut().expect("segment");
    let mut quoted = vec![TextRun {
        text: "\"".into(),
        style: InlineStyle {
            emphasis: true,
            ..InlineStyle::default()
        },
    }];
    quoted.extend(runs.iter().cloned());
    quoted.push(TextRun {
        text: "\"".into(),
        style: InlineStyle {
            emphasis: true,
            ..InlineStyle::default()
        },
    });
    push_styled_runs(
        &mut seg.1,
        &quoted,
        metrics,
        glyph_sets,
        body_layout(metrics, 18.0),
    )
}

fn layout_code(
    text: &str,
    metrics: &ProfileMetrics,
    segments: &mut [LayoutSegment],
    glyph_sets: &mut GlyphSets,
) -> Result<(), WeaveError> {
    let seg = segments.last_mut().expect("segment");
    let font_size = metrics.code_size;
    let leading = font_size * 1.25;
    for line in text.lines() {
        seg.1.push(LaidItem::Text(LaidLine::shaped(
            FaceId::MonoRegular,
            line,
            font_size,
            leading,
            glyph_sets,
        )?));
    }
    seg.1.push(LaidItem::Text(LaidLine::gap(10.0)));
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum FaceMode {
    Body,
    Heading,
}

fn resolve_face(style: InlineStyle, metrics: &ProfileMetrics, mode: FaceMode) -> FaceId {
    match mode {
        FaceMode::Heading => {
            let mut s = style;
            if !s.code && !s.strong && !s.emphasis {
                s.strong = true;
            }
            FaceId::from_style(&s, false)
        }
        FaceMode::Body => FaceId::from_style(&style, metrics.serif_body),
    }
}

#[derive(Clone, Copy)]
struct RunLayout {
    font_size: f32,
    leading: f32,
    gap_after: f32,
    glue_last_content: bool,
    mode: FaceMode,
    indent: f32,
}

#[allow(clippy::too_many_arguments)]
fn push_figure(
    segments: &mut [LayoutSegment],
    images: &mut Vec<PreparedImage>,
    image: &PrintImage,
    alt: &str,
    caption: &[TextRun],
    placement: FigurePlacement,
    metrics: &ProfileMetrics,
    glyph_sets: &mut GlyphSets,
) -> Result<(), WeaveError> {
    let float_near = matches!(placement, FigurePlacement::FloatNear);
    let seg = segments.last_mut().expect("segment");
    if float_near {
        // Keep the figure with the preceding block when possible.
        if let Some(prev) = seg
            .1
            .iter_mut()
            .rev()
            .find(|item| !matches!(item, LaidItem::Text(line) if line.spans.is_empty()))
        {
            prev.set_glue_after(true);
        }
    }

    let Ok(prepared) = prepare_image(image) else {
        let label = if alt.is_empty() {
            "[figure]".into()
        } else {
            format!("[figure: {alt}]")
        };
        let mut line = LaidLine::shaped(
            FaceId::SansItalic,
            &label,
            metrics.body_size,
            metrics.body_leading,
            glyph_sets,
        )?;
        line.glue_after = !caption.is_empty() || float_near;
        seg.1.push(LaidItem::Text(line));
        if caption.is_empty() {
            seg.1.push(LaidItem::Text(LaidLine::gap(10.0)));
        } else {
            push_styled_runs(
                &mut seg.1,
                caption,
                metrics,
                glyph_sets,
                RunLayout {
                    font_size: metrics.body_size,
                    leading: metrics.body_leading,
                    gap_after: 10.0,
                    glue_last_content: false,
                    mode: FaceMode::Body,
                    indent: 0.0,
                },
            )?;
        }
        return Ok(());
    };

    let (w, h) = prepared.fit_width(metrics.content_width());
    let img_idx = images.len();
    images.push(prepared);

    seg.1.push(LaidItem::Image {
        img_idx,
        width: w,
        height: h,
        glue_after: !caption.is_empty() || float_near,
    });
    if caption.is_empty() {
        seg.1.push(LaidItem::Text(LaidLine::gap(6.0)));
    } else {
        push_styled_runs(
            &mut seg.1,
            caption,
            metrics,
            glyph_sets,
            RunLayout {
                font_size: metrics.body_size,
                leading: metrics.body_leading,
                gap_after: 10.0,
                glue_last_content: false,
                mode: FaceMode::Body,
                indent: 0.0,
            },
        )?;
    }
    Ok(())
}

fn layout_math(
    display: bool,
    latex: &str,
    metrics: &ProfileMetrics,
    segments: &mut [LayoutSegment],
    glyph_sets: &mut GlyphSets,
) -> Result<(), WeaveError> {
    let pretty = prettify_latex_math(latex);
    let face = if metrics.serif_body {
        FaceId::SerifItalic
    } else {
        FaceId::SansItalic
    };
    let font_size = if display {
        metrics.body_size * 1.15
    } else {
        metrics.body_size
    };
    let leading = font_size * 1.4;
    let seg = segments.last_mut().expect("segment");
    if display {
        seg.1.push(LaidItem::Text(LaidLine::gap(8.0)));
    }
    let mut line = LaidLine::shaped(face, &pretty, font_size, leading, glyph_sets)?;
    line.center = display;
    seg.1.push(LaidItem::Text(line));
    if display {
        seg.1.push(LaidItem::Text(LaidLine::gap(12.0)));
    } else {
        seg.1.push(LaidItem::Text(LaidLine::gap(4.0)));
    }
    Ok(())
}

/// Light LaTeX-math prettifier (delimiters + common tokens). Not a TeX engine.
fn prettify_latex_math(latex: &str) -> String {
    let mut s = latex.trim().to_string();
    for wrap in ["$$", "$", "\\[", "\\]", "\\(", "\\)"] {
        if let Some(stripped) = s.strip_prefix(wrap) {
            s = stripped.to_string();
        }
        if let Some(stripped) = s.strip_suffix(wrap) {
            s = stripped.to_string();
        }
    }
    s = s.trim().to_string();
    let replacements = [
        ("\\times", "×"),
        ("\\cdot", "·"),
        ("\\pm", "±"),
        ("\\leq", "≤"),
        ("\\geq", "≥"),
        ("\\neq", "≠"),
        ("\\approx", "≈"),
        ("\\infty", "∞"),
        ("\\rightarrow", "→"),
        ("\\leftarrow", "←"),
        ("\\Rightarrow", "⇒"),
        ("\\alpha", "α"),
        ("\\beta", "β"),
        ("\\gamma", "γ"),
        ("\\delta", "δ"),
        ("\\epsilon", "ε"),
        ("\\theta", "θ"),
        ("\\lambda", "λ"),
        ("\\mu", "μ"),
        ("\\pi", "π"),
        ("\\sigma", "σ"),
        ("\\phi", "φ"),
        ("\\omega", "ω"),
        ("\\sum", "∑"),
        ("\\prod", "∏"),
        ("\\int", "∫"),
        ("\\sqrt", "√"),
        ("\\ldots", "…"),
        ("\\dots", "…"),
        ("\\ ", " "),
        ("\\,", " "),
        ("\\;", " "),
        ("\\!", ""),
        ("{", ""),
        ("}", ""),
    ];
    for (from, to) in replacements {
        s = s.replace(from, to);
    }
    // Very light superscripts / subscripts for single digits or letters.
    s = apply_script_chars(&s, '^', true);
    s = apply_script_chars(&s, '_', false);
    if s.is_empty() { "[math]".into() } else { s }
}

fn apply_script_chars(input: &str, marker: char, super_script: bool) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == marker
            && let Some(&next) = chars.peek()
        {
            let mapped = if super_script {
                to_superscript(next)
            } else {
                to_subscript(next)
            };
            if let Some(rep) = mapped {
                out.push(rep);
                chars.next();
                continue;
            }
        }
        out.push(ch);
    }
    out
}

fn to_superscript(ch: char) -> Option<char> {
    Some(match ch {
        '0' => '⁰',
        '1' => '¹',
        '2' => '²',
        '3' => '³',
        '4' => '⁴',
        '5' => '⁵',
        '6' => '⁶',
        '7' => '⁷',
        '8' => '⁸',
        '9' => '⁹',
        '+' => '⁺',
        '-' => '⁻',
        'n' => 'ⁿ',
        'i' => 'ⁱ',
        _ => return None,
    })
}

fn to_subscript(ch: char) -> Option<char> {
    Some(match ch {
        '0' => '₀',
        '1' => '₁',
        '2' => '₂',
        '3' => '₃',
        '4' => '₄',
        '5' => '₅',
        '6' => '₆',
        '7' => '₇',
        '8' => '₈',
        '9' => '₉',
        '+' => '₊',
        '-' => '₋',
        'n' => 'ₙ',
        'i' => 'ᵢ',
        _ => return None,
    })
}

fn push_table(
    out: &mut Vec<LaidItem>,
    rows: &[TableRow],
    metrics: &ProfileMetrics,
    glyph_sets: &mut GlyphSets,
) -> Result<(), WeaveError> {
    if rows.is_empty() {
        out.push(LaidItem::Text(LaidLine::shaped(
            FaceId::SansItalic,
            "[table]",
            metrics.body_size,
            metrics.body_leading,
            glyph_sets,
        )?));
        out.push(LaidItem::Text(LaidLine::gap(10.0)));
        return Ok(());
    }

    let cols = rows.iter().map(|r| r.cells.len()).max().unwrap_or(0).max(1);
    let pad = 5.0_f32;
    let font_size = metrics.body_size;
    let leading = metrics.body_leading.min(font_size * 1.25);
    let face = if metrics.serif_body {
        FaceId::SerifRegular
    } else {
        FaceId::SansRegular
    };
    let header_face = if metrics.serif_body {
        FaceId::SerifBold
    } else {
        FaceId::SansBold
    };
    let col_width = metrics.content_width() / cols as f32;
    let inner_width = (col_width - pad * 2.0).max(24.0);
    let col_widths = vec![col_width; cols];

    let mut laid_rows = Vec::with_capacity(rows.len());
    for (row_idx, row) in rows.iter().enumerate() {
        let mut cells = Vec::with_capacity(cols);
        let mut row_h = pad * 2.0 + leading;
        for col in 0..cols {
            let text = row.cells.get(col).map_or("", String::as_str);
            let cell_face = if row_idx == 0 { header_face } else { face };
            let lines =
                wrap_plain_text(text, cell_face, font_size, leading, inner_width, glyph_sets)?;
            let content_h = if lines.is_empty() {
                leading
            } else {
                lines.iter().map(|l| l.leading).sum::<f32>()
            };
            row_h = row_h.max(pad * 2.0 + content_h);
            cells.push(lines);
        }
        laid_rows.push(LaidTableRow {
            height: row_h,
            cells,
        });
    }

    out.push(LaidItem::Table(LaidTable {
        col_widths,
        rows: laid_rows,
        pad,
        gap_after: 12.0,
    }));
    Ok(())
}

fn wrap_plain_text(
    text: &str,
    face: FaceId,
    font_size: f32,
    leading: f32,
    max_width: f32,
    glyph_sets: &mut GlyphSets,
) -> Result<Vec<LaidLine>, WeaveError> {
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let mut lines = Vec::new();
    let mut current: Vec<LaidSpan> = Vec::new();
    let mut current_width = 0.0_f32;
    let mut remaining = text;
    while !remaining.is_empty() {
        let (chunk, rest) = next_wrap_chunk(remaining);
        remaining = rest;
        let chunk = chunk.trim_end();
        if chunk.is_empty() {
            continue;
        }
        let glyphs = shape_text(face, chunk, font_size)?;
        let w = shaped_width(&glyphs);
        if current_width + w > max_width && !current.is_empty() {
            lines.push(LaidLine {
                spans: std::mem::take(&mut current),
                leading,
                glue_after: false,
                indent: 0.0,
                center: false,
            });
            current_width = 0.0;
        }
        if w > max_width && current.is_empty() {
            for piece in hard_break_text(face, chunk, font_size, max_width)? {
                let glyphs = shape_text(face, &piece, font_size)?;
                let set = glyph_sets.entry(face).or_default();
                collect_glyph_set(face, &piece, set);
                note_shaped_glyphs(&glyphs, set);
                lines.push(LaidLine {
                    spans: vec![LaidSpan {
                        face,
                        font_size,
                        glyphs,
                    }],
                    leading,
                    glue_after: false,
                    indent: 0.0,
                    center: false,
                });
            }
            continue;
        }
        let set = glyph_sets.entry(face).or_default();
        collect_glyph_set(face, chunk, set);
        note_shaped_glyphs(&glyphs, set);
        current.push(LaidSpan {
            face,
            font_size,
            glyphs,
        });
        current_width += w;
    }
    if !current.is_empty() {
        lines.push(LaidLine {
            spans: current,
            leading,
            glue_after: false,
            indent: 0.0,
            center: false,
        });
    }
    Ok(lines)
}

fn layout_slide(
    layout_id: &str,
    regions: &[SlideRegionContent],
    metrics: &ProfileMetrics,
    segments: &mut Vec<LayoutSegment>,
    glyph_sets: &mut GlyphSets,
) -> Result<(), WeaveError> {
    let _ = layout_id;
    if segments.last().is_some_and(|(_, items)| !items.is_empty()) {
        segments.push((ForcedBreak::Always, Vec::new()));
    }

    let seg = segments.last_mut().expect("segment");
    let title_scale = if metrics.is_deck { 1.45 } else { 1.8 };
    let title_gap = if metrics.is_deck { 20.0 } else { 16.0 };
    let (titles, rest): (Vec<_>, Vec<_>) = regions.iter().partition(|r| {
        let slot = r.slot.to_ascii_lowercase();
        slot == "title" || slot == "heading" || slot.ends_with(".title")
    });

    if metrics.is_deck {
        seg.1.push(LaidItem::Text(LaidLine::gap(12.0)));
    }

    if titles.is_empty() && rest.is_empty() {
        seg.1.push(LaidItem::Text(LaidLine::shaped(
            FaceId::SansItalic,
            "[empty slide]",
            metrics.body_size,
            metrics.body_leading,
            glyph_sets,
        )?));
    } else {
        for region in titles {
            push_styled_runs(
                &mut seg.1,
                &[TextRun {
                    text: region.text.clone(),
                    style: InlineStyle {
                        strong: true,
                        ..InlineStyle::default()
                    },
                }],
                metrics,
                glyph_sets,
                RunLayout {
                    font_size: metrics.body_size * title_scale,
                    leading: metrics.body_size * title_scale * 1.2,
                    gap_after: title_gap,
                    glue_last_content: false,
                    mode: FaceMode::Heading,
                    indent: 0.0,
                },
            )?;
        }
        for region in rest {
            let slot = region.slot.to_ascii_lowercase();
            let (size, gap, mode, strong) = if slot == "subtitle" || slot.ends_with(".subtitle") {
                (metrics.body_size * 1.15, 14.0, FaceMode::Body, false)
            } else {
                (metrics.body_size, 12.0, FaceMode::Body, false)
            };
            if !matches!(slot.as_str(), "body" | "content" | "text") && !region.slot.is_empty() {
                push_styled_runs(
                    &mut seg.1,
                    &[TextRun {
                        text: format!("{}:", region.slot),
                        style: InlineStyle {
                            strong: true,
                            ..InlineStyle::default()
                        },
                    }],
                    metrics,
                    glyph_sets,
                    RunLayout {
                        font_size: metrics.body_size * 0.85,
                        leading: metrics.body_leading,
                        gap_after: 2.0,
                        glue_last_content: true,
                        mode: FaceMode::Body,
                        indent: 0.0,
                    },
                )?;
            }
            push_styled_runs(
                &mut seg.1,
                &[TextRun {
                    text: region.text.clone(),
                    style: InlineStyle {
                        strong,
                        emphasis: slot == "subtitle" || slot.ends_with(".subtitle"),
                        ..InlineStyle::default()
                    },
                }],
                metrics,
                glyph_sets,
                RunLayout {
                    font_size: size,
                    leading: size * 1.35,
                    gap_after: gap,
                    glue_last_content: false,
                    mode,
                    indent: 0.0,
                },
            )?;
        }
    }

    segments.push((ForcedBreak::Always, Vec::new()));
    Ok(())
}

fn push_styled_runs(
    out: &mut Vec<LaidItem>,
    runs: &[TextRun],
    metrics: &ProfileMetrics,
    glyph_sets: &mut GlyphSets,
    layout: RunLayout,
) -> Result<(), WeaveError> {
    if runs.is_empty() {
        return Ok(());
    }

    let start = out.len();
    let max_width = (metrics.content_width() - layout.indent).max(36.0);
    let mut current_spans: Vec<LaidSpan> = Vec::new();
    let mut current_width = 0.0_f32;

    let flush_line = |spans: &mut Vec<LaidSpan>, dest: &mut Vec<LaidItem>, glue: bool| {
        if spans.is_empty() {
            return;
        }
        dest.push(LaidItem::Text(LaidLine {
            spans: std::mem::take(spans),
            leading: layout.leading,
            glue_after: glue,
            indent: layout.indent,
            center: false,
        }));
    };

    for run in runs {
        let face = resolve_face(run.style, metrics, layout.mode);
        let mut remaining = run.text.as_str();
        while !remaining.is_empty() {
            let (chunk, rest) = next_wrap_chunk(remaining);
            remaining = rest;
            let chunk = chunk.trim_end();
            if chunk.is_empty() {
                continue;
            }
            let glyphs = shape_text(face, chunk, layout.font_size)?;
            let w = shaped_width(&glyphs);
            if current_width + w > max_width && !current_spans.is_empty() {
                flush_line(&mut current_spans, out, false);
                current_width = 0.0;
            }
            if w > max_width && current_spans.is_empty() {
                // Hard-break tokens wider than the content box (URLs, long code).
                for piece in hard_break_text(face, chunk, layout.font_size, max_width)? {
                    let glyphs = shape_text(face, &piece, layout.font_size)?;
                    let set = glyph_sets.entry(face).or_default();
                    collect_glyph_set(face, &piece, set);
                    note_shaped_glyphs(&glyphs, set);
                    current_spans.push(LaidSpan {
                        face,
                        font_size: layout.font_size,
                        glyphs,
                    });
                    flush_line(&mut current_spans, out, false);
                    current_width = 0.0;
                }
                continue;
            }
            let set = glyph_sets.entry(face).or_default();
            collect_glyph_set(face, chunk, set);
            note_shaped_glyphs(&glyphs, set);
            current_spans.push(LaidSpan {
                face,
                font_size: layout.font_size,
                glyphs,
            });
            current_width += w;
        }
    }

    flush_line(&mut current_spans, out, layout.glue_last_content);
    let content_end = out.len();
    apply_widow_orphan(&mut out[start..content_end]);
    if layout.gap_after > 0.0 {
        out.push(LaidItem::Text(LaidLine::gap(layout.gap_after)));
    }
    Ok(())
}

/// Keep at least two content lines together at paragraph start/end.
fn apply_widow_orphan(items: &mut [LaidItem]) {
    let idxs: Vec<usize> = items
        .iter()
        .enumerate()
        .filter_map(|(i, item)| match item {
            LaidItem::Text(line) if !line.spans.is_empty() => Some(i),
            _ => None,
        })
        .collect();
    if idxs.len() < 2 {
        return;
    }
    if let LaidItem::Text(line) = &mut items[idxs[0]] {
        line.glue_after = true;
    }
    let penultimate = idxs[idxs.len() - 2];
    if let LaidItem::Text(line) = &mut items[penultimate] {
        line.glue_after = true;
    }
}

/// Split `text` into pieces that each fit within `max_width` points.
fn hard_break_text(
    face: FaceId,
    text: &str,
    font_size: f32,
    max_width: f32,
) -> Result<Vec<String>, WeaveError> {
    let mut pieces = Vec::new();
    let mut buf = String::new();
    for ch in text.chars() {
        let mut trial = buf.clone();
        trial.push(ch);
        let tw = shaped_width(&shape_text(face, &trial, font_size)?);
        if tw > max_width && !buf.is_empty() {
            pieces.push(std::mem::take(&mut buf));
            buf.push(ch);
        } else {
            buf.push(ch);
        }
    }
    if !buf.is_empty() {
        pieces.push(buf);
    }
    if pieces.is_empty() {
        pieces.push(String::new());
    }
    Ok(pieces)
}

/// Take the next whitespace-delimited chunk (word + trailing spaces).
fn next_wrap_chunk(s: &str) -> (&str, &str) {
    let mut chars = s.char_indices();
    let Some((_, first)) = chars.next() else {
        return ("", "");
    };
    if first.is_whitespace() {
        let end = s.find(|c: char| !c.is_whitespace()).unwrap_or(s.len());
        return (&s[..end], &s[end..]);
    }
    let word_end = s.find(char::is_whitespace).unwrap_or(s.len());
    let after_ws = s[word_end..]
        .find(|c: char| !c.is_whitespace())
        .map_or(s.len(), |i| word_end + i);
    (&s[..after_ws], &s[after_ws..])
}

fn push_list_lines(
    out: &mut Vec<LaidItem>,
    ordered: bool,
    items: &[crate::ir::ListItem],
    depth: usize,
    metrics: &ProfileMetrics,
    glyph_sets: &mut GlyphSets,
) -> Result<(), WeaveError> {
    for (i, item) in items.iter().enumerate() {
        let marker = if ordered {
            format!("{}. ", i + 1)
        } else {
            "• ".into()
        };
        let mut runs = vec![TextRun::plain(marker)];
        runs.extend(item.runs.iter().cloned());
        push_styled_runs(
            out,
            &runs,
            metrics,
            glyph_sets,
            RunLayout {
                font_size: metrics.body_size,
                leading: metrics.body_size * 1.35,
                gap_after: 0.0,
                glue_last_content: false,
                mode: FaceMode::Body,
                indent: 18.0 * depth as f32,
            },
        )?;
        for child in &item.children {
            match child {
                PrintBlock::List {
                    ordered: child_ordered,
                    items: child_items,
                } => push_list_lines(
                    out,
                    *child_ordered,
                    child_items,
                    depth + 1,
                    metrics,
                    glyph_sets,
                )?,
                other => return Err(WeaveError::UnsupportedBlock(block_name(other))),
            }
        }
    }
    out.push(LaidItem::Text(LaidLine::gap(8.0)));
    Ok(())
}

fn block_name(block: &PrintBlock) -> &'static str {
    match block {
        PrintBlock::Heading { .. } => "heading",
        PrintBlock::Paragraph { .. } => "paragraph",
        PrintBlock::List { .. } => "list",
        PrintBlock::Code { .. } => "code",
        PrintBlock::Quote { .. } => "quote",
        PrintBlock::Table { .. } => "table",
        PrintBlock::Figure { .. } => "figure",
        PrintBlock::Math { .. } => "math",
        PrintBlock::Slide { .. } => "slide",
        PrintBlock::Break(_) => "break",
    }
}

#[derive(Debug, Clone, Copy)]
enum ForcedBreak {
    None,
    Always,
}

fn paginate_items(
    segments: &[(ForcedBreak, Vec<LaidItem>)],
    max_y_span: f32,
) -> Vec<Vec<LaidItem>> {
    let max_y_span = (max_y_span - 18.0).max(72.0);
    let mut pages: Vec<Vec<LaidItem>> = Vec::new();
    let mut current: Vec<LaidItem> = Vec::new();
    let mut used = 0.0_f32;

    let flush = |pages: &mut Vec<Vec<LaidItem>>, current: &mut Vec<LaidItem>, used: &mut f32| {
        if !current.is_empty() {
            pages.push(std::mem::take(current));
            *used = 0.0;
        } else if pages.is_empty() {
            pages.push(Vec::new());
        }
    };

    for (forced, items) in segments {
        if matches!(forced, ForcedBreak::Always) && !current.is_empty() {
            flush(&mut pages, &mut current, &mut used);
        }
        for item in items {
            let h = item.height();
            if used + h > max_y_span && !current.is_empty() {
                let mut peeled: Vec<LaidItem> = Vec::new();
                while current.last().is_some_and(LaidItem::glue_after) {
                    let peeled_item = current.pop().expect("last");
                    used -= peeled_item.height();
                    peeled.push(peeled_item);
                }
                if current.is_empty() {
                    while let Some(l) = peeled.pop() {
                        used += l.height();
                        current.push(l);
                    }
                    flush(&mut pages, &mut current, &mut used);
                } else {
                    flush(&mut pages, &mut current, &mut used);
                    while let Some(l) = peeled.pop() {
                        used += l.height();
                        current.push(l);
                    }
                }
                if used + h > max_y_span && !current.is_empty() {
                    flush(&mut pages, &mut current, &mut used);
                }
            }
            used += h;
            current.push(item.clone());
        }
    }

    if current.is_empty() && pages.is_empty() {
        pages.push(Vec::new());
    } else if !current.is_empty() {
        pages.push(current);
    }

    pages
}

fn build_page_content(
    items: &[LaidItem],
    metrics: &ProfileMetrics,
    page_no: usize,
    page_count: usize,
    subsets: &SubsetMap,
) -> Result<Vec<u8>, WeaveError> {
    let mut content = Content::new();
    let mut y = metrics.page_h - metrics.margin;

    for item in items {
        match item {
            LaidItem::Text(line) => {
                y -= line.leading;
                if y < metrics.margin + 18.0 {
                    break;
                }
                if line.spans.is_empty() {
                    continue;
                }
                content.begin_text();
                let mut x = if line.center {
                    metrics.margin + (metrics.content_width() - line.width()) / 2.0
                } else {
                    metrics.margin + line.indent
                };
                for span in &line.spans {
                    content.set_font(Name(resource_name(span.face)), span.font_size);
                    content.set_text_matrix([1.0, 0.0, 0.0, 1.0, x, y]);
                    let encoded = encode_gids(&span.glyphs);
                    content.show(Str(&encoded));
                    x += shaped_width(&span.glyphs);
                }
                content.end_text();
            }
            LaidItem::Image {
                img_idx,
                width,
                height,
                glue_after: _,
            } => {
                y -= *height;
                if y < metrics.margin + 18.0 {
                    break;
                }
                let name = image_resource_name(*img_idx);
                content.save_state();
                content.transform([*width, 0.0, 0.0, *height, metrics.margin, y]);
                content.x_object(Name(&name));
                content.restore_state();
                y -= 8.0;
            }
            LaidItem::Table(table) => {
                let table_h = table.rows.iter().map(|r| r.height).sum::<f32>();
                if y - table_h < metrics.margin + 18.0 {
                    break;
                }
                paint_table(&mut content, table, metrics.margin, y)?;
                y -= table_h + table.gap_after;
            }
        }
    }

    let footer = format!("{page_no} / {page_count}");
    let mut footer_glyphs = shape_text(FaceId::SansRegular, &footer, 9.0)?;
    if let Some(subset) = subsets.get(&FaceId::SansRegular) {
        for g in &mut footer_glyphs {
            *g = subset.remap_glyph(*g);
        }
    }
    let footer_w = shaped_width(&footer_glyphs);
    let footer_y = metrics.margin * 0.45;
    let footer_x = (metrics.page_w - footer_w) / 2.0;
    content.begin_text();
    content.set_font(Name(resource_name(FaceId::SansRegular)), 9.0);
    content.set_text_matrix([1.0, 0.0, 0.0, 1.0, footer_x, footer_y]);
    content.show(Str(&encode_gids(&footer_glyphs)));
    content.end_text();

    Ok(content.finish().into_vec())
}

fn paint_table(
    content: &mut Content,
    table: &LaidTable,
    origin_x: f32,
    top_y: f32,
) -> Result<(), WeaveError> {
    let table_w: f32 = table.col_widths.iter().sum();
    let table_h: f32 = table.rows.iter().map(|r| r.height).sum();
    let bottom = top_y - table_h;

    content.save_state();
    content.set_stroke_gray(0.25);
    content.set_line_width(0.6);
    content.rect(origin_x, bottom, table_w, table_h);
    content.stroke();

    let mut y = top_y;
    for row in &table.rows {
        y -= row.height;
        if (y - bottom).abs() > 0.01 {
            content.move_to(origin_x, y);
            content.line_to(origin_x + table_w, y);
            content.stroke();
        }
    }

    let mut x = origin_x;
    for width in table
        .col_widths
        .iter()
        .take(table.col_widths.len().saturating_sub(1))
    {
        x += width;
        content.move_to(x, top_y);
        content.line_to(x, bottom);
        content.stroke();
    }
    content.restore_state();

    let mut row_top = top_y;
    for row in &table.rows {
        let mut cell_x = origin_x;
        for (col, cell_lines) in row.cells.iter().enumerate() {
            let col_w = table.col_widths.get(col).copied().unwrap_or(0.0);
            let mut text_y = row_top - table.pad;
            for line in cell_lines {
                text_y -= line.leading;
                if line.spans.is_empty() {
                    continue;
                }
                content.begin_text();
                let mut span_x = cell_x + table.pad;
                for span in &line.spans {
                    content.set_font(Name(resource_name(span.face)), span.font_size);
                    content.set_text_matrix([1.0, 0.0, 0.0, 1.0, span_x, text_y]);
                    content.show(Str(&encode_gids(&span.glyphs)));
                    span_x += shaped_width(&span.glyphs);
                }
                content.end_text();
            }
            cell_x += col_w;
        }
        row_top -= row.height;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{InlineStyle, PrintMeta, PrintProfileId, TableRow};
    use image::{ImageBuffer, ImageFormat, Rgb};

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
                        "Owned print IR to PDF with Liberation Sans + rustybuzz.",
                    )],
                },
            ],
        }
    }

    fn tiny_png_bytes() -> Vec<u8> {
        let img: ImageBuffer<Rgb<u8>, Vec<u8>> =
            ImageBuffer::from_fn(32, 24, |x, y| Rgb([x as u8 * 7, y as u8 * 9, 180]));
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, ImageFormat::Png)
            .expect("encode png");
        buf.into_inner()
    }

    #[test]
    fn emits_pdf_magic() {
        let bytes = emit_pdf(&hello_doc()).expect("emit");
        assert!(bytes.starts_with(b"%PDF-"));
        assert!(bytes.windows(5).any(|w| w == b"%%EOF"));
    }

    #[test]
    fn subsetted_hello_pdf_is_compact() {
        let bytes = emit_pdf(&hello_doc()).expect("emit");
        // Full Liberation Sans alone is ~400KB; subset prose should stay well under that.
        assert!(
            bytes.len() < 80_000,
            "expected subsetted PDF < 80KB, got {}",
            bytes.len()
        );
    }

    #[test]
    fn hard_breaks_overlong_token() {
        let long = "A".repeat(80);
        let doc = PrintDocument {
            meta: PrintMeta {
                title: "Long".into(),
                doc_kind: "note".into(),
                language: None,
                source_doc_id: None,
            },
            profile: PrintProfileId::print_v0(),
            blocks: vec![PrintBlock::Paragraph {
                runs: vec![TextRun::plain(long)],
            }],
        };
        let bytes = emit_pdf(&doc).expect("emit overlong");
        assert!(bytes.starts_with(b"%PDF-"));
    }

    #[test]
    fn rejects_unknown_profile() {
        let mut doc = hello_doc();
        doc.profile = PrintProfileId {
            name: "manuscript".into(),
            version: 1,
        };
        assert!(matches!(
            emit_pdf(&doc).unwrap_err(),
            WeaveError::UnsupportedProfile { .. }
        ));
    }

    #[test]
    fn accepts_manuscript_v0() {
        let mut doc = hello_doc();
        doc.profile = PrintProfileId::manuscript_v0();
        assert!(emit_pdf(&doc).expect("emit").starts_with(b"%PDF-"));
    }

    #[test]
    fn styled_runs_embed_bold_font() {
        let mut doc = hello_doc();
        doc.blocks.push(PrintBlock::Paragraph {
            runs: vec![
                TextRun::plain("plain "),
                TextRun {
                    text: "bold".into(),
                    style: InlineStyle {
                        strong: true,
                        ..InlineStyle::default()
                    },
                },
            ],
        });
        let bytes = emit_pdf(&doc).expect("emit");
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("LiberationSans-Bold"));
        assert!(s.contains("/Subtype /Type0") || s.contains("/Subtype/Type0"));
    }

    #[test]
    fn emits_unicode_em_dash() {
        let doc = PrintDocument {
            meta: PrintMeta {
                title: "Dash".into(),
                doc_kind: "note".into(),
                language: None,
                source_doc_id: None,
            },
            profile: PrintProfileId::print_v0(),
            blocks: vec![PrintBlock::Paragraph {
                runs: vec![TextRun::plain("alpha — omega")],
            }],
        };
        let bytes = emit_pdf(&doc).expect("emit unicode");
        assert!(bytes.starts_with(b"%PDF-"));
    }

    #[test]
    fn table_draws_grid_paths() {
        let doc = PrintDocument {
            meta: PrintMeta {
                title: "T".into(),
                doc_kind: "note".into(),
                language: None,
                source_doc_id: None,
            },
            profile: PrintProfileId::print_v0(),
            blocks: vec![PrintBlock::Table {
                rows: vec![
                    TableRow {
                        cells: vec!["Name".into(), "Value".into()],
                    },
                    TableRow {
                        cells: vec!["alpha".into(), "1".into()],
                    },
                ],
            }],
        };
        let bytes = emit_pdf(&doc).expect("emit");
        assert!(bytes.starts_with(b"%PDF-"));
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("LiberationSans"));
        // Table grid sets a distinctive stroke width before path ops.
        assert!(
            s.contains("0.6 w"),
            "expected table stroke width in content stream"
        );
    }

    #[test]
    fn slide_emits_own_page() {
        use crate::ir::SlideRegionContent;
        let doc = PrintDocument {
            meta: PrintMeta {
                title: "Deck".into(),
                doc_kind: "deck".into(),
                language: None,
                source_doc_id: None,
            },
            profile: PrintProfileId::print_v0(),
            blocks: vec![
                PrintBlock::Paragraph {
                    runs: vec![TextRun::plain("Before slides.")],
                },
                PrintBlock::Slide {
                    layout_id: "title-body".into(),
                    regions: vec![
                        SlideRegionContent {
                            slot: "title".into(),
                            text: "First slide".into(),
                        },
                        SlideRegionContent {
                            slot: "body".into(),
                            text: "Bullet ideas live here.".into(),
                        },
                    ],
                },
                PrintBlock::Slide {
                    layout_id: "title-body".into(),
                    regions: vec![SlideRegionContent {
                        slot: "title".into(),
                        text: "Second slide".into(),
                    }],
                },
                PrintBlock::Paragraph {
                    runs: vec![TextRun::plain("After slides.")],
                },
            ],
        };
        let bytes = emit_pdf(&doc).expect("emit");
        let page_dicts = bytes.windows(10).filter(|w| *w == b"/Type /Pag").count();
        assert!(
            page_dicts >= 3,
            "prose + 2 slides should span >= 3 pages; got {page_dicts}"
        );
    }

    #[test]
    fn manuscript_emphasis_uses_serif_italic() {
        let doc = PrintDocument {
            meta: PrintMeta {
                title: "MS".into(),
                doc_kind: "manuscript".into(),
                language: None,
                source_doc_id: None,
            },
            profile: PrintProfileId::manuscript_v0(),
            blocks: vec![PrintBlock::Paragraph {
                runs: vec![
                    TextRun::plain("plain "),
                    TextRun {
                        text: "emph".into(),
                        style: InlineStyle {
                            emphasis: true,
                            ..InlineStyle::default()
                        },
                    },
                    TextRun::plain(" "),
                    TextRun {
                        text: "strong".into(),
                        style: InlineStyle {
                            strong: true,
                            ..InlineStyle::default()
                        },
                    },
                ],
            }],
        };
        let bytes = emit_pdf(&doc).expect("emit");
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("LiberationSerif-Italic"));
        assert!(s.contains("LiberationSerif-Bold"));
    }

    #[test]
    fn figure_png_embeds_xobject() {
        let png = tiny_png_bytes();
        let doc = PrintDocument {
            meta: PrintMeta {
                title: "Fig".into(),
                doc_kind: "note".into(),
                language: None,
                source_doc_id: None,
            },
            profile: PrintProfileId::print_v0(),
            blocks: vec![
                PrintBlock::Heading {
                    level: 1,
                    runs: vec![TextRun::plain("With figure")],
                    break_before: BreakHint::None,
                },
                PrintBlock::Figure {
                    image: PrintImage {
                        bytes: png,
                        media_type: "image/png".into(),
                        width_px: Some(32),
                        height_px: Some(24),
                    },
                    alt: "swatch".into(),
                    caption: vec![TextRun::plain("A tiny PNG.")],
                    placement: FigurePlacement::Flow,
                },
            ],
        };
        let bytes = emit_pdf(&doc).expect("emit");
        assert!(bytes.starts_with(b"%PDF-"));
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("/Subtype /Image") || s.contains("/Subtype/Image"));
        std::fs::create_dir_all("tmp").ok();
        std::fs::write("tmp/figure_sample.pdf", &bytes).ok();
    }

    #[test]
    fn deck_v0_is_landscape_16x9() {
        let metrics = profile::resolve_metrics(&PrintProfileId::deck_v0()).expect("deck");
        assert!(metrics.is_deck);
        assert!((metrics.page_w / metrics.page_h - 16.0 / 9.0).abs() < 0.01);
        let doc = PrintDocument {
            meta: PrintMeta {
                title: "Deck".into(),
                doc_kind: "deck".into(),
                language: None,
                source_doc_id: None,
            },
            profile: PrintProfileId::deck_v0(),
            blocks: vec![PrintBlock::Slide {
                layout_id: "title-body".into(),
                regions: vec![
                    SlideRegionContent {
                        slot: "title".into(),
                        text: "Landscape deck".into(),
                    },
                    SlideRegionContent {
                        slot: "body".into(),
                        text: "Large type on 16:9.".into(),
                    },
                ],
            }],
        };
        let bytes = emit_pdf(&doc).expect("emit deck");
        assert!(bytes.starts_with(b"%PDF-"));
        std::fs::create_dir_all("tmp").ok();
        std::fs::write("tmp/deck_sample.pdf", &bytes).ok();
    }

    #[test]
    fn math_prettify_and_emit() {
        assert_eq!(prettify_latex_math(r"$E = mc^2$"), "E = mc²");
        assert!(prettify_latex_math(r"\alpha + \beta").contains('α'));
        let doc = PrintDocument {
            meta: PrintMeta {
                title: "Math".into(),
                doc_kind: "note".into(),
                language: None,
                source_doc_id: None,
            },
            profile: PrintProfileId::print_v0(),
            blocks: vec![PrintBlock::Math {
                display: true,
                latex: r"E = mc^2".into(),
            }],
        };
        let bytes = emit_pdf(&doc).expect("emit math");
        assert!(bytes.starts_with(b"%PDF-"));
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("LiberationSans-Italic") || s.contains("LiberationSerif-Italic"));
    }

    #[test]
    fn float_near_figure_emits() {
        let png = tiny_png_bytes();
        let doc = PrintDocument {
            meta: PrintMeta {
                title: "Float".into(),
                doc_kind: "note".into(),
                language: None,
                source_doc_id: None,
            },
            profile: PrintProfileId::print_v0(),
            blocks: vec![
                PrintBlock::Paragraph {
                    runs: vec![TextRun::plain("See the figure nearby.")],
                },
                PrintBlock::Figure {
                    image: PrintImage {
                        bytes: png,
                        media_type: "image/png".into(),
                        width_px: Some(32),
                        height_px: Some(24),
                    },
                    alt: "swatch".into(),
                    caption: vec![TextRun::plain("Caption.")],
                    placement: FigurePlacement::FloatNear,
                },
            ],
        };
        assert!(emit_pdf(&doc).expect("emit").starts_with(b"%PDF-"));
    }
}
