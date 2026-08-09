//! Token / delimiter prettify for math leaf strings.

/// Command → Unicode map, longest first (so `\rightarrow` wins over shorter prefixes).
const TOKEN_REPLACEMENTS: &[(&str, &str)] = &[
    // Arrows (longest first)
    ("\\Leftrightarrow", "⇔"),
    ("\\leftrightarrow", "↔"),
    ("\\Rightarrow", "⇒"),
    ("\\rightarrow", "→"),
    ("\\leftarrow", "←"),
    ("\\mapsto", "↦"),
    ("\\to", "→"),
    // Relations / sets
    ("\\subseteq", "⊆"),
    ("\\supseteq", "⊇"),
    ("\\subset", "⊂"),
    ("\\supset", "⊃"),
    ("\\notin", "∉"),
    ("\\approx", "≈"),
    ("\\infty", "∞"),
    ("\\equiv", "≡"),
    ("\\leq", "≤"),
    ("\\geq", "≥"),
    ("\\neq", "≠"),
    // Integral family before `\in` (otherwise `\int` → `∈t`).
    ("\\iiint", "∭"),
    ("\\iint", "∬"),
    ("\\oint", "∮"),
    ("\\int", "∫"),
    ("\\in", "∈"),
    ("\\forall", "∀"),
    ("\\exists", "∃"),
    ("\\emptyset", "∅"),
    ("\\varnothing", "∅"),
    // Binary ops
    ("\\times", "×"),
    ("\\cdot", "·"),
    ("\\circ", "∘"),
    ("\\div", "÷"),
    ("\\mp", "∓"),
    ("\\pm", "±"),
    ("\\cup", "∪"),
    ("\\cap", "∩"),
    // Big operators
    ("\\coprod", "∐"),
    ("\\bigcup", "⋃"),
    ("\\bigcap", "⋂"),
    ("\\sum", "∑"),
    ("\\prod", "∏"),
    // Calculus / misc
    ("\\partial", "∂"),
    ("\\nabla", "∇"),
    ("\\sqrt", "√"),
    ("\\ldots", "…"),
    ("\\dots", "…"),
    ("\\hbar", "ℏ"),
    // Greek capitals
    ("\\Gamma", "Γ"),
    ("\\Delta", "Δ"),
    ("\\Theta", "Θ"),
    ("\\Lambda", "Λ"),
    ("\\Xi", "Ξ"),
    ("\\Pi", "Π"),
    ("\\Sigma", "Σ"),
    ("\\Upsilon", "Υ"),
    ("\\Phi", "Φ"),
    ("\\Psi", "Ψ"),
    ("\\Omega", "Ω"),
    // Greek variants / lowercase (longest first)
    ("\\varepsilon", "ε"),
    ("\\vartheta", "ϑ"),
    ("\\varrho", "ϱ"),
    ("\\varphi", "φ"),
    ("\\alpha", "α"),
    ("\\beta", "β"),
    ("\\gamma", "γ"),
    ("\\delta", "δ"),
    ("\\epsilon", "ε"),
    ("\\zeta", "ζ"),
    ("\\eta", "η"),
    ("\\theta", "θ"),
    ("\\iota", "ι"),
    ("\\kappa", "κ"),
    ("\\lambda", "λ"),
    ("\\mu", "μ"),
    ("\\nu", "ν"),
    ("\\xi", "ξ"),
    ("\\pi", "π"),
    ("\\rho", "ρ"),
    ("\\sigma", "σ"),
    ("\\tau", "τ"),
    ("\\upsilon", "υ"),
    ("\\phi", "φ"),
    ("\\chi", "χ"),
    ("\\psi", "ψ"),
    ("\\omega", "ω"),
    // Spacing
    ("\\ ", " "),
    ("\\,", " "),
    ("\\;", " "),
    ("\\!", ""),
];

pub(in crate::emit) fn prettify_latex_math(latex: &str) -> String {
    let mut s = strip_math_delimiters(latex);
    s = prettify_tokens(&s);
    s = s.replace(['{', '}'], "");
    s = apply_script_chars(&s, '^', true);
    s = apply_script_chars(&s, '_', false);
    if s.is_empty() { "[math]".into() } else { s }
}

pub(super) fn strip_math_delimiters(latex: &str) -> String {
    let mut s = latex.trim().to_string();
    for wrap in ["$$", "$", "\\[", "\\]", "\\(", "\\)"] {
        if let Some(stripped) = s.strip_prefix(wrap) {
            s = stripped.to_string();
        }
        if let Some(stripped) = s.strip_suffix(wrap) {
            s = stripped.to_string();
        }
    }
    s.trim().to_string()
}

pub(super) fn prettify_tokens(input: &str) -> String {
    let mut s = input.to_string();
    for &(from, to) in TOKEN_REPLACEMENTS {
        s = s.replace(from, to);
    }
    s
}

fn apply_script_chars(input: &str, marker: char, super_script: bool) -> String {
    let map = if super_script {
        to_superscript
    } else {
        to_subscript
    };
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == marker
            && let Some(&next) = chars.peek()
            && let Some(rep) = map(next)
        {
            out.push(rep);
            chars.next();
            continue;
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

#[cfg(test)]
mod prettify_tests {
    use super::prettify_tokens;

    #[test]
    fn maps_delta_and_long_arrows() {
        assert_eq!(prettify_tokens(r"\Delta"), "Δ");
        assert_eq!(prettify_tokens(r"\Leftrightarrow"), "⇔");
        assert_eq!(prettify_tokens(r"\subseteq"), "⊆");
        assert_eq!(prettify_tokens(r"\oint"), "∮");
        assert_eq!(prettify_tokens(r"\int"), "∫");
        assert_eq!(prettify_tokens(r"\in"), "∈");
        assert_ne!(prettify_tokens(r"\int"), "∈t");
    }
}
