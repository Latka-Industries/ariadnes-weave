//! Print IR types (prose MVP surface).
//!
//! Normative sketch: Tessera `docs/print_ir.md` / D21. Layout chunks: D24
//! (`place` / `vspace` / `rule`). Serde names should stay stable once published;
//! rename modules freely.

use serde::{Deserialize, Serialize};

use crate::knobs::TextAlign;

/// Top-level input to ariadnes-weave.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrintDocument {
    /// Document metadata (title, kind, language).
    pub meta: PrintMeta,
    /// Versioned print profile id (policy, not CSS).
    pub profile: PrintProfileId,
    /// Reading-order blocks.
    pub blocks: Vec<PrintBlock>,
}

/// Lightweight document metadata carried into PDF info / future headers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrintMeta {
    /// Human title.
    pub title: String,
    /// Mirror catalog / superblock kind (opaque string for now).
    pub doc_kind: String,
    /// BCP-47 language tag, if known.
    pub language: Option<String>,
    /// Optional source document id for provenance.
    pub source_doc_id: Option<String>,
}

/// Stable id for a versioned profile (not a CSS file).
///
/// Coupled MVP ids compose mode × page size × margins — see
/// `docs/decisions/D-print-profile-axes.md`. Examples: `print@0`,
/// `print-letter@0`, `manuscript@0`, `deck@0`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrintProfileId {
    /// Profile family name (`print`, `print-letter`, `manuscript`, `deck`, …).
    pub name: String,
    /// Version bumped when pagination rules change.
    pub version: u32,
}

impl PrintProfileId {
    fn v0(name: &str) -> Self {
        Self {
            name: name.into(),
            version: 0,
        }
    }

    /// Construct `print@0` (A4 technical prose + Liberation Sans).
    #[must_use]
    pub fn print_v0() -> Self {
        Self::v0("print")
    }

    /// Construct `print-letter@0` (US Letter + same print mode as `print@0`).
    #[must_use]
    pub fn print_letter_v0() -> Self {
        Self::v0("print-letter")
    }

    /// Construct `manuscript@0` (US Letter, double-space, Liberation Serif body).
    #[must_use]
    pub fn manuscript_v0() -> Self {
        Self::v0("manuscript")
    }

    /// Construct `deck@0` (16:9 landscape, large type for slides).
    #[must_use]
    pub fn deck_v0() -> Self {
        Self::v0("deck")
    }

    /// Construct `resume@0` (US Letter, tight margins, dense sans body).
    #[must_use]
    pub fn resume_v0() -> Self {
        Self::v0("resume")
    }

    /// Display as `name@version`.
    #[must_use]
    pub fn as_label(&self) -> String {
        format!("{}@{}", self.name, self.version)
    }
}

