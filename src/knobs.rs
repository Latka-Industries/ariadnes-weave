//! Named layout optical defaults (per-category TOML under `defaults/`).
//!
//! Profiles own page size / margins / body size ([`crate::profile`]). These knobs
//! are the hard-coded paddings, gaps, and scale factors inside emit.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// Full layout knob set (one TOML file per category).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutKnobs {
    /// Paragraph / heading / list / quote / code spacing.
    pub prose: ProseKnobs,
    /// Table cell padding and trailing gap.
    pub table: TableKnobs,
    /// Slide / deck region scales and column gaps.
    pub deck: DeckKnobs,
    /// Structured math optical constants.
    pub math: MathKnobs,
    /// Page chrome (footer, clearance, stroke gray).
    pub page: PageKnobs,
}

impl Default for LayoutKnobs {
    fn default() -> Self {
        Self::bundled()
    }
}

impl LayoutKnobs {
    /// Parse the crate-bundled `defaults/*.toml` files (cached).
    #[must_use]
    pub fn bundled() -> Self {
        static BUNDLED: OnceLock<LayoutKnobs> = OnceLock::new();
        BUNDLED.get_or_init(load_bundled).clone()
    }

    /// Flat `category.section.key = value` lines for agent / human dumps.
    #[must_use]
    pub fn describe(&self) -> String {
        let mut lines = Vec::new();
        push_category(&mut lines, "prose", &self.prose);
        push_category(&mut lines, "table", &self.table);
        push_category(&mut lines, "deck", &self.deck);
        push_category(&mut lines, "math", &self.math);
        push_category(&mut lines, "page", &self.page);
        lines.join("\n")
    }
}

fn load_bundled() -> LayoutKnobs {
    LayoutKnobs {
        prose: parse_category("prose", include_str!("../defaults/prose.toml")),
        table: parse_category("table", include_str!("../defaults/table.toml")),
        deck: parse_category("deck", include_str!("../defaults/deck.toml")),
        math: parse_category("math", include_str!("../defaults/math.toml")),
        page: parse_category("page", include_str!("../defaults/page.toml")),
    }
}

fn parse_category<T: for<'de> Deserialize<'de>>(name: &str, raw: &str) -> T {
    toml::from_str(raw).unwrap_or_else(|e| {
        panic!("invalid bundled defaults/{name}.toml: {e}");
    })
}

fn push_category<T: Serialize>(lines: &mut Vec<String>, category: &str, value: &T) {
    let Ok(toml::Value::Table(table)) = toml::Value::try_from(value) else {
        panic!("{category} knobs must serialize to a TOML table");
    };
    let mut sections: Vec<_> = table.keys().cloned().collect();
    sections.sort();
    for section in sections {
        let Some(toml::Value::Table(fields)) = table.get(&section) else {
            continue;
        };
        let mut keys: Vec<_> = fields.keys().cloned().collect();
        keys.sort();
        for key in keys {
            let formatted = format_toml_value(&fields[&key]);
            lines.push(format!("{category}.{section}.{key} = {formatted}"));
        }
    }
}

fn format_toml_value(v: &toml::Value) -> String {
    match v {
        toml::Value::Float(f) => {
            // Prefer short decimal form for agent dumps (avoid f32 binary noise).
            let rounded = (*f * 1_000_000.0).round() / 1_000_000.0;
            let s = format!("{rounded}");
            if s.contains('.') {
                s.trim_end_matches('0').trim_end_matches('.').to_string()
            } else {
                s
            }
        }
        other => other.to_string(),
    }
}

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
    /// Figure trailing gaps.
    pub figure: ProseFigureKnobs,
    /// Figure caption size / italic / optional color.
    pub caption: ProseCaptionKnobs,
    /// Wrap helpers.
    pub wrap: ProseWrapKnobs,
    /// Default body text color (optional; omit for engine black).
    #[serde(default, skip_serializing_if = "ProseTextKnobs::is_empty")]
    pub text: ProseTextKnobs,
    /// Citation marker paint policy.
    #[serde(default, skip_serializing_if = "ProseCiteKnobs::is_default")]
    pub cite: ProseCiteKnobs,
}

