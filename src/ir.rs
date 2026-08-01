//! Print IR types (prose MVP surface).
//!
//! Normative sketch: Tessera `docs/print_ir.md` / D21. Serde names should stay
//! stable once published; rename modules freely.

use serde::{Deserialize, Serialize};

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
/// Examples: `print@0` (MVP stub), later `print@1`, `manuscript@1`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrintProfileId {
    /// Profile family name (`print`, `manuscript`, `deck`, …).
    pub name: String,
    /// Version bumped when pagination rules change.
    pub version: u32,
}

impl PrintProfileId {
    /// Construct `print@0` (A4 + Liberation Sans body).
    #[must_use]
    pub fn print_v0() -> Self {
        Self {
            name: "print".into(),
            version: 0,
        }
    }

    /// Construct `manuscript@0` (US Letter, double-space, Liberation Serif body).
    #[must_use]
    pub fn manuscript_v0() -> Self {
        Self {
            name: "manuscript".into(),
            version: 0,
        }
    }

    /// Construct `deck@0` (16:9 landscape, large type for slides).
    #[must_use]
    pub fn deck_v0() -> Self {
        Self {
            name: "deck".into(),
            version: 0,
        }
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
    },
    /// Body paragraph.
    Paragraph {
        /// Inline runs.
        runs: Vec<TextRun>,
    },
    /// Ordered or bullet list (items already coalesced from Tessera chunks).
    List {
        /// `true` for numbered lists.
        ordered: bool,
        /// Top-level items (nested lists via [`ListItem::children`]).
        items: Vec<ListItem>,
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
    },
    /// Structured table (drawn grid + wrapped cell text).
    Table {
        /// Row-major cells.
        rows: Vec<TableRow>,
    },
    /// Figure with image bytes + caption.
    Figure {
        /// Embedded image.
        image: PrintImage,
        /// Alt text.
        alt: String,
        /// Caption runs.
        caption: Vec<TextRun>,
        /// Placement hint.
        placement: FigurePlacement,
    },
    /// Math (LaTeX source; emit prettifies common tokens, not full TeX).
    Math {
        /// Display vs inline.
        display: bool,
        /// LaTeX source.
        latex: String,
    },
    /// Deck slide (one page; regions laid out by slot name).
    Slide {
        /// Layout template id.
        layout_id: String,
        /// Region payloads (`title`, `body`, …).
        regions: Vec<SlideRegionContent>,
    },
    /// Explicit author/export break (e.g. chapter boundary).
    Break(BreakHint),
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
}

impl TextRun {
    /// Plain (unstyled) run.
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: InlineStyle::default(),
            face: None,
        }
    }

    /// Plain run that uses a pinned face id from [`crate::EmitOptions`].
    pub fn pinned(text: impl Into<String>, face: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: InlineStyle::default(),
            face: Some(face.into()),
        }
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
    /// Hyperlink (URL carried elsewhere later; flag only for MVP).
    #[serde(default)]
    pub link: bool,
    /// Citation marker.
    #[serde(default)]
    pub cite: bool,
}

/// Page-break / keep hints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BreakHint {
    /// No special break.
    #[default]
    None,
    /// Prefer new page (soft).
    Page,
    /// Always new page (e.g. manuscript H1).
    PageAlways,
    /// Keep with following block (heading + first lines).
    KeepWithNext,
}
