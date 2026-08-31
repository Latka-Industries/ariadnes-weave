//! Build PDF content streams from laid items (text, images, tables).

use std::collections::{BTreeMap, BTreeSet};

use pdf_writer::{Content, Name, Str};

use crate::error::WeaveError;
use crate::font::{FaceId, FaceRef, FontBag, encode_gids, shape_text, shaped_width};
use crate::knobs::{FigureAlign, LayoutKnobs, PageChromeBand, TextAlign};
use crate::profile::ProfileMetrics;

use super::math::paint_math;
use super::types::{LaidColumns, LaidItem, LaidLine, LaidMath, LaidSpan, LaidTable, SubsetMap};

/// Clickable box collected while painting a page (PDF user space).
#[derive(Debug, Clone)]
pub(super) struct PageLink {
    pub target: PageLinkTarget,
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

/// URI external link or internal `GoTo` destination id.
#[derive(Debug, Clone)]
pub(super) enum PageLinkTarget {
    Uri(String),
    Dest { id: String },
}

fn link_rect_for_span(origin_x: f32, baseline_y: f32, span: &LaidSpan) -> Option<PageLink> {
    let target = if let Some(uri) = span.link_uri.as_ref() {
        if uri.is_empty() {
            None
        } else {
            Some(PageLinkTarget::Uri(uri.clone()))
        }
    } else if let Some(id) = span.link_dest.as_ref() {
        if id.is_empty() {
            None
        } else {
            Some(PageLinkTarget::Dest { id: id.clone() })
        }
    } else {
        None
    }?;
    let width = shaped_width(&span.glyphs);
    if width <= 0.0 {
        return None;
    }
    let y = baseline_y + span.baseline_shift;
    // Ink box around the baseline (generous hit target for icons).
    let pad = span.font_size * 0.15;
    Some(PageLink {
        target,
        x0: origin_x - pad * 0.25,
        y0: y - span.font_size * 0.25 - pad,
        x1: origin_x + width + pad * 0.25,
        y1: y + span.font_size * 0.85 + pad,
    })
}

fn push_span_links(links: &mut Vec<PageLink>, spans: &[LaidSpan], origin_x: f32, baseline_y: f32) {
    let mut x = origin_x;
    for span in spans {
        if let Some(hit) = link_rect_for_span(x, baseline_y, span) {
            links.push(hit);
        }
        x += shaped_width(&span.glyphs);
    }
}

fn paint_span_text(
    content: &mut Content,
    fonts: &FontBag,
    span: &LaidSpan,
    origin_x: f32,
    baseline_y: f32,
    last_fill: &mut Option<[f32; 3]>,
) {
    apply_fill_rgb(content, last_fill, span.fill);
    let face_name = fonts.resource_name(span.face);
    content.set_font(Name(&face_name), span.font_size);
    let y = baseline_y + span.baseline_shift;
    content.set_text_matrix([1.0, 0.0, 0.0, 1.0, origin_x, y]);
    content.show(Str(&encode_gids(&span.glyphs)));
}

fn stroke_span_underline(
    content: &mut Content,
    span: &LaidSpan,
    origin_x: f32,
    baseline_y: f32,
    width: f32,
) {
    if !span.underline || width <= 0.0 {
        return;
    }
    let [red, green, blue] = span.fill;
    let underline_y = baseline_y - span.font_size * 0.12;
    content.save_state();
    content.set_stroke_rgb(red, green, blue);
    content.set_line_width((span.font_size * 0.06).max(0.4));
    content.move_to(origin_x, underline_y);
    content.line_to(origin_x + width, underline_y);
    content.stroke();
    content.restore_state();
}

fn paint_span_underlines(
    content: &mut Content,
    spans: &[LaidSpan],
    mut origin_x: f32,
    baseline_y: f32,
) {
    for span in spans {
        let width = shaped_width(&span.glyphs);
        stroke_span_underline(content, span, origin_x, baseline_y, width);
        origin_x += width;
    }
}

/// Paint a horizontal run of spans (text object + optional underlines).
///
/// `last_fill` is the page content-stream fill, not per-line: PDF `/rg`
/// survives `ET`/`BT`, so a new line that starts black must still restore
/// after a colored link (resume rows, following bullets).
fn paint_laid_spans(
    content: &mut Content,
    fonts: &FontBag,
    spans: &[LaidSpan],
    origin_x: f32,
    baseline_y: f32,
    last_fill: &mut Option<[f32; 3]>,
) {
    if spans.is_empty() {
        return;
    }
    content.begin_text();
    let mut x = origin_x;
    for span in spans {
        paint_span_text(content, fonts, span, x, baseline_y, last_fill);
        x += shaped_width(&span.glyphs);
    }
    content.end_text();
    paint_span_underlines(content, spans, origin_x, baseline_y);
}

/// Per-line justify geometry: stretch slots, discarded trailing WS, optical width.
struct JustifyPlan {
    space_slots: Vec<(usize, usize)>,
    trailing: BTreeSet<(usize, usize)>,
    /// Advance width excluding trailing WS, with last glyph ink flush.
    natural: f32,
}

fn plan_justify(spans: &[LaidSpan]) -> JustifyPlan {
    let mut space_slots: Vec<(usize, usize)> = Vec::new();
    for (si, span) in spans.iter().enumerate() {
        for (gi, glyph) in span.glyphs.iter().enumerate() {
            if glyph.is_whitespace {
                space_slots.push((si, gi));
            }
        }
    }
    let mut trailing = BTreeSet::new();
    while space_slots
        .last()
        .is_some_and(|&(si, gi)| is_line_tail_whitespace(spans, si, gi))
    {
        if let Some(slot) = space_slots.pop() {
            trailing.insert(slot);
        }
    }

    let mut natural_advance = 0.0_f32;
    let mut last_non_ws = None;
    for (si, span) in spans.iter().enumerate() {
        for (gi, glyph) in span.glyphs.iter().enumerate() {
            if trailing.contains(&(si, gi)) {
                continue;
            }
            natural_advance += glyph.advance;
            if !glyph.is_whitespace {
                last_non_ws = Some(*glyph);
            }
        }
    }
    let natural = match last_non_ws {
        Some(glyph) if glyph.ink_x_max > 0.0 && glyph.ink_x_max < glyph.advance => {
            natural_advance - glyph.advance + glyph.ink_x_max
        }
        _ => natural_advance,
    };
    JustifyPlan {
        space_slots,
        trailing,
        natural,
    }
}

/// Word-justify spans across `measure` by padding inter-word whitespace advances.
///
/// Trailing whitespace is discarded for width/slack (and not painted). Slack is
/// computed so the **ink** of the last glyph (not its advance box) meets the band
/// end — otherwise side-bearing leaves a visible hairline short of the figure edge.
fn paint_justified_spans(
    content: &mut Content,
    fonts: &FontBag,
    spans: &[LaidSpan],
    origin_x: f32,
    baseline_y: f32,
    measure: f32,
    last_fill: &mut Option<[f32; 3]>,
) {
    if spans.is_empty() {
        return;
    }
    let plan = plan_justify(spans);
    let slack = (measure - plan.natural).max(0.0);
    let space_pads = distribute_justify_pads(&plan.space_slots, slack);

    content.begin_text();
    let mut x = origin_x;
    for (si, span) in spans.iter().enumerate() {
        apply_fill_rgb(content, last_fill, span.fill);
        let face_name = fonts.resource_name(span.face);
        content.set_font(Name(&face_name), span.font_size);
        for (gi, glyph) in span.glyphs.iter().enumerate() {
            if plan.trailing.contains(&(si, gi)) {
                continue;
            }
            content.set_text_matrix([1.0, 0.0, 0.0, 1.0, x, baseline_y + span.baseline_shift]);
            content.show(Str(&encode_gids(std::slice::from_ref(glyph))));
            x += glyph.advance;
            if let Some(&pad) = space_pads.get(&(si, gi)) {
                x += pad;
            }
        }
    }
    content.end_text();
    paint_span_underlines_justified(
        content,
        spans,
        origin_x,
        baseline_y,
        &space_pads,
        &plan.trailing,
    );
}

/// Equal gap pads with residual on the last slot so advances sum exactly to slack.
fn distribute_justify_pads(
    space_slots: &[(usize, usize)],
    slack: f32,
) -> BTreeMap<(usize, usize), f32> {
    let mut pads = BTreeMap::new();
    if space_slots.is_empty() || slack <= 0.0 {
        return pads;
    }
    let n = space_slots.len();
    let each = slack / n as f32;
    let mut used = 0.0_f32;
    for (i, &slot) in space_slots.iter().enumerate() {
        let pad = if i + 1 == n {
            (slack - used).max(0.0)
        } else {
            each
        };
        used += pad;
        pads.insert(slot, pad);
    }
    pads
}

fn is_line_tail_whitespace(spans: &[LaidSpan], span_i: usize, glyph_i: usize) -> bool {
    // True when every glyph after (span_i, glyph_i) is also whitespace (or absent).
    for (si, span) in spans.iter().enumerate().skip(span_i) {
        let start = if si == span_i { glyph_i + 1 } else { 0 };
        for glyph in span.glyphs.iter().skip(start) {
            if !glyph.is_whitespace {
                return false;
            }
        }
    }
    true
}

fn paint_span_underlines_justified(
    content: &mut Content,
    spans: &[LaidSpan],
    mut origin_x: f32,
    baseline_y: f32,
    space_pads: &BTreeMap<(usize, usize), f32>,
    trailing: &BTreeSet<(usize, usize)>,
) {
    for (si, span) in spans.iter().enumerate() {
        let width: f32 = span
            .glyphs
            .iter()
            .enumerate()
            .filter(|(gi, _)| !trailing.contains(&(si, *gi)))
            .map(|(gi, g)| g.advance + space_pads.get(&(si, gi)).copied().unwrap_or(0.0))
            .sum();
        stroke_span_underline(content, span, origin_x, baseline_y, width);
        origin_x += width;
    }
}

/// Emit `/rg` only when fill changes. Default PDF fill is black, so the first
/// black span stays silent (byte-stable fixtures). `last` is page-level: after
/// a colored link, later black runs (next line, next row pane, next block)
/// must emit `0 0 0 rg` or they inherit the cite/link accent.
fn apply_fill_rgb(content: &mut Content, last: &mut Option<[f32; 3]>, fill: [f32; 3]) {
    let is_black = rgb_bits(fill) == rgb_bits([0.0, 0.0, 0.0]);
    match *last {
        None if is_black => {}
        Some(prev) if rgb_bits(prev) == rgb_bits(fill) => {}
        _ => {
            content.set_fill_rgb(fill[0], fill[1], fill[2]);
            *last = Some(fill);
        }
    }
}

fn rgb_bits(rgb: [f32; 3]) -> [u32; 3] {
    [rgb[0].to_bits(), rgb[1].to_bits(), rgb[2].to_bits()]
}

/// Resource name bytes for image `XObject` `Im{idx}`.
pub(super) fn image_resource_name(idx: usize) -> Vec<u8> {
    format!("Im{idx}").into_bytes()
}

/// Paint one page's items top-down and append optional header/footer chrome.
///
/// Returns content bytes plus URI link boxes for `/Annots`.
///
/// # Errors
///
/// Returns [`WeaveError::Font`] if chrome shaping fails.
pub(super) struct BuildPageContent<'a> {
    pub items: &'a [LaidItem],
    pub metrics: &'a ProfileMetrics,
    pub page_no: usize,
    pub page_count: usize,
    pub title: &'a str,
    pub heading: &'a str,
    pub fonts: &'a FontBag,
    pub subsets: &'a SubsetMap,
    pub knobs: &'a LayoutKnobs,
    pub notes: &'a super::notes::NoteBook,
    pub footnote_carry: &'a mut Vec<LaidItem>,
}