/// `#RGB` / `#RRGGBB` color for aesthetic knobs (0..=255 channels).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HexColor {
    /// Red 0..=255.
    pub r: u8,
    /// Green 0..=255.
    pub g: u8,
    /// Blue 0..=255.
    pub b: u8,
}

impl HexColor {
    /// Parse `#RGB` or `#RRGGBB` (case-insensitive hex digits).
    ///
    /// # Errors
    ///
    /// Returns a message when the string is not a valid hex color.
    pub fn parse(raw: &str) -> Result<Self, String> {
        let s = raw.trim();
        let hex = s
            .strip_prefix('#')
            .ok_or_else(|| format!("expected #RGB or #RRGGBB, got {raw:?}"))?;
        let full = match hex.len() {
            3 => {
                let mut expanded = String::with_capacity(6);
                for ch in hex.chars() {
                    expanded.push(ch);
                    expanded.push(ch);
                }
                expanded
            }
            6 => hex.to_string(),
            _ => {
                return Err(format!(
                    "expected #RGB or #RRGGBB (3 or 6 hex digits), got {raw:?}"
                ));
            }
        };
        if !full.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(format!("non-hex digit in color {raw:?}"));
        }
        let n = u32::from_str_radix(&full, 16).map_err(|_| format!("invalid hex color {raw:?}"))?;
        Ok(Self {
            r: ((n >> 16) & 0xff) as u8,
            g: ((n >> 8) & 0xff) as u8,
            b: (n & 0xff) as u8,
        })
    }

    /// Canonical `#RRGGBB` form.
    #[must_use]
    pub fn to_hex_string(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }

    /// PDF `set_fill_rgb` components in 0.0..=1.0.
    #[must_use]
    pub fn to_rgb01(self) -> [f32; 3] {
        [
            f32::from(self.r) / 255.0,
            f32::from(self.g) / 255.0,
            f32::from(self.b) / 255.0,
        ]
    }
}

impl Serialize for HexColor {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex_string())
    }
}

impl<'de> Deserialize<'de> for HexColor {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(serde::de::Error::custom)
    }
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
}

/// Horizontal alignment for figure image + caption band (`[figure].align`).
///
/// Sealed enum — no freeform x. Vertical float / wrap stay out of scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FigureAlign {
    /// Center within the content box (default).
    #[default]
    Center,
    /// Flush to the content-box start.
    Left,
    /// Flush to the content-box end.
    Right,
}

impl FigureAlign {
    /// Horizontal offset from the content-box left for an item of `item_w`.
    #[must_use]
    pub fn offset_x(self, content_w: f32, item_w: f32) -> f32 {
        let slack = (content_w - item_w).max(0.0);
        match self {
            Self::Left => 0.0,
            Self::Center => slack / 2.0,
            Self::Right => slack,
        }
    }
}

/// Resolved in-band text alignment (no `follow`) used by layout / paint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
    /// Flush to the band / content start.
    #[default]
    Left,
    /// Center within the measure.
    Center,
    /// Flush to the band / content end.
    Right,
    /// Distribute extra space across whitespace glyphs; start flush left.
    Justify,
}

impl TextAlign {
    /// Horizontal offset of natural-width text within `measure` (justify → `0`).
    #[must_use]
    pub fn offset_x(self, measure: f32, item_w: f32) -> f32 {
        match self {
            Self::Justify => 0.0,
            Self::Left => FigureAlign::Left.offset_x(measure, item_w),
            Self::Center => FigureAlign::Center.offset_x(measure, item_w),
            Self::Right => FigureAlign::Right.offset_x(measure, item_w),
        }
    }
}

impl From<FigureAlign> for TextAlign {
    fn from(value: FigureAlign) -> Self {
        match value {
            FigureAlign::Left => Self::Left,
            FigureAlign::Center => Self::Center,
            FigureAlign::Right => Self::Right,
        }
    }
}

/// Title band alignment relative to the figure (`[figure].title_align`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FigureTitleAlign {
    /// Same as [`ProseFigureKnobs::align`] (default).
    #[default]
    Follow,
    /// Flush left in the content box.
    Left,
    /// Center in the content box.
    Center,
    /// Flush right in the content box.
    Right,
}

