//! Prose / block spacing knobs (`defaults/prose.toml`).

use serde::{Deserialize, Serialize};

use super::CaptionBand;
use super::CaptionOverflow;
use super::FigureAlign;
use super::FigureTextAlign;
use super::FigureTitleAlign;
use super::HexColor;
use super::TextAlign;

/// Prose / block spacing knobs (`defaults/prose.toml`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProseKnobs {
    /// Paragraph gaps.
    pub paragraph: ProseParagraphKnobs,
    /// Heading leading / trailing gap.
    pub heading: ProseHeadingKnobs,
    /// Quote indent / body italic / optional color.
    pub quote: ProseQuoteKnobs,
    /// Code block leading / gap.
    pub code: ProseCodeKnobs,
    /// List indent / leading.
    pub list: ProseListKnobs,
    /// Chunk band indent (`indent` level × step → points).
    #[serde(default, skip_serializing_if = "ProseIndentKnobs::is_default")]
    pub indent: ProseIndentKnobs,
    /// Figure trailing gaps.
    pub figure: ProseFigureKnobs,
    /// Figure caption size / italic / optional color.
    pub caption: ProseCaptionKnobs,
    /// Wrap helpers.
    pub wrap: ProseWrapKnobs,
    /// Defaults for [`crate::PrintBlock::Columns`] gap (THI-391).
    ///
    /// Distinct from deck `[columns]` (slide two-column). Pack overlay:
    /// `[body_columns]` or `[prose.body_columns]`.
    #[serde(default, skip_serializing_if = "ProseBodyColumnsKnobs::is_default")]
    pub body_columns: ProseBodyColumnsKnobs,
    /// Body-column review chrome (THI-415). Pack overlay: `[body]`.
    #[serde(default, skip_serializing_if = "ProseBodyKnobs::is_default")]
    pub body: ProseBodyKnobs,
    /// Titled-band opticals for [`crate::PrintBlock::Callout`] (THI-412 / 414).
    #[serde(default, skip_serializing_if = "ProseCalloutKnobs::is_default")]
    pub callout: ProseCalloutKnobs,
    /// Default body text color (optional; omit for engine black).
    #[serde(default, skip_serializing_if = "ProseTextKnobs::is_empty")]
    pub text: ProseTextKnobs,
    /// Citation marker paint policy.
    #[serde(default, skip_serializing_if = "ProseCiteKnobs::is_default")]
    pub cite: ProseCiteKnobs,
    /// Hyperlink paint policy (`TextRun.link_uri`).
    #[serde(default, skip_serializing_if = "ProseLinkKnobs::is_default")]
    pub link: ProseLinkKnobs,
}

/// `[text]` in `prose.toml` — optional body fill color / default font pin.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ProseTextKnobs {
    /// Optional `#RGB` / `#RRGGBB` body color; omit for engine black.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<HexColor>,
    /// Optional pin id into `EmitOptions.pinned_faces`; omit for Liberation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font: Option<String>,
}

impl ProseTextKnobs {
    fn is_empty(&self) -> bool {
        self.color.is_none() && self.font.is_none()
    }
}

/// `[cite]` in `prose.toml` — citation marker color / underline / font pin.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ProseCiteKnobs {
    /// Optional `#RGB` / `#RRGGBB`; else inherits category / text / black.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<HexColor>,
    /// Underline cite runs when true (default false).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub underline: bool,
    /// Optional pin id into `EmitOptions.pinned_faces`; omit for Liberation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font: Option<String>,
}

impl ProseCiteKnobs {
    fn is_default(&self) -> bool {
        self.color.is_none() && !self.underline && self.font.is_none()
    }
}

/// `[link]` in `prose.toml` — outbound URI run policy (`TextRun.link_uri`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ProseLinkKnobs {
    /// Auto-underline text links (LaTeX hyperref-style). Default **false** —
    /// underline only when `InlineStyle.underline` is set. Icon-only link runs
    /// never auto-underline.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub underline: bool,
}

impl ProseLinkKnobs {
    fn is_default(&self) -> bool {
        !self.underline
    }
}

/// `[paragraph]` in `prose.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProseParagraphKnobs {
    /// Gap after a body paragraph (points).
    pub gap_after: f32,
    /// Body paragraph in-band text alignment (bundled `left`).
    #[serde(default = "default_paragraph_text_align")]
    pub text_align: TextAlign,
}

fn default_paragraph_text_align() -> TextAlign {
    TextAlign::Left
}