/// A pagination-ready block in reading order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PrintBlock {
    /// Heading at outline level 1–6.
    Heading {
        /// Outline level (1 = top).
        level: u8,
        /// Inline runs.
        runs: Vec<TextRun>,
        /// Break preference before this heading.
        break_before: BreakHint,
        /// Optional internal destination id (TOC / outline `GoTo`; THI-390/393).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dest_id: Option<String>,
    },
    /// Body paragraph.
    Paragraph {
        /// Inline runs.
        runs: Vec<TextRun>,
        /// Band indent level (0 = content margin). Points = `level × indent_step`.
        #[serde(default, skip_serializing_if = "u32_is_zero")]
        indent: u32,
        /// Per-block override of pack `[paragraph].text_align` (THI-398).
        ///
        /// `None` → inherit from enclosing [`Self::Columns`] region, else pack knobs.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text_align: Option<TextAlign>,
    },
    /// Ordered or bullet list (items already coalesced from Tessera chunks).
    List {
        /// `true` for numbered lists.
        ordered: bool,
        /// Top-level items (nested lists via [`ListItem::children`]).
        items: Vec<ListItem>,
        /// Band indent level (0 = content margin). Nesting uses `list_depth` / depth.
        #[serde(default, skip_serializing_if = "u32_is_zero")]
        indent: u32,
        /// Per-block override of pack `[paragraph].text_align` (THI-398).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text_align: Option<TextAlign>,
    },
    /// Fenced / indented code block.
    Code {
        /// Optional fence language tag.
        lang: Option<String>,
        /// Raw code text (no inline styles).
        text: String,
    },
    /// Block quote.
    Quote {
        /// Inline runs.
        runs: Vec<TextRun>,
        /// Per-block override; `None` → inherit / flush start (quotes do not
        /// take pack `[paragraph].text_align` unless set).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text_align: Option<TextAlign>,
    },
    /// Structured table (drawn grid + wrapped cell text).
    Table {
        /// Row-major cells.
        rows: Vec<TableRow>,
        /// Optional internal destination id (list-of-tables `GoTo`; THI-395).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dest_id: Option<String>,
    },
    /// Meta row without a table grid (LaTeX `\hfill` stand-in).
    ///
    /// Pane count is `panes.len()` (≥1). Layout: the **last** pane is
    /// natural-width and flush to the end edge; earlier panes share the
    /// leftover measure equally. Two panes recover classic left/right CV rows.
    Row {
        /// Ordered panes (Tessprek `\row{…}{…}…`).
        panes: Vec<Vec<TextRun>>,
        /// Band indent level (0 = content margin). Points = `level × indent_step`.
        #[serde(default, skip_serializing_if = "u32_is_zero")]
        indent: u32,
    },
    /// Table-of-contents line: title (+ optional section prefix in runs),
    /// optional dotted leaders, optional page digits, optional `GoTo` dest (THI-390).
    ///
    /// When [`Self::TocEntry::page_label`] is `None` and `dest_id` is set, emit
    /// resolves the page number after a layout pass. Page digits are flush-right
    /// in a reserved slot so multi-digit pages share one column edge.
    TocEntry {
        /// Title runs (may include section number prefix).
        title: Vec<TextRun>,
        /// Page digits when known; `None` → resolve from `dest_id` when possible.
        /// `Some("")` omits the page column.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        page_label: Option<String>,
        /// Internal destination matching a heading / figure / table `dest_id`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dest_id: Option<String>,
        /// Band indent level (0 = content margin). Nesting ≈ heading depth − 1.
        #[serde(default, skip_serializing_if = "u32_is_zero")]
        indent: u32,
        /// Fill the gap between title and page with `.` leaders (default true).
        #[serde(default = "default_true", skip_serializing_if = "is_true")]
        leaders: bool,
    },
    /// Figure with image bytes + optional title + caption.
    Figure {
        /// Embedded image.
        image: PrintImage,
        /// Alt text.
        alt: String,
        /// Title runs above the image (empty = none). Prefer this over a prior
        /// `Paragraph`+strong stand-in so title can share the figure band.
        #[serde(default)]
        title: Vec<TextRun>,
        /// Caption runs.
        caption: Vec<TextRun>,
        /// Placement hint.
        placement: FigurePlacement,
        /// Optional internal destination id (list-of-figures `GoTo`; THI-395).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dest_id: Option<String>,
    },
    /// Math (LaTeX source; structured layout for `\frac` / scripts / display
    /// ∑∏ limits / matrices).
    Math {
        /// Display vs inline (`true` → under/over limits on ∑/∏/…; ∫ stays side).
        display: bool,
        /// LaTeX source.
        latex: String,
    },
    /// Deck slide (one page; regions laid out by `layout_id`).
    ///
    /// Supported templates: `title-body` (default), `title-subtitle-body`,
    /// `two-column` / `title-two-column` (title band + `left`/`right` columns).
    Slide {
        /// Layout template id (`title-body`, `two-column`, …).
        layout_id: String,
        /// Region payloads (`title`, `body`, `left`, `right`, …).
        regions: Vec<SlideRegionContent>,
    },
    /// Sealed layout chunk (D24): closed `place` / `vspace` / `rule` ops.
    Layout {
        /// Ordered layout ops in reading order.
        ops: Vec<LayoutOp>,
    },
    /// Continuous multi-column body flow (newspaper/article; THI-391).
    ///
    /// Distinct from [`Self::Row`] (meta hfill panes) and slide `two-column`.
    /// Children flow down column 1, then 2… then the next page. Headings,
    /// figures, tables, math, slides, layout ops, breaks, rows, TOC lines, and
    /// nested columns **span** full measure (flush the current column band).
    Columns {
        /// Column count (clamped to 2..=6 at layout).
        count: u8,
        /// Gap between columns in points; `None` → knobs `[body_columns].gap`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        gap: Option<u16>,
        /// Nested body blocks in reading order.
        children: Vec<PrintBlock>,
        /// Region default for flow children that omit `text_align` (THI-398).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text_align: Option<TextAlign>,
    },
    /// Footnote or endnote definition (THI-410). Not painted in body flow.
    ///
    /// Markers in prose set [`TextRun::note_id`] to this `id`. Footnotes paint
    /// in a reserved band above footer chrome; endnotes dump after the last
    /// body block.
    Note {
        /// Stable id referenced by [`TextRun::note_id`].
        id: String,
        /// Footnote (page band) vs endnote (end dump).
        ///
        /// Named `note_kind` because internally tagged serde already uses `kind`
        /// for the [`PrintBlock`] variant.
        note_kind: NoteKind,
        /// Note body runs.
        runs: Vec<TextRun>,
    },
    /// Explicit author/export break (e.g. chapter boundary).
    Break(BreakHint),
}