impl FigureTitleAlign {
    /// Resolve to a concrete [`FigureAlign`] given the figure's image align.
    #[must_use]
    pub fn resolve(self, figure_align: FigureAlign) -> FigureAlign {
        resolve_follow_align(self.into(), figure_align)
    }
}

/// In-band text alignment for figure title / caption (`follow` = figure `align`).
///
/// Placement of the band is separate ([`FigureTitleAlign`] / [`CaptionBand`]);
/// this controls left / center / right / justify *within* that band (or full measure).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FigureTextAlign {
    /// Same as [`ProseFigureKnobs::align`] (default). Never resolves to [`TextAlign::Justify`].
    #[default]
    Follow,
    /// Flush to the band start.
    Left,
    /// Center within the band.
    Center,
    /// Flush to the band end.
    Right,
    /// Word-justify within the band (last soft-wrapped line stays left).
    Justify,
}

impl FigureTextAlign {
    /// Resolve to a concrete [`TextAlign`] for in-band text placement.
    #[must_use]
    pub fn resolve(self, figure_align: FigureAlign) -> TextAlign {
        match self {
            Self::Follow => TextAlign::from(figure_align),
            Self::Left => TextAlign::Left,
            Self::Center => TextAlign::Center,
            Self::Right => TextAlign::Right,
            Self::Justify => TextAlign::Justify,
        }
    }
}

/// Shared Follow / Left / Center / Right choice for band placement (`title_align`).
#[derive(Clone, Copy)]
enum FollowOrAlign {
    Follow,
    Left,
    Center,
    Right,
}

fn resolve_follow_align(choice: FollowOrAlign, figure_align: FigureAlign) -> FigureAlign {
    match choice {
        FollowOrAlign::Follow => figure_align,
        FollowOrAlign::Left => FigureAlign::Left,
        FollowOrAlign::Center => FigureAlign::Center,
        FollowOrAlign::Right => FigureAlign::Right,
    }
}

impl From<FigureTitleAlign> for FollowOrAlign {
    fn from(value: FigureTitleAlign) -> Self {
        match value {
            FigureTitleAlign::Follow => Self::Follow,
            FigureTitleAlign::Left => Self::Left,
            FigureTitleAlign::Center => Self::Center,
            FigureTitleAlign::Right => Self::Right,
        }
    }
}

/// Whether caption text shares the image horizontal band (`[caption].band`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptionBand {
    /// Indent + wrap width match the laid image (default).
    #[default]
    MatchFigure,
    /// Caption at content-box left with full content wrap width.
    FullMeasure,
}

/// Overlong-token policy for caption wrap (`[caption].overflow`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptionOverflow {
    /// Split tokens wider than the wrap measure (default).
    #[default]
    HardBreak,
    /// Wrap on whitespace only; overlong tokens may stick out of the band.
    SoftOnly,
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

/// Table layout knobs (`defaults/table.toml`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableKnobs {
    /// Cell padding / leading.
    pub cell: TableCellKnobs,
    /// Outer table block gap.
    pub block: TableBlockKnobs,
}

/// `[cell]` in `table.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableCellKnobs {
    /// Cell padding (points).
    pub pad: f32,
    /// Cell line leading factor (capped by body leading).
    pub leading_factor: f32,
    /// Minimum inner cell content width (points).
    pub min_inner_width: f32,
}

/// `[block]` in `table.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableBlockKnobs {
    /// Gap after the table (points).
    pub gap_after: f32,
}

/// Deck / slide knobs (`defaults/deck.toml`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeckKnobs {
    /// Slide frame.
    pub slide: DeckSlideKnobs,
    /// Title region.
    pub title: DeckTitleKnobs,
    /// Subtitle region.
    pub subtitle: DeckSubtitleKnobs,
    /// Body / list regions.
    pub body: DeckBodyKnobs,
    /// Two-column layout.
    pub columns: DeckColumnsKnobs,
}

/// `[slide]` in `deck.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeckSlideKnobs {
    /// Top gap on a new slide (points).
    pub top_gap: f32,
}

