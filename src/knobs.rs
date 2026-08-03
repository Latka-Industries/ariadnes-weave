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
    /// Quote indent.
    pub quote: ProseQuoteKnobs,
    /// Code block leading / gap.
    pub code: ProseCodeKnobs,
    /// List indent / leading.
    pub list: ProseListKnobs,
    /// Figure trailing gaps.
    pub figure: ProseFigureKnobs,
    /// Wrap helpers.
    pub wrap: ProseWrapKnobs,
}

/// `[paragraph]` in `prose.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProseParagraphKnobs {
    /// Gap after a body paragraph (points).
    pub gap_after: f32,
}

/// `[heading]` in `prose.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProseHeadingKnobs {
    /// Heading leading as a multiple of heading font size.
    pub leading_factor: f32,
    /// Gap after a heading block (points).
    pub gap_after: f32,
}

/// `[quote]` in `prose.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProseQuoteKnobs {
    /// Quote left indent (points).
    pub indent: f32,
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

/// `[figure]` in `prose.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProseFigureKnobs {
    /// Gap after a figure (points).
    pub gap_after: f32,
    /// Gap after a figure alt-text placeholder (points).
    pub alt_gap_after: f32,
}

/// `[wrap]` in `prose.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProseWrapKnobs {
    /// Generic body run leading factor.
    pub body_leading_factor: f32,
    /// Minimum wrap width (points).
    pub min_width: f32,
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
    /// Extra space after big operators (mu).
    pub op_after_space_mu: f32,
    /// `.notdef` box height factor.
    pub notdef_height_factor: f32,
    /// `.notdef` box depth factor.
    pub notdef_depth_factor: f32,
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
        assert!((k.math.metrics.mu_per_em - 18.0).abs() < f32::EPSILON);
        assert!((k.math.frac.gap_num_factor - 0.1).abs() < f32::EPSILON);
        let dump = k.describe();
        assert!(dump.contains("prose.paragraph.gap_after = 10"));
        assert!(dump.contains("math.metrics.axis_factor = 0.25"));
        assert!(dump.contains("page.footer.font_size = 9"));
        assert!(dump.contains("prose.heading.leading_factor = 1.35"));
    }
}