/// `[heading]` in `prose.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProseHeadingKnobs {
    /// Heading leading as a multiple of heading font size.
    pub leading_factor: f32,
    /// Gap after a heading block (points).
    pub gap_after: f32,
    /// Optional pin id into `EmitOptions.pinned_faces`; omit for Liberation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font: Option<String>,
}

/// `[quote]` in `prose.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProseQuoteKnobs {
    /// Quote left indent (points).
    pub indent: f32,
    /// Italicize quote body runs (decorative marks stay emphasized either way).
    pub italic: bool,
    /// Optional quote fill; inherits `[text].color` then engine black.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<HexColor>,
    /// Optional pin id into `EmitOptions.pinned_faces`; omit for Liberation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font: Option<String>,
}

/// `[code]` in `prose.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProseCodeKnobs {
    /// Code-block line leading as a multiple of code size.
    pub leading_factor: f32,
    /// Gap after a code block (points).
    pub gap_after: f32,
}

/// `[list]` in `prose.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProseListKnobs {
    /// List indent added per nesting depth (points).
    pub indent_per_depth: f32,
    /// List item leading as a multiple of body size.
    pub item_leading_factor: f32,
    /// Extra right-side gutter so list text clears a date/meta column (points).
    ///
    /// Bundled default `0.0`; resume densify sets ~`90` (1.25 in).
    #[serde(default)]
    pub end_gutter: f32,
}

/// `[indent]` in `prose.toml` — sealed chunk band levels → points.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProseIndentKnobs {
    /// Points per sealed `indent` level (0 = no band shift).
    ///
    /// Resume densify sets `14` (≈ LaTeX `\parindent`).
    #[serde(default)]
    pub step: f32,
}

impl Default for ProseIndentKnobs {
    fn default() -> Self {
        Self { step: 0.0 }
    }
}

impl ProseIndentKnobs {
    fn is_default(&self) -> bool {
        self.step == 0.0
    }

    /// Convert a sealed indent level to points.
    #[must_use]
    pub fn pts(&self, level: u32) -> f32 {
        #[allow(clippy::cast_precision_loss)]
        {
            self.step * level as f32
        }
    }
}

/// `[figure]` in `prose.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProseFigureKnobs {
    /// Gap after a figure with no caption (points).
    pub gap_after: f32,
    /// Gap after a figure alt-text placeholder (points).
    pub alt_gap_after: f32,
    /// Gap between the image bottom and the next laid item (usually the caption).
    pub gap_after_image: f32,
    /// Gap before the image: replaces the prior block's trailing gap when present.
    pub gap_before: f32,
    /// Gap between figure title bottom and the image (points); unused when title is empty.
    pub gap_after_title: f32,
    /// Horizontal alignment of the image + caption band.
    #[serde(default)]
    pub align: FigureAlign,
    /// Cap figure display width as a factor of content width (`(0, 1]`; bundled `1.0`).
    pub max_width_factor: f32,
    /// Title band alignment (`follow` = same as [`Self::align`]).
    #[serde(default)]
    pub title_align: FigureTitleAlign,
    /// Title text alignment within the title band (bundled `center`).
    #[serde(default = "default_title_text_align")]
    pub title_text_align: FigureTextAlign,
}

fn default_title_text_align() -> FigureTextAlign {
    FigureTextAlign::Center
}

impl ProseFigureKnobs {
    /// Content-relative max width for `fit_width`, clamped to `(0, 1]`.
    #[must_use]
    pub fn max_display_width(&self, content_w: f32) -> f32 {
        let factor = self.max_width_factor.clamp(1e-3, 1.0);
        content_w * factor
    }
}

/// `[caption]` in `prose.toml` — figure caption size / italic / optional color.
///
/// Applied to [`crate::ir::PrintBlock::Figure`] caption runs only (v1). Non-figure
/// Tessera caption paragraphs stay body/`Paragraph` until a Caption IR lands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProseCaptionKnobs {
    /// Italicize caption runs (default true to match Tessera stand-in).
    pub italic: bool,
    /// Caption size as a factor of profile body size (not absolute points).
    pub size_factor: f32,
    /// Caption line leading as a factor of caption font size.
    pub leading_factor: f32,
    /// Gap after a figure caption (points).
    pub gap_after: f32,
    /// Whether caption shares the image horizontal band.
    #[serde(default)]
    pub band: CaptionBand,
    /// Caption text alignment within the caption measure (bundled `left`;
    /// band still follows the figure via [`Self::band`]).
    #[serde(default = "default_caption_text_align")]
    pub text_align: FigureTextAlign,
    /// Overlong-token policy inside the caption wrap measure.
    #[serde(default)]
    pub overflow: CaptionOverflow,
    /// Optional caption fill; inherits `[text].color` then engine black.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<HexColor>,
    /// Optional pin id into `EmitOptions.pinned_faces`; omit for Liberation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font: Option<String>,
}