impl BuildPageContent<'_> {
    pub(super) fn run(self) -> Result<(Vec<u8>, Vec<PageLink>), WeaveError> {
        let mut content = Content::new();
        let mut links = Vec::new();
        let header_drop = self.knobs.page.header_reserve();
        let fn_reserve = self
            .knobs
            .page
            .footnote_reserve(self.notes.has_footnote_defs());
        let mut y = self.metrics.page_h - self.metrics.margin - header_drop;
        let bottom_limit =
            self.metrics.margin + self.knobs.page.content.bottom_clearance + fn_reserve;

        paint_page_chrome_band(PaintChromeBand {
            content: &mut content,
            metrics: self.metrics,
            page_no: self.page_no,
            page_count: self.page_count,
            title: self.title,
            heading: self.heading,
            fonts: self.fonts,
            subsets: self.subsets,
            band: &self.knobs.page.header,
            baseline_y: self.metrics.page_h
                - self.metrics.margin * self.knobs.page.header.y_margin_factor(),
        })?;

        let mut last_fill;
        {
            let mut cursor = PageCursor {
                content: &mut content,
                links: &mut links,
                y: &mut y,
                bottom_limit,
                metrics: self.metrics,
                fonts: self.fonts,
                knobs: self.knobs,
                last_fill: None,
            };
            for item in self.items {
                if !cursor.paint_item(item) {
                    break;
                }
            }
            last_fill = cursor.last_fill;
        }

        paint_footnote_band(PaintFootnotes {
            content: &mut content,
            links: &mut links,
            items: self.items,
            notes: self.notes,
            carry: self.footnote_carry,
            metrics: self.metrics,
            fonts: self.fonts,
            knobs: self.knobs,
            last_fill: &mut last_fill,
        });

        paint_page_chrome_band(PaintChromeBand {
            content: &mut content,
            metrics: self.metrics,
            page_no: self.page_no,
            page_count: self.page_count,
            title: self.title,
            heading: self.heading,
            fonts: self.fonts,
            subsets: self.subsets,
            band: &self.knobs.page.footer,
            baseline_y: self.metrics.margin * self.knobs.page.footer.y_margin_factor(),
        })?;
        Ok((content.finish().into_vec(), links))
    }
}