/// `[title]` in `deck.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeckTitleKnobs {
    /// Title size scale on `deck@0`.
    pub scale: f32,
    /// Title size scale when not in deck profile.
    pub scale_non_deck: f32,
    /// Gap after title on deck.
    pub gap_after: f32,
    /// Gap after title off deck.
    pub gap_after_non_deck: f32,
}

/// `[subtitle]` in `deck.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeckSubtitleKnobs {
    /// Subtitle size as a factor of body size.
    pub size_factor: f32,
    /// Gap after subtitle (points).
    pub gap_after: f32,
}

/// `[body]` in `deck.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeckBodyKnobs {
    /// Gap after body region text (points).
    pub gap_after: f32,
    /// Tight gap between list-like slide regions (points).
    pub region_gap_after: f32,
    /// Slide list text size factor of body size.
    pub list_size_factor: f32,
}

/// `[columns]` in `deck.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeckColumnsKnobs {
    /// Two-column gap on deck (points).
    pub gap: f32,
    /// Two-column gap off deck (points).
    pub gap_non_deck: f32,
    /// Gap after a columns band on deck (points).
    pub gap_after: f32,
    /// Gap after a columns band off deck (points).
    pub gap_after_non_deck: f32,
    /// Gap after each region inside a column (points).
    pub region_gap_after: f32,
    /// Minimum column width (points).
    pub min_width: f32,
}

/// Math layout knobs (`defaults/math.toml`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MathKnobs {
    /// Display vs inline framing.
    pub display: MathDisplayKnobs,
    /// Axis / mu / `.notdef` metrics.
    pub metrics: MathMetricsKnobs,
    /// Big operators (∑ / ∏ / ∫) and display under/over limits.
    pub op: MathOpKnobs,
    /// Geometric arrows.
    pub arrow: MathArrowKnobs,
    /// Infinity glyph optics.
    pub infinity: MathInfinityKnobs,
    /// Fractions.
    pub frac: MathFracKnobs,
    /// Super/subscripts.
    pub script: MathScriptKnobs,
    /// Matrices.
    pub matrix: MathMatrixKnobs,
    /// Stretchy parentheses.
    pub paren: MathParenKnobs,
    /// Square-root radical + vinculum.
    pub sqrt: MathSqrtKnobs,
}

/// `[display]` in `math.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MathDisplayKnobs {
    /// Display math size as a factor of body size.
    pub size_factor: f32,
    /// Gap before a math block (points).
    pub pre_gap: f32,
    /// Gap after display math (points).
    pub gap_after: f32,
    /// Gap after inline math (points).
    pub inline_gap_after: f32,
}

/// `[metrics]` in `math.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MathMetricsKnobs {
    /// Math axis height as a factor of font size.
    pub axis_factor: f32,
    /// TeX math units per em (usually 18).
    pub mu_per_em: f32,
    /// Fallback ink above when glyph bounds missing.
    pub fallback_ink_above_factor: f32,
    /// `.notdef` box height factor.
    pub notdef_height_factor: f32,
    /// `.notdef` box depth factor.
    pub notdef_depth_factor: f32,
}

/// `[op]` in `math.toml` — large operators and display-style limits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MathOpKnobs {
    /// Glyph scale for ∑ / ∏ / ∫ relative to surrounding math size.
    pub size_factor: f32,
    /// Limit (under/over) size as a factor of surrounding math size.
    pub limit_size_factor: f32,
    /// Gap above the operator to the upper limit (factor of font size).
    pub gap_above_factor: f32,
    /// Gap below the operator to the lower limit (factor of font size).
    pub gap_below_factor: f32,
    /// Extra horizontal space after an op-with-limits atom (mu).
    pub after_space_mu: f32,
}

/// `[arrow]` in `math.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MathArrowKnobs {
    /// Minimum arrow height factor.
    pub min_height_factor: f32,
    /// Arrow width factor.
    pub width_factor: f32,
    /// Arrow stroke thickness factor.
    pub thickness_factor: f32,
    /// Arrow thickness clamp min (points).
    pub thickness_min: f32,
    /// Arrow thickness clamp max (points).
    pub thickness_max: f32,
}