fn default_caption_text_align() -> FigureTextAlign {
    FigureTextAlign::Left
}

/// `[wrap]` in `prose.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProseWrapKnobs {
    /// Generic body run leading factor.
    pub body_leading_factor: f32,
    /// Minimum wrap width (points).
    pub min_width: f32,
    /// Soft-hyphenate ASCII letter words when they do not fit (THI-394).
    #[serde(default = "default_hyphenate")]
    pub hyphenate: bool,
    /// Min content lines kept together at the start of a paragraph (CSS orphans).
    #[serde(default = "default_orphan_lines")]
    pub orphan_lines: u32,
    /// Min content lines kept together at the end of a paragraph (CSS widows).
    #[serde(default = "default_widow_lines")]
    pub widow_lines: u32,
}

fn default_hyphenate() -> bool {
    true
}

fn default_orphan_lines() -> u32 {
    2
}

fn default_widow_lines() -> u32 {
    2
}

/// `[body_columns]` in `prose.toml` — newspaper/article column flow defaults (THI-391).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProseBodyColumnsKnobs {
    /// Gap between columns when [`crate::PrintBlock::Columns::gap`] is omitted (points).
    #[serde(default = "default_body_columns_gap")]
    pub gap: f32,
}

fn default_body_columns_gap() -> f32 {
    18.0
}

impl Default for ProseBodyColumnsKnobs {
    fn default() -> Self {
        Self {
            gap: default_body_columns_gap(),
        }
    }
}

impl ProseBodyColumnsKnobs {
    fn is_default(&self) -> bool {
        (self.gap - default_body_columns_gap()).abs() < f32::EPSILON
    }
}

/// `[body]` in `prose.toml` — review line-number gutter (THI-415).
///
/// TOML keeps the `line_` prefix (`line_numbers`, `line_number_gutter`,
/// `line_number_size_factor`). Rust fields drop it so the struct does not
/// share one prefix across every field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProseBodyKnobs {
    /// Number laid-out body lines in a per-column gutter (bundled off).
    #[serde(default, rename = "line_numbers")]
    pub numbers: bool,
    /// Gutter width reserved at the start of each column (points).
    #[serde(default = "default_line_number_gutter", rename = "line_number_gutter")]
    pub gutter_width: f32,
    /// Line-number size as a factor of profile body size.
    #[serde(
        default = "default_line_number_size_factor",
        rename = "line_number_size_factor"
    )]
    pub size_factor: f32,
}

fn default_line_number_gutter() -> f32 {
    18.0
}

fn default_line_number_size_factor() -> f32 {
    0.7
}

impl Default for ProseBodyKnobs {
    fn default() -> Self {
        Self {
            numbers: false,
            gutter_width: default_line_number_gutter(),
            size_factor: default_line_number_size_factor(),
        }
    }
}

impl ProseBodyKnobs {
    fn is_default(&self) -> bool {
        !self.numbers
            && (self.gutter_width - default_line_number_gutter()).abs() < f32::EPSILON
            && (self.size_factor - default_line_number_size_factor()).abs() < f32::EPSILON
    }

    /// Gutter inset when numbering is on; `0` when off.
    #[must_use]
    pub fn gutter(&self) -> f32 {
        if self.numbers {
            self.gutter_width.max(0.0)
        } else {
            0.0
        }
    }
}

/// `[callout]` in `prose.toml` — titled-band paint (THI-412 / THI-414).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProseCalloutKnobs {
    /// Extra indent of the band from the content left (points).
    #[serde(default)]
    pub indent: f32,
    /// Left-rule thickness (points).
    #[serde(default = "default_callout_rule_thickness")]
    pub rule_thickness: f32,
    /// Gap between the rule and title/body (points).
    #[serde(default = "default_callout_rule_gap")]
    pub rule_gap: f32,
    /// Gap between title line and body (points).
    #[serde(default = "default_callout_title_gap")]
    pub title_gap: f32,
}

fn default_callout_rule_thickness() -> f32 {
    1.5
}

fn default_callout_rule_gap() -> f32 {
    8.0
}

fn default_callout_title_gap() -> f32 {
    4.0
}

impl Default for ProseCalloutKnobs {
    fn default() -> Self {
        Self {
            indent: 0.0,
            rule_thickness: default_callout_rule_thickness(),
            rule_gap: default_callout_rule_gap(),
            title_gap: default_callout_title_gap(),
        }
    }
}