/// Paint one page's items top-down and append optional header/footer chrome.
///
/// Returns content bytes plus URI link boxes for `/Annots`.
///
/// # Errors
///
/// Returns [`WeaveError::Font`] if chrome shaping fails.
pub(super) fn build_page_content(
    args: BuildPageContent<'_>,
) -> Result<(Vec<u8>, Vec<PageLink>), WeaveError> {
    args.run()
}

struct PaintFootnotes<'a> {
    content: &'a mut Content,
    links: &'a mut Vec<PageLink>,
    items: &'a [LaidItem],
    notes: &'a super::notes::NoteBook,
    carry: &'a mut Vec<LaidItem>,
    metrics: &'a ProfileMetrics,
    fonts: &'a FontBag,
    knobs: &'a LayoutKnobs,
    last_fill: &'a mut Option<[f32; 3]>,
}

fn paint_footnote_band(args: PaintFootnotes<'_>) {
    let PaintFootnotes {
        content,
        links,
        items,
        notes,
        carry,
        metrics,
        fonts,
        knobs,
        last_fill,
    } = args;
    if !notes.has_footnote_defs() && carry.is_empty() {
        return;
    }
    let mut band = std::mem::take(carry);
    for id in super::notes::page_note_ids(items) {
        if notes
            .defs
            .get(&id)
            .is_some_and(|d| d.kind == crate::ir::NoteKind::Footnote)
            && let Some(laid) = notes.laid_footnotes.get(&id)
        {
            band.extend(laid.iter().cloned());
        }
    }
    if band.is_empty() {
        return;
    }
    let footer_top = metrics.margin + knobs.page.footer_reserve();
    let max_band = knobs.page.footnote.max_band.max(0.0);
    if max_band <= 0.0 {
        *carry = band;
        return;
    }
    let rule_y = footer_top + max_band;
    paint_layout_rule(
        content,
        metrics.margin,
        rule_y + knobs.page.footnote.gap_before_rule,
        metrics.content_width() * 0.35,
        knobs.page.footnote.rule_thickness,
        knobs.page.footnote.rule_thickness,
        knobs.page.chrome.stroke_gray,
    );
    let mut y = rule_y - knobs.page.footnote.gap_before_rule;
    let mut used = 0usize;
    for (i, item) in band.iter().enumerate() {
        let h = item.height();
        if y - h < footer_top {
            break;
        }
        match item {
            LaidItem::Text(line) => {
                if !paint_text_item(
                    content, links, line, &mut y, footer_top, metrics, fonts, last_fill,
                ) {
                    break;
                }
            }
            _ => {
                y -= h;
            }
        }
        used = i + 1;
    }
    *carry = band.split_off(used);
}