/// `[infinity]` in `math.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MathInfinityKnobs {
    /// Optical reference above factor.
    pub ref_above_factor: f32,
    /// Optical reference below factor.
    pub ref_below_factor: f32,
    /// Scale boost vs letter ink.
    pub scale_boost: f32,
    /// Scale clamp min.
    pub scale_min: f32,
    /// Scale clamp max.
    pub scale_max: f32,
    /// Vertical lower tweak factor.
    pub lower_factor: f32,
    /// Horizontal pad in mu.
    pub pad_mu: f32,
}

/// `[frac]` in `math.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MathFracKnobs {
    /// Numerator/denominator size factor.
    pub script_size_factor: f32,
    /// Rule thickness factor.
    pub rule_thickness_factor: f32,
    /// Rule thickness minimum (points).
    pub rule_thickness_min: f32,
    /// Gap above fraction rule.
    pub gap_num_factor: f32,
    /// Gap below fraction rule.
    pub gap_den_factor: f32,
    /// Horizontal pad inside fraction.
    pub pad_factor: f32,
}

/// `[script]` in `math.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MathScriptKnobs {
    /// Super/subscript size factor.
    pub size_factor: f32,
    /// Gap between base and scripts (mu).
    pub gap_mu: f32,
    /// Script overlap / italic correction (mu).
    pub overlap_mu: f32,
    /// Superscript raise factor of font size.
    pub superscript_raise_factor: f32,
    /// Subscript lower factor of font size.
    pub subscript_lower_factor: f32,
}

/// `[matrix]` in `math.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MathMatrixKnobs {
    /// Cell size as a factor of font size.
    pub cell_size: f32,
    /// Column gap as a factor of font size.
    pub col_gap: f32,
    /// Row gap as a factor of font size.
    pub row_gap: f32,
    /// Outer pad as a factor of font size.
    pub pad: f32,
}

/// `[paren]` in `math.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MathParenKnobs {
    /// Width as factor of half-height.
    pub width_factor: f32,
    /// Width clamp min (points).
    pub width_min: f32,
    /// Width clamp max (points).
    pub width_max: f32,
    /// Stroke thickness factor.
    pub thickness_factor: f32,
    /// Thickness clamp min.
    pub thickness_min: f32,
    /// Thickness clamp max.
    pub thickness_max: f32,
}

/// `[sqrt]` in `math.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MathSqrtKnobs {
    /// Gap between vinculum and radicand (factor of font size).
    pub gap_factor: f32,
    /// Vinculum thickness factor.
    pub rule_thickness_factor: f32,
    /// Vinculum thickness minimum (points).
    pub rule_thickness_min: f32,
    /// Extra pad after the radical before content (factor of font size).
    pub pad_factor: f32,
    /// Horizontal overhang of the vinculum past the radicand (factor of font size).
    pub overhang_factor: f32,
}

/// Page chrome knobs (`defaults/page.toml`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageKnobs {
    /// Page-number footer.
    pub footer: PageFooterKnobs,
    /// Content box clearance.
    pub content: PageContentKnobs,
    /// Stroke / fill gray for rules and math chrome.
    pub chrome: PageChromeKnobs,
}

/// `[footer]` in `page.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageFooterKnobs {
    /// Footer font size (points).
    pub font_size: f32,
    /// Footer baseline as a factor of bottom margin.
    pub y_margin_factor: f32,
}

/// `[content]` in `page.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageContentKnobs {
    /// Extra clearance above bottom margin when painting (points).
    pub bottom_clearance: f32,
}