/// Footnote vs endnote placement (THI-410).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteKind {
    /// Page-bottom band above footer chrome.
    Footnote,
    /// Dumped after the last body block.
    Endnote,
}

/// One closed layout op inside [`PrintBlock::Layout`] (D24).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LayoutOp {
    /// Horizontal skip, then inline content runs.
    Place {
        /// Skip before content (`frac` of measure or `em`).
        skip: PlaceSkip,
        /// Inline runs drawn after the skip.
        #[serde(default)]
        runs: Vec<TextRun>,
    },
    /// Extra vertical air (no measure-`frac`).
    Vspace {
        /// Named step or em distance.
        amount: VspaceAmount,
    },
    /// Horizontal rule across part of the measure.
    Rule {
        /// Rule width (`frac` and/or `em`, summed).
        width: RuleWidth,
    },
}

// serde `skip_serializing_if` requires `fn(&T) -> bool`.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn u32_is_zero(v: &u32) -> bool {
    *v == 0
}

fn default_true() -> bool {
    true
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_true(v: &bool) -> bool {
    *v
}

impl PrintBlock {
    /// Body paragraph at indent level 0.
    #[must_use]
    pub fn paragraph(runs: Vec<TextRun>) -> Self {
        Self::paragraph_align(runs, 0, None)
    }

    /// Body paragraph at an explicit band indent level.
    #[must_use]
    pub fn paragraph_indent(runs: Vec<TextRun>, indent: u32) -> Self {
        Self::paragraph_align(runs, indent, None)
    }

    /// Body paragraph with optional per-block [`TextAlign`] (THI-398).
    #[must_use]
    pub fn paragraph_align(runs: Vec<TextRun>, indent: u32, text_align: Option<TextAlign>) -> Self {
        Self::Paragraph {
            runs,
            indent,
            text_align,
        }
    }

    /// Block quote (flush start unless `text_align` is set).
    #[must_use]
    pub fn quote(runs: Vec<TextRun>) -> Self {
        Self::Quote {
            runs,
            text_align: None,
        }
    }

    /// Block quote with optional per-block [`TextAlign`] (THI-398).
    #[must_use]
    pub fn quote_align(runs: Vec<TextRun>, text_align: Option<TextAlign>) -> Self {
        Self::Quote { runs, text_align }
    }

    /// Two-pane meta row (classic left / right `\hfill`).
    #[must_use]
    pub fn row_two(left: Vec<TextRun>, right: Vec<TextRun>) -> Self {
        Self::Row {
            panes: vec![left, right],
            indent: 0,
        }
    }

    /// N-pane meta row (`panes.len()` ≥ 1).
    #[must_use]
    pub fn row(panes: Vec<Vec<TextRun>>) -> Self {
        Self::Row { panes, indent: 0 }
    }

    /// N-pane meta row at an explicit band indent level.
    #[must_use]
    pub fn row_indent(panes: Vec<Vec<TextRun>>, indent: u32) -> Self {
        Self::Row { panes, indent }
    }

    /// TOC line with optional page label, leaders, and internal dest.
    #[must_use]
    pub fn toc_entry(
        title: Vec<TextRun>,
        page_label: Option<String>,
        dest_id: Option<String>,
        indent: u32,
    ) -> Self {
        Self::toc_entry_leaders(title, page_label, dest_id, indent, true)
    }

    /// TOC line with explicit leaders on/off.
    #[must_use]
    pub fn toc_entry_leaders(
        title: Vec<TextRun>,
        page_label: Option<String>,
        dest_id: Option<String>,
        indent: u32,
        leaders: bool,
    ) -> Self {
        Self::TocEntry {
            title,
            page_label,
            dest_id,
            indent,
            leaders,
        }
    }

    /// Heading with optional internal destination id.
    #[must_use]
    pub fn heading(level: u8, runs: Vec<TextRun>, break_before: BreakHint) -> Self {
        Self::Heading {
            level,
            runs,
            break_before,
            dest_id: None,
        }
    }

    /// Heading that registers an internal destination for TOC / outline.
    #[must_use]
    pub fn heading_dest(
        level: u8,
        runs: Vec<TextRun>,
        break_before: BreakHint,
        dest_id: impl Into<String>,
    ) -> Self {
        Self::Heading {
            level,
            runs,
            break_before,
            dest_id: Some(dest_id.into()),
        }
    }

    /// Table without an internal destination.
    #[must_use]
    pub fn table(rows: Vec<TableRow>) -> Self {
        Self::Table {
            rows,
            dest_id: None,
        }
    }

    /// Table that registers an internal destination for list-of-tables.
    #[must_use]
    pub fn table_dest(rows: Vec<TableRow>, dest_id: impl Into<String>) -> Self {
        Self::Table {
            rows,
            dest_id: Some(dest_id.into()),
        }
    }

    /// Figure without an internal destination.
    #[must_use]
    pub fn figure(
        image: PrintImage,
        alt: impl Into<String>,
        title: Vec<TextRun>,
        caption: Vec<TextRun>,
        placement: FigurePlacement,
    ) -> Self {
        Self::Figure {
            image,
            alt: alt.into(),
            title,
            caption,
            placement,
            dest_id: None,
        }
    }

    /// Figure that registers an internal destination for list-of-figures.
    #[must_use]
    pub fn figure_dest(
        image: PrintImage,
        alt: impl Into<String>,
        title: Vec<TextRun>,
        caption: Vec<TextRun>,
        placement: FigurePlacement,
        dest_id: impl Into<String>,
    ) -> Self {
        Self::Figure {
            image,
            alt: alt.into(),
            title,
            caption,
            placement,
            dest_id: Some(dest_id.into()),
        }
    }

    /// Multi-column body region (THI-391).
    #[must_use]
    pub fn columns(count: u8, gap: Option<u16>, children: Vec<Self>) -> Self {
        Self::columns_align(count, gap, children, None)
    }

    /// Multi-column body region with a default [`TextAlign`] for flow children
    /// that omit their own `text_align` (THI-398).
    #[must_use]
    pub fn columns_align(
        count: u8,
        gap: Option<u16>,
        children: Vec<Self>,
        text_align: Option<TextAlign>,
    ) -> Self {
        Self::Columns {
            count,
            gap,
            children,
            text_align,
        }
    }

    /// Footnote or endnote definition (not in-flow body).
    #[must_use]
    pub fn note(id: impl Into<String>, kind: NoteKind, runs: Vec<TextRun>) -> Self {
        Self::Note {
            id: id.into(),
            note_kind: kind,
            runs,
        }
    }
}

/// Horizontal skip for [`LayoutOp::Place`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlaceSkip {
    /// Fraction of line measure in reading direction.
    ///
    /// At full measure ([`MeasureFrac::FULL`]), content is flushed to the end
    /// edge using leftover width after measuring the runs (LaTeX-style).
    Frac {
        /// Measure fraction (`0..=10_000` bps).
        frac: MeasureFrac,
    },
    /// Skip in body ems.
    Em {
        /// Em distance.
        em: EmAmount,
    },
}