struct PageCursor<'a> {
    content: &'a mut Content,
    links: &'a mut Vec<PageLink>,
    y: &'a mut f32,
    bottom_limit: f32,
    metrics: &'a ProfileMetrics,
    fonts: &'a FontBag,
    knobs: &'a LayoutKnobs,
    last_fill: Option<[f32; 3]>,
}

impl PageCursor<'_> {
    /// Paint one laid item; returns `false` when the cursor is below `bottom_limit`.
    fn paint_item(&mut self, item: &LaidItem) -> bool {
        match item {
            LaidItem::Text(line) => paint_text_item(
                self.content,
                self.links,
                line,
                self.y,
                self.bottom_limit,
                self.metrics,
                self.fonts,
                &mut self.last_fill,
            ),
            LaidItem::Image {
                img_idx,
                width,
                height,
                glue_after: _,
                gap_after,
                align,
            } => paint_image_item(
                self.content,
                &ImagePaint {
                    img_idx: *img_idx,
                    width: *width,
                    height: *height,
                    gap_after: *gap_after,
                    align: *align,
                },
                self.y,
                self.bottom_limit,
                self.metrics,
            ),
            LaidItem::Table(table) => self.paint_table_item(table),
            LaidItem::Columns(cols) => self.paint_columns_item(cols),
            LaidItem::Math(math) => self.paint_math_item(math),
            LaidItem::Rule {
                width,
                thickness,
                leading,
                gap_after,
            } => self.paint_rule_item(*width, *thickness, *leading, *gap_after),
        }
    }

    fn paint_table_item(&mut self, table: &LaidTable) -> bool {
        let table_h = table.rows.iter().map(|r| r.height).sum::<f32>();
        if *self.y - table_h < self.bottom_limit {
            return false;
        }
        paint_table(
            self.content,
            self.links,
            table,
            self.metrics.margin,
            *self.y,
            self.fonts,
            &mut self.last_fill,
        );
        *self.y -= table_h + table.gap_after;
        true
    }

    fn paint_columns_item(&mut self, cols: &LaidColumns) -> bool {
        let h = cols.height() - cols.gap_after;
        if *self.y - h < self.bottom_limit {
            return false;
        }
        paint_columns(
            self.content,
            self.links,
            cols,
            self.metrics.margin,
            *self.y,
            self.fonts,
            &mut self.last_fill,
        );
        *self.y -= cols.height();
        true
    }

    fn paint_math_item(&mut self, math: &LaidMath) -> bool {
        if *self.y - math.height < self.bottom_limit {
            return false;
        }
        paint_math(
            self.content,
            math,
            self.metrics.margin,
            *self.y,
            self.metrics.content_width(),
            self.fonts,
            &self.knobs.page.chrome,
        );
        *self.y -= math.height + math.gap_after;
        true
    }

    fn paint_rule_item(
        &mut self,
        width: f32,
        thickness: f32,
        leading: f32,
        gap_after: f32,
    ) -> bool {
        if *self.y - leading < self.bottom_limit {
            return false;
        }
        paint_layout_rule(
            self.content,
            self.metrics.margin,
            *self.y,
            width,
            thickness,
            leading,
            self.knobs.page.chrome.stroke_gray,
        );
        *self.y -= leading + gap_after;
        true
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_text_item(
    content: &mut Content,
    links: &mut Vec<PageLink>,
    line: &LaidLine,
    y: &mut f32,
    bottom_limit: f32,
    metrics: &ProfileMetrics,
    fonts: &FontBag,
    last_fill: &mut Option<[f32; 3]>,
) -> bool {
    *y -= line.leading;
    if *y < bottom_limit {
        return false;
    }
    if line.is_gap() {
        return true;
    }
    let origin_x = metrics.margin + line.indent;
    let measure = line.measure.max(line.width());
    paint_aligned_line(
        content, links, line, origin_x, *y, measure, fonts, last_fill,
    );
    true
}

/// Paint one laid line at `origin_x` / `baseline_y` honoring [`LaidLine::text_align`].
#[allow(clippy::too_many_arguments)]
fn paint_aligned_line(
    content: &mut Content,
    links: &mut Vec<PageLink>,
    line: &LaidLine,
    origin_x: f32,
    baseline_y: f32,
    measure: f32,
    fonts: &FontBag,
    last_fill: &mut Option<[f32; 3]>,
) {
    match line.text_align {
        TextAlign::Justify => {
            paint_justified_spans(
                content,
                fonts,
                &line.spans,
                origin_x,
                baseline_y,
                measure.max(line.width()),
                last_fill,
            );
            // Justify redistributes glyph advances; use natural boxes
            // as a best-effort hit target (resume links are short).
            push_span_links(links, &line.spans, origin_x, baseline_y);
        }
        align => {
            let x = origin_x + align.offset_x(measure.max(line.width()), line.width());
            paint_laid_spans(content, fonts, &line.spans, x, baseline_y, last_fill);
            push_span_links(links, &line.spans, x, baseline_y);
        }
    }
}

struct ImagePaint {
    img_idx: usize,
    width: f32,
    height: f32,
    gap_after: f32,
    align: FigureAlign,
}

fn paint_image_item(
    content: &mut Content,
    image: &ImagePaint,
    y: &mut f32,
    bottom_limit: f32,
    metrics: &ProfileMetrics,
) -> bool {
    *y -= image.height;
    if *y < bottom_limit {
        return false;
    }
    let name = image_resource_name(image.img_idx);
    let x = metrics.margin + image.align.offset_x(metrics.content_width(), image.width);
    content.save_state();
    content.transform([image.width, 0.0, 0.0, image.height, x, *y]);
    content.x_object(Name(&name));
    content.restore_state();
    *y -= image.gap_after;
    true
}

fn paint_layout_rule(
    content: &mut Content,
    origin_x: f32,
    top_y: f32,
    width: f32,
    thickness: f32,
    leading: f32,
    stroke_gray: f32,
) {
    let mid_y = top_y - leading / 2.0;
    content.save_state();
    content.set_stroke_gray(stroke_gray);
    content.set_line_width(thickness);
    content.move_to(origin_x, mid_y);
    content.line_to(origin_x + width, mid_y);
    content.stroke();
    content.restore_state();
}

struct PaintChromeBand<'a, B: PageChromeBand + ?Sized> {
    content: &'a mut Content,
    metrics: &'a ProfileMetrics,
    page_no: usize,
    page_count: usize,
    title: &'a str,
    heading: &'a str,
    fonts: &'a FontBag,
    subsets: &'a SubsetMap,
    band: &'a B,
    baseline_y: f32,
}

