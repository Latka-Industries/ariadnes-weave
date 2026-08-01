//! Display/inline math layout via a light LaTeX-token prettifier (not TeX).

use crate::error::WeaveError;
use crate::font::{FaceId, FaceRef, FontBag};
use crate::profile::ProfileMetrics;

use super::types::{GlyphSets, LaidItem, LaidLine, LayoutSegment};

/// Lay out prettified math as italic text; display math is centered with gaps.
pub(super) fn layout_math(
    display: bool,
    latex: &str,
    metrics: &ProfileMetrics,
    fonts: &FontBag,
    segments: &mut [LayoutSegment],
    glyph_sets: &mut GlyphSets,
) -> Result<(), WeaveError> {
    let pretty = prettify_latex_math(latex);
    let face = FaceRef::Bundled(if metrics.serif_body {
        FaceId::SerifItalic
    } else {
        FaceId::SansItalic
    });
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
    let mut line = LaidLine::shaped(fonts, face, &pretty, font_size, leading, glyph_sets)?;
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
pub(super) fn prettify_latex_math(latex: &str) -> String {
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