/// Vertical gap for [`LayoutOp::Vspace`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VspaceAmount {
    /// Small step (~0.5 em).
    Small,
    /// Medium step (~1 em).
    Med,
    /// Large step (~2 em).
    Big,
    /// Explicit em distance.
    Em {
        /// Em distance.
        em: EmAmount,
    },
}

/// Rule width for [`LayoutOp::Rule`] — `frac` and/or `em` (widths add).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleWidth {
    /// Fraction of line measure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frac: Option<MeasureFrac>,
    /// Additional width in body ems.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub em: Option<EmAmount>,
}

impl RuleWidth {
    /// Width from measure fraction only.
    #[must_use]
    pub fn frac(frac: MeasureFrac) -> Self {
        Self {
            frac: Some(frac),
            em: None,
        }
    }

    /// Width from em amount only.
    #[must_use]
    pub fn em(em: EmAmount) -> Self {
        Self {
            frac: None,
            em: Some(em),
        }
    }
}

/// Fraction of line measure stored as basis points (`10_000` = 1.0).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeasureFrac {
    /// Ten-thousandths of full measure (`10_000` = flush / full width).
    pub bps: u16,
}

impl MeasureFrac {
    /// Full measure (`frac = 1`); place flush uses leftover after content width.
    pub const FULL: Self = Self { bps: 10_000 };

