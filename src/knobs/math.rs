//! Structured math optical constants (`defaults/math.toml`).

use serde::{Deserialize, Serialize};

/// Math layout knobs (`defaults/math.toml`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MathKnobs {
    /// Display vs inline framing.
    pub display: MathDisplayKnobs,
    /// Axis / mu / `.notdef` metrics.
    pub metrics: MathMetricsKnobs,
    /// Big operators (∑ / ∏ / ∫): display `.v1` glyphs; ∑-family under/over; ∫ `\nolimits`.
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
    /// `\bar` accent (THI-385).
    #[serde(default)]
    pub accent: MathAccentKnobs,
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
    /// Geometric fallback scale when the face lacks the op glyph (display uses `.v1`).
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

/// `[paren]` in `math.toml` — stretchy matrix delimiters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MathParenKnobs {
    /// Delimiter shape for `pmatrix`: `round` `(…)` or `square` `[…]` (`bmatrix` is always square).
    pub style: MathParenStyle,
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

/// Matrix fence style (`[paren].style` in `math.toml`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MathParenStyle {
    /// Curved parentheses (TeX `pmatrix`).
    #[default]
    Round,
    /// Square brackets (TeX `bmatrix`-like).
    Square,
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

/// `[accent]` in `math.toml` — `\bar{…}` macron (THI-385).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MathAccentKnobs {
    /// Gap between nucleus and bar (factor of font size).
    #[serde(default = "default_accent_gap")]
    pub gap_factor: f32,
    /// Bar thickness factor.
    #[serde(default = "default_accent_thickness_factor")]
    pub thickness_factor: f32,
    /// Bar thickness minimum (points).
    #[serde(default = "default_accent_thickness_min")]
    pub thickness_min: f32,
    /// How much shorter the bar is than the nucleus (factor of font size).
    #[serde(default = "default_accent_overhang")]
    pub overhang_factor: f32,
}

fn default_accent_gap() -> f32 {
    0.08
}
fn default_accent_thickness_factor() -> f32 {
    0.045
}
fn default_accent_thickness_min() -> f32 {
    0.5
}
fn default_accent_overhang() -> f32 {
    0.12
}

impl Default for MathAccentKnobs {
    fn default() -> Self {
        Self {
            gap_factor: default_accent_gap(),
            thickness_factor: default_accent_thickness_factor(),
            thickness_min: default_accent_thickness_min(),
            overhang_factor: default_accent_overhang(),
        }
    }
}
