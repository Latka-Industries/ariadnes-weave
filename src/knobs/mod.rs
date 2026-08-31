//! Named layout optical defaults (per-category TOML under `defaults/`).
//!
//! Profiles own page size / margins / body size ([`crate::profile`]). These knobs
//! are the hard-coded paddings, gaps, and scale factors inside emit.

mod align;
mod color;
mod deck;
mod math;
mod page;
mod prose;
mod table;

pub use align::{
    CaptionBand, CaptionOverflow, FigureAlign, FigureTextAlign, FigureTitleAlign, TextAlign,
};
pub use color::HexColor;
pub use deck::*;
pub use math::*;
pub use page::*;
pub use prose::*;
pub use table::*;

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
    /// Page chrome (header/footer, clearance, stroke gray).
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

    /// Tighten gaps / paddings / list gutter for `resume@0` emits.
    ///
    /// Profiles own page/type; this overlays optical density on host knobs
    /// (pack fonts/colors still apply).
    pub fn densify_resume(&mut self) {
        // Match LaTeX `\fontsize{9.5pt}{11.5pt}` rhythm — not sub-em leading.
        // (Earlier 1.02 crushed wrapped bullets: 9.5×1.02 < body size.)
        self.prose.paragraph.gap_after = 2.0;
        self.prose.heading.gap_after = 1.0;
        self.prose.heading.leading_factor = 1.12;
        self.prose.list.indent_per_depth = 12.0;
        self.prose.list.item_leading_factor = 11.5 / 9.5;
        // ~1.25 in — clears date column on one-column CVs.
        self.prose.list.end_gutter = 90.0;
        // LaTeX `\parindent` band: org=1 → 14pt, role/bullets=2 → 28pt.
        self.prose.indent.step = 14.0;
        self.prose.code.gap_after = 2.0;
        // LaTeX dark-blue links / icons (`rgb{0.15,0.15,0.4}` ≈ #262666).
        self.prose.cite.color = Some(HexColor {
            r: 0x26,
            g: 0x26,
            b: 0x66,
        });
        self.table.cell.pad = 1.0;
        self.table.cell.leading_factor = 11.5 / 9.5;
        self.table.block.gap_after = 2.0;
        self.page.footer.enabled = false;
        self.page.header.enabled = false;
        self.page.content.bottom_clearance = 2.0;
        self.page.content.top_clearance = 0.0;
        self.prose.wrap.hyphenate = false;
    }
}

fn load_bundled() -> LayoutKnobs {
    LayoutKnobs {
        prose: parse_category("prose", include_str!("../../defaults/prose.toml")),
        table: parse_category("table", include_str!("../../defaults/table.toml")),
        deck: parse_category("deck", include_str!("../../defaults/deck.toml")),
        math: parse_category("math", include_str!("../../defaults/math.toml")),
        page: parse_category("page", include_str!("../../defaults/page.toml")),
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
        assert_eq!(k.math.paren.style, MathParenStyle::Round);
        let dump = k.describe();
        assert!(dump.contains("prose.paragraph.gap_after = 10"));
        assert!(
            dump.contains("prose.paragraph.text_align = \"left\"")
                || dump.contains("prose.paragraph.text_align = left")
        );
        assert!(dump.contains("math.metrics.axis_factor = 0.25"));
        assert!(dump.contains("math.op.limit_size_factor = 0.65"));
        assert!(dump.contains("page.footer.font_size = 9"));
        assert!(
            dump.contains("page.footer.format = \"{page} / {pages}\"")
                || dump.contains("page.footer.format = {page} / {pages}")
        );
        assert!(dump.contains("page.header.enabled = false"));
        assert!(dump.contains("prose.wrap.hyphenate = true"));
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