fn paint_page_chrome_band<B: PageChromeBand + ?Sized>(
    args: PaintChromeBand<'_, B>,
) -> Result<(), WeaveError> {
    let PaintChromeBand {
        content,
        metrics,
        page_no,
        page_count,
        title,
        heading,
        fonts,
        subsets,
        band,
        baseline_y,
    } = args;
    if !band.enabled() {
        return Ok(());
    }
    let text =
        crate::knobs::expand_chrome_format(band.format(), page_no, page_count, title, heading);
    if text.is_empty() {
        return Ok(());
    }
    let face = FaceRef::Bundled(FaceId::SansRegular);
    let mut glyphs = shape_text(fonts, face, &text, band.font_size())?;
    if let Some(subset) = subsets.get(&face) {
        for g in &mut glyphs {
            *g = subset.remap_glyph(*g);
        }
    }
    let text_w = shaped_width(&glyphs);
    let measure = metrics.content_width();
    let x = metrics.margin + band.align().offset_x(measure, text_w);
    let name = fonts.resource_name(face);
    content.begin_text();
    content.set_font(Name(&name), band.font_size());
    content.set_text_matrix([1.0, 0.0, 0.0, 1.0, x, baseline_y]);
    content.show(Str(&encode_gids(&glyphs)));
    content.end_text();
    Ok(())
}