    /// Half measure (`frac = 0.5`).
    pub const HALF: Self = Self { bps: 5_000 };

    /// Construct from basis points; caller / layout validates `≤ 10_000`.
    #[must_use]
    pub const fn from_bps(bps: u16) -> Self {
        Self { bps }
    }

    /// Convert `0.0..=1.0` to nearest bps.
    ///
    /// # Errors
    ///
    /// Returns [`crate::WeaveError::InvalidLayoutFrac`] when outside range.
    pub fn try_from_f32(frac: f32) -> Result<Self, crate::WeaveError> {
        if !(0.0..=1.0).contains(&frac) || !frac.is_finite() {
            let bps = if frac.is_finite() && frac > 1.0 {
                ((frac * 10_000.0).round() as i64).clamp(0, u16::MAX as i64) as u16
            } else {
                u16::MAX
            };
            return Err(crate::WeaveError::InvalidLayoutFrac(bps));
        }
        Ok(Self {
            bps: (frac * 10_000.0).round() as u16,
        })
    }

    /// `bps` as a `0.0..=1.0` factor (no validation).
    #[must_use]
    pub fn as_f32(self) -> f32 {
        f32::from(self.bps) / 10_000.0
    }
}

/// Distance in thousandths of an em (`1000` = 1em).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmAmount {
    /// Thousandths of an em.
    pub milli: i32,
}

impl EmAmount {
    /// One em.
    pub const ONE: Self = Self { milli: 1000 };

    /// Construct from thousandths of an em.
    #[must_use]
    pub const fn from_milli(milli: i32) -> Self {
        Self { milli }
    }

    /// Construct from an em multiple (rounded to milli-ems).
    #[must_use]
    pub fn from_em(em: f32) -> Self {
        Self {
            milli: (em * 1000.0).round() as i32,
        }
    }

    /// Convert to points given the body em size.
    #[must_use]
    pub fn to_points(self, em_size: f32) -> f32 {
        em_size * (self.milli as f32) / 1000.0
    }
}

/// One table row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableRow {
    /// Ordered cells (plain text for now).
    pub cells: Vec<String>,
}

/// Embedded image for [`PrintBlock::Figure`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrintImage {
    /// Raw image bytes.
    pub bytes: Vec<u8>,
    /// MIME type (`image/png`, `image/jpeg`, …).
    pub media_type: String,
    /// Optional pixel width.
    pub width_px: Option<u32>,
    /// Optional pixel height.
    pub height_px: Option<u32>,
}