/// `[chrome]` in `page.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageChromeKnobs {
    /// Default stroke gray for rules / math chrome.
    pub stroke_gray: f32,
    /// Default fill gray for math chrome.
    pub fill_gray: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_loads_and_describes() {
        let k = LayoutKnobs::bundled();
        assert!((k.prose.paragraph.gap_after - 10.0).abs() < f32::EPSILON);
        assert_eq!(k.prose.paragraph.text_align, TextAlign::Left);
        assert!((k.math.metrics.mu_per_em - 18.0).abs() < f32::EPSILON);
        assert!((k.math.frac.gap_num_factor - 0.1).abs() < f32::EPSILON);
        assert!((k.math.op.size_factor - 1.35).abs() < f32::EPSILON);
        assert!((k.math.op.after_space_mu - 2.5).abs() < f32::EPSILON);
        let dump = k.describe();
        assert!(dump.contains("prose.paragraph.gap_after = 10"));
        assert!(
            dump.contains("prose.paragraph.text_align = \"left\"")
                || dump.contains("prose.paragraph.text_align = left")
        );
        assert!(dump.contains("math.metrics.axis_factor = 0.25"));
        assert!(dump.contains("math.op.limit_size_factor = 0.65"));
        assert!(dump.contains("page.footer.font_size = 9"));
        assert!(dump.contains("prose.heading.leading_factor = 1.35"));
        assert!(k.prose.quote.italic);
        assert!(dump.contains("prose.quote.italic = true"));
        assert!(k.prose.caption.italic);
        assert!((k.prose.caption.size_factor - 0.9).abs() < f32::EPSILON);
        assert!((k.prose.caption.gap_after - 6.0).abs() < f32::EPSILON);
        assert!((k.prose.caption.leading_factor - 1.15).abs() < f32::EPSILON);
        assert!((k.prose.figure.gap_after_image - 2.0).abs() < f32::EPSILON);
        assert!((k.prose.figure.gap_before - 6.0).abs() < f32::EPSILON);
        assert!((k.prose.figure.gap_after_title - 18.0).abs() < f32::EPSILON);
        assert_eq!(k.prose.figure.align, FigureAlign::Center);
        assert!((k.prose.figure.max_width_factor - 1.0).abs() < f32::EPSILON);
        assert_eq!(k.prose.figure.title_align, FigureTitleAlign::Follow);
        assert_eq!(k.prose.figure.title_text_align, FigureTextAlign::Center);
        assert_eq!(k.prose.caption.band, CaptionBand::MatchFigure);
        assert_eq!(k.prose.caption.text_align, FigureTextAlign::Left);
        assert_eq!(k.prose.caption.overflow, CaptionOverflow::HardBreak);
        assert!(dump.contains("prose.caption.italic = true"));
        assert!(dump.contains("prose.caption.size_factor = 0.9"));
        assert!(dump.contains("prose.caption.leading_factor = 1.15"));
        assert!(dump.contains("prose.figure.gap_after_image = 2"));
        assert!(dump.contains("prose.figure.gap_before = 6"));
        assert!(dump.contains("prose.figure.gap_after_title = 18"));
        assert!(
            dump.contains("prose.figure.align = \"center\"")
                || dump.contains("prose.figure.align = center")
        );
        assert!(dump.contains("prose.figure.max_width_factor = 1"));
        assert!(
            dump.contains("prose.figure.title_align = \"follow\"")
                || dump.contains("prose.figure.title_align = follow")
        );
        assert!(
            dump.contains("prose.figure.title_text_align = \"center\"")
                || dump.contains("prose.figure.title_text_align = center")
        );
        assert!(
            dump.contains("prose.caption.band = \"match_figure\"")
                || dump.contains("prose.caption.band = match_figure")
        );
        assert!(
            dump.contains("prose.caption.text_align = \"left\"")
                || dump.contains("prose.caption.text_align = left")
        );
        assert!(
            dump.contains("prose.caption.overflow = \"hard_break\"")
                || dump.contains("prose.caption.overflow = hard_break")
        );
        assert!(k.prose.text.color.is_none());
        assert!(k.prose.quote.color.is_none());
        assert!(k.prose.caption.color.is_none());
        assert!(k.prose.cite.color.is_none());
        assert!(!k.prose.cite.underline);
        assert!(
            !dump.contains("prose.text.") && !dump.contains("prose.cite."),
            "omitted aesthetic keys should not appear in dump: {dump}"
        );
    }

    #[test]
    fn hex_color_parses_rgb_and_rrggbb() {
        assert_eq!(
            HexColor::parse("#abc").unwrap(),
            HexColor {
                r: 0xaa,
                g: 0xbb,
                b: 0xcc
            }
        );
        assert_eq!(
            HexColor::parse("#336699").unwrap(),
            HexColor {
                r: 0x33,
                g: 0x66,
                b: 0x99
            }
        );
        assert!(HexColor::parse("336699").is_err());
        assert!(HexColor::parse("#gg0000").is_err());
        assert!(HexColor::parse("#12").is_err());
    }

    #[test]
    fn prose_toml_overlay_sets_aesthetic_keys() {
        let mut k = LayoutKnobs::bundled();
        let overlay = r##"
[paragraph]
gap_after = 10.0

[heading]
leading_factor = 1.35
gap_after = 8.0
font = "display"

[quote]
indent = 18.0
italic = true
color = "#445566"
font = "armenian"

[code]
leading_factor = 1.25
gap_after = 10.0

[list]
indent_per_depth = 18.0
item_leading_factor = 1.35

[figure]
gap_after = 6.0
alt_gap_after = 10.0
gap_after_image = 2.0
gap_before = 6.0
gap_after_title = 8.0
align = "center"
max_width_factor = 0.75
title_align = "left"
title_text_align = "center"

[caption]
italic = true
size_factor = 0.9
leading_factor = 1.15
gap_after = 6.0
band = "full_measure"
text_align = "right"
overflow = "soft_only"
color = "#556677"
font = "body"

[wrap]
body_leading_factor = 1.35
min_width = 36.0

[text]
color = "#112233"
font = "body"

[cite]
color = "#990000"
underline = true
font = "body"
"##;
        k.prose = toml::from_str(overlay).expect("overlay");
        assert_eq!(k.prose.text.color.unwrap().to_hex_string(), "#112233");
        assert_eq!(k.prose.quote.color.unwrap().to_hex_string(), "#445566");
        assert_eq!(k.prose.cite.color.unwrap().to_hex_string(), "#990000");
        assert_eq!(k.prose.caption.color.unwrap().to_hex_string(), "#556677");
        assert!(k.prose.cite.underline);
        assert_eq!(k.prose.figure.align, FigureAlign::Center);
        assert!((k.prose.figure.max_width_factor - 0.75).abs() < f32::EPSILON);
        assert_eq!(k.prose.figure.title_align, FigureTitleAlign::Left);
        assert_eq!(k.prose.figure.title_text_align, FigureTextAlign::Center);
        assert!((k.prose.figure.gap_after_title - 8.0).abs() < f32::EPSILON);
        assert_eq!(k.prose.caption.band, CaptionBand::FullMeasure);
        assert_eq!(k.prose.caption.text_align, FigureTextAlign::Right);
        assert_eq!(k.prose.caption.overflow, CaptionOverflow::SoftOnly);
        assert_eq!(k.prose.text.font.as_deref(), Some("body"));
        assert_eq!(k.prose.heading.font.as_deref(), Some("display"));
        assert_eq!(k.prose.quote.font.as_deref(), Some("armenian"));
        assert_eq!(k.prose.cite.font.as_deref(), Some("body"));
        assert_eq!(k.prose.caption.font.as_deref(), Some("body"));
        let dump = k.describe();
        assert!(dump.contains("prose.text.color = \"#112233\""));
        assert!(dump.contains("prose.cite.underline = true"));
        assert!(dump.contains("prose.caption.color = \"#556677\""));
        assert!(dump.contains("prose.heading.font = \"display\""));
        assert!(dump.contains("prose.text.font = \"body\""));
        assert!(dump.contains("prose.quote.font = \"armenian\""));
        assert!(dump.contains("prose.cite.font = \"body\""));
        assert!(dump.contains("prose.caption.font = \"body\""));
        let bundled = LayoutKnobs::bundled().describe();
        assert!(
            !bundled.contains("prose.text.font"),
            "bundled dump should omit unset category fonts"
        );
        assert!(!bundled.contains("prose.heading.font"));
        assert!(!bundled.contains("prose.quote.font"));
        assert!(!bundled.contains("prose.cite.font"));
        assert!(!bundled.contains("prose.caption.font"));
        assert!(!bundled.contains("prose.caption.color"));
    }
}