/// Paint side-by-side columns; `top_y` is the top edge in PDF space.
pub(super) fn paint_columns(
    content: &mut Content,
    links: &mut Vec<PageLink>,
    cols: &LaidColumns,
    origin_x: f32,
    top_y: f32,
    fonts: &FontBag,
    last_fill: &mut Option<[f32; 3]>,
) {
    let mut x = origin_x + cols.indent;
    for (i, lines) in cols.columns.iter().enumerate() {
        let col_w = cols.col_widths.get(i).copied().unwrap_or(0.0);
        let mut text_y = top_y;
        for line in lines {
            text_y -= line.leading;
            if line.is_gap() {
                continue;
            }
            let measure = if line.measure > 0.0 {
                line.measure
            } else {
                col_w
            };
            paint_aligned_line(
                content,
                links,
                line,
                x + line.indent,
                text_y,
                measure,
                fonts,
                last_fill,
            );
        }
        x += col_w + cols.gap;
    }
}

/// Stroke the table grid and draw cell text; `top_y` is the top edge in PDF space.
pub(super) fn paint_table(
    content: &mut Content,
    links: &mut Vec<PageLink>,
    table: &LaidTable,
    origin_x: f32,
    top_y: f32,
    fonts: &FontBag,
    last_fill: &mut Option<[f32; 3]>,
) {
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
                paint_laid_spans(
                    content,
                    fonts,
                    &line.spans,
                    cell_x + table.pad,
                    text_y,
                    last_fill,
                );
                push_span_links(links, &line.spans, cell_x + table.pad, text_y);
            }
            cell_x += col_w;
        }
        row_top -= row.height;
    }
}