impl ProseCalloutKnobs {
    fn is_default(&self) -> bool {
        self.indent.abs() < f32::EPSILON
            && (self.rule_thickness - default_callout_rule_thickness()).abs() < f32::EPSILON
            && (self.rule_gap - default_callout_rule_gap()).abs() < f32::EPSILON
            && (self.title_gap - default_callout_title_gap()).abs() < f32::EPSILON
    }
}

/// Mutually exclusive prose fill categories (`[text]` / `[quote]` / `[caption]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProsePaintCategory {
    /// Body / heading / list — `[text].color`.
    Text,
    /// Quote block — `[quote].color` else `[text].color`.
    Quote,
    /// Figure caption — `[caption].color` else `[text].color`.
    Caption,
}

/// Mutually exclusive category font pin slots (cite overlays separately).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProseFontCategory {
    /// `[text].font` for body / list runs.
    Text,
    /// `[heading].font` for heading runs.
    Heading,
    /// `[quote].font` for quote body runs.
    Quote,
    /// `[caption].font` for figure caption runs.
    Caption,
}

impl From<ProsePaintCategory> for ProseFontCategory {
    fn from(paint: ProsePaintCategory) -> Self {
        match paint {
            ProsePaintCategory::Text => Self::Text,
            ProsePaintCategory::Quote => Self::Quote,
            ProsePaintCategory::Caption => Self::Caption,
        }
    }
}

impl ProseKnobs {
    /// Category fill for body/heading/list: `[text].color` else engine black.
    #[must_use]
    pub fn text_fill_rgb01(&self) -> [f32; 3] {
        color_or_black(self.text.color)
    }

    /// Category fill for quotes: `[quote].color` else `[text].color` else black.
    #[must_use]
    pub fn quote_fill_rgb01(&self) -> [f32; 3] {
        self.fill_or_text(self.quote.color)
    }

    /// Category fill for figure captions: `[caption].color` else `[text].color` else black.
    #[must_use]
    pub fn caption_fill_rgb01(&self) -> [f32; 3] {
        self.fill_or_text(self.caption.color)
    }

    /// Fill for a mutually exclusive paint category.
    #[must_use]
    pub fn category_fill_rgb01(&self, category: ProsePaintCategory) -> [f32; 3] {
        match category {
            ProsePaintCategory::Text => self.text_fill_rgb01(),
            ProsePaintCategory::Quote => self.quote_fill_rgb01(),
            ProsePaintCategory::Caption => self.caption_fill_rgb01(),
        }
    }

    /// Per-run fill: cite color when set, else `category_fill`.
    #[must_use]
    pub fn run_fill_rgb01(&self, cite: bool, category_fill: [f32; 3]) -> [f32; 3] {
        if cite && let Some(c) = self.cite.color {
            return c.to_rgb01();
        }
        category_fill
    }

    /// Resolve fill + underline for a run under a paint category.
    ///
    /// `cite` and `style_underline` are orthogonal run flags (not paint categories).
    /// Underline when `style_underline` **or** (`cite` and `[cite].underline`).
    #[must_use]
    pub fn run_paint_rgb01(
        &self,
        cite: bool,
        category: ProsePaintCategory,
        style_underline: bool,
    ) -> ([f32; 3], bool) {
        (
            self.run_fill_rgb01(cite, self.category_fill_rgb01(category)),
            style_underline || (cite && self.cite.underline),
        )
    }

    /// Optional category default pin id when `TextRun.face` is unset.
    ///
    /// Precedence: cite (orthogonal run flag) → `category` pin. No pin-id inherit
    /// between categories; omit means Liberation via style mapping.
    #[must_use]
    pub fn category_font_pin(&self, cite: bool, category: ProseFontCategory) -> Option<&str> {
        if cite {
            return self.cite.font.as_deref();
        }
        match category {
            ProseFontCategory::Heading => self.heading.font.as_deref(),
            ProseFontCategory::Quote => self.quote.font.as_deref(),
            ProseFontCategory::Caption => self.caption.font.as_deref(),
            ProseFontCategory::Text => self.text.font.as_deref(),
        }
    }

    fn fill_or_text(&self, color: Option<HexColor>) -> [f32; 3] {
        color_or_black(color.or(self.text.color))
    }
}

fn color_or_black(color: Option<HexColor>) -> [f32; 3] {
    color.map_or([0.0, 0.0, 0.0], HexColor::to_rgb01)
}