/// Figure placement policy (no freeform x/y).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FigurePlacement {
    /// In normal reading flow.
    #[default]
    Flow,
    /// Prefer near the mentioning text.
    FloatNear,
}

/// Named region payload on a slide page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlideRegionContent {
    /// Region slot name (`title`, `subtitle`, `body`, …).
    pub slot: String,
    /// Plain text for the slot.
    pub text: String,
}

/// One list item, optionally with nested child lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListItem {
    /// Item body runs.
    pub runs: Vec<TextRun>,
    /// Nested lists under this item.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<PrintBlock>,
}

/// Styled inline text run (no free CSS).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextRun {
    /// Run text (Unicode).
    pub text: String,
    /// Inline style flags; MVP emit may ignore most of these.
    pub style: InlineStyle,
    /// Optional pin id matching [`crate::EmitOptions::pinned_faces`].
    ///
    /// When set, emit uses that host-supplied TTF instead of Liberation style
    /// mapping. Unknown ids fail emit with [`crate::WeaveError::Font`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub face: Option<String>,
    /// External URI for clickable PDF link annotations (`http` / `https` / `mailto`).
    ///
    /// When set, emit paints with the cite/link color and writes a `/Link`
    /// annotation over the run's ink box. Prefer setting [`InlineStyle::link`]
    /// as well for hosts that only inspect style flags.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link_uri: Option<String>,
    /// Id of a [`PrintBlock::Note`] this run cites (superscript marker).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note_id: Option<String>,
}

impl TextRun {
    /// Plain (unstyled) run.
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: InlineStyle::default(),
            face: None,
            link_uri: None,
            note_id: None,
        }
    }

    /// Strong / bold run (figure titles, emphasis without other flags).
    pub fn strong(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: InlineStyle {
                strong: true,
                ..InlineStyle::default()
            },
            face: None,
            link_uri: None,
            note_id: None,
        }
    }

    /// Plain run that uses a pinned face id from [`crate::EmitOptions`].
    pub fn pinned(text: impl Into<String>, face: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: InlineStyle::default(),
            face: Some(face.into()),
            link_uri: None,
            note_id: None,
        }
    }

    /// Linked run (URI + `style.link`).
    pub fn link(text: impl Into<String>, uri: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: InlineStyle {
                link: true,
                ..InlineStyle::default()
            },
            face: None,
            link_uri: Some(uri.into()),
            note_id: None,
        }
    }

    /// Attach a footnote/endnote id (superscript at layout).
    #[must_use]
    pub fn with_note(mut self, id: impl Into<String>) -> Self {
        self.note_id = Some(id.into());
        self
    }
}

/// Inline style flags — no free CSS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct InlineStyle {
    /// Bold / strong.
    #[serde(default)]
    pub strong: bool,
    /// Italic / emphasis.
    #[serde(default)]
    pub emphasis: bool,
    /// Inline code.
    #[serde(default)]
    pub code: bool,
    /// Hyperlink paint hint (URI lives on [`TextRun::link_uri`]).
    #[serde(default)]
    pub link: bool,
    /// Citation marker.
    #[serde(default)]
    pub cite: bool,
    /// Underline this run (also painted when `cite` and `[cite].underline`).
    #[serde(default)]
    pub underline: bool,
}

/// Page-break / keep hints.
///
/// `manuscript@0` also forces a new page before each H1 after the first content
/// on a page (`force_h1_page_break`) without requiring [`BreakHint::PageAlways`].
/// See `docs/profiles.md` and `docs/decisions/D-literary-unfolding.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BreakHint {
    /// No special break.
    #[default]
    None,
    /// Prefer new page. Today treated like [`BreakHint::PageAlways`].
    Page,
    /// Always new page when the current page already has content.
    PageAlways,
    /// Keep with following block (also applied automatically for H1/H2).
    KeepWithNext,
}

impl BreakHint {
    /// True when emit should start a new page if the current page already has content.
    #[must_use]
    pub fn forces_page_break(self) -> bool {
        matches!(self, BreakHint::Page | BreakHint::PageAlways)
    }
}
