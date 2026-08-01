//! Structured math layout for a small LaTeX subset (fractions, scripts, matrices).
//!
//! Leaf tokens still map through [`prettify_tokens`]. This is not a TeX engine.

use crate::error::WeaveError;
use crate::font::{
    FaceId, FaceRef, FontBag, ShapedGlyph, collect_glyph_set, note_shaped_glyphs, shape_text,
    shaped_width,
};
use crate::profile::ProfileMetrics;

use super::types::{GlyphSets, LaidItem, LaidLine, LaidMath, LaidMathEl, LayoutSegment};

/// Command → Unicode map, longest first (so `\rightarrow` wins over shorter prefixes).
const TOKEN_REPLACEMENTS: &[(&str, &str)] = &[
    ("\\rightarrow", "→"),
    ("\\leftarrow", "←"),
    ("\\Rightarrow", "⇒"),
    ("\\approx", "≈"),
    ("\\infty", "∞"),
    ("\\times", "×"),
    ("\\cdot", "·"),
    ("\\leq", "≤"),
    ("\\geq", "≥"),
    ("\\neq", "≠"),
    ("\\pm", "±"),
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
];

/// Lay out display/inline math as a structured box.
pub(super) fn layout_math(
    display: bool,
    latex: &str,
    metrics: &ProfileMetrics,
    fonts: &FontBag,
    segments: &mut [LayoutSegment],
    glyph_sets: &mut GlyphSets,
) -> Result<(), WeaveError> {
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
    let seg = segments.last_mut().expect("segment");
    if display {
        seg.1.push(LaidItem::Text(LaidLine::gap(8.0)));
    }

    let body = strip_math_delimiters(latex);
    let expr = parse_math(&body).unwrap_or_else(|_| MathExpr::Ord(prettify_latex_math(latex)));
    let math = layout_expr(&expr, fonts, face, font_size, glyph_sets)?;
    seg.1.push(LaidItem::Math(LaidMath {
        width: math.width,
        height: math.height + math.depth,
        center: display,
        gap_after: if display { 12.0 } else { 4.0 },
        elements: shift_to_top_origin(math),
    }));
    Ok(())
}

/// Light LaTeX-math prettifier (delimiters + common tokens). Kept for simple
/// fixtures and as a leaf/fallback path — not a TeX engine.
pub(super) fn prettify_latex_math(latex: &str) -> String {
    let mut s = strip_math_delimiters(latex);
    s = prettify_tokens(&s);
    s = s.replace(['{', '}'], "");
    s = apply_script_chars(&s, '^', true);
    s = apply_script_chars(&s, '_', false);
    if s.is_empty() { "[math]".into() } else { s }
}

fn strip_math_delimiters(latex: &str) -> String {
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

fn prettify_tokens(input: &str) -> String {
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

// --- AST -------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
enum MathExpr {
    Ord(String),
    Row(Vec<MathExpr>),
    Frac(Box<MathExpr>, Box<MathExpr>),
    Scripts {
        base: Box<MathExpr>,
        sup: Option<Box<MathExpr>>,
        sub: Option<Box<MathExpr>>,
    },
    Matrix {
        delimited: bool,
        rows: Vec<Vec<MathExpr>>,
    },
}

#[derive(Debug, Clone, Copy)]
enum ScriptKind {
    Sup,
    Sub,
}

#[derive(Debug)]
struct ParseError;

fn parse_math(input: &str) -> Result<MathExpr, ParseError> {
    let chars: Vec<char> = input.chars().collect();
    let mut p = Parser { chars, i: 0 };
    let expr = p.parse_row()?;
    if p.i != p.chars.len() && matches!(&expr, MathExpr::Row(items) if items.is_empty()) {
        return Err(ParseError);
    }
    Ok(flatten(expr))
}

fn flatten(expr: MathExpr) -> MathExpr {
    match expr {
        MathExpr::Row(mut items) if items.len() == 1 => flatten(items.remove(0)),
        MathExpr::Row(items) => MathExpr::Row(items.into_iter().map(flatten).collect()),
        MathExpr::Frac(n, d) => MathExpr::Frac(Box::new(flatten(*n)), Box::new(flatten(*d))),
        MathExpr::Scripts { base, sup, sub } => MathExpr::Scripts {
            base: Box::new(flatten(*base)),
            sup: sup.map(|e| Box::new(flatten(*e))),
            sub: sub.map(|e| Box::new(flatten(*e))),
        },
        MathExpr::Matrix { delimited, rows } => MathExpr::Matrix {
            delimited,
            rows: rows
                .into_iter()
                .map(|r| r.into_iter().map(flatten).collect())
                .collect(),
        },
        other => other,
    }
}

fn attach_script(nucleus: MathExpr, kind: ScriptKind, script: MathExpr) -> MathExpr {
    let script = Box::new(script);
    match (kind, nucleus) {
        (
            ScriptKind::Sup,
            MathExpr::Scripts {
                base,
                sup: None,
                sub,
            },
        ) => MathExpr::Scripts {
            base,
            sup: Some(script),
            sub,
        },
        (
            ScriptKind::Sub,
            MathExpr::Scripts {
                base,
                sup,
                sub: None,
            },
        ) => MathExpr::Scripts {
            base,
            sup,
            sub: Some(script),
        },
        (ScriptKind::Sup, base) => MathExpr::Scripts {
            base: Box::new(base),
            sup: Some(script),
            sub: None,
        },
        (ScriptKind::Sub, base) => MathExpr::Scripts {
            base: Box::new(base),
            sup: None,
            sub: Some(script),
        },
    }
}

struct Parser {
    chars: Vec<char>,
    i: usize,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.i).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.i += 1;
        Some(ch)
    }

    fn eat(&mut self, expected: char) -> Result<(), ParseError> {
        if self.bump() == Some(expected) {
            Ok(())
        } else {
            Err(ParseError)
        }
    }

    fn starts_with(&self, s: &str) -> bool {
        let needle: Vec<char> = s.chars().collect();
        self.chars[self.i..].starts_with(&needle)
    }

    fn eat_str(&mut self, s: &str) -> Result<(), ParseError> {
        if self.starts_with(s) {
            self.i += s.chars().count();
            Ok(())
        } else {
            Err(ParseError)
        }
    }

    fn skip_spaces(&mut self) {
        while matches!(self.peek(), Some(c) if c.is_whitespace()) {
            self.i += 1;
        }
    }

    fn at_seq_stop(&self) -> bool {
        match self.peek() {
            None | Some('}') | Some('&') => true,
            Some('\\') => self.starts_with("\\\\") || self.starts_with("\\end"),
            _ => false,
        }
    }

    fn parse_atom_seq(&mut self) -> Result<MathExpr, ParseError> {
        let mut items = Vec::new();
        loop {
            self.skip_spaces();
            if self.at_seq_stop() {
                break;
            }
            items.push(self.parse_atom()?);
        }
        Ok(MathExpr::Row(items))
    }

    fn parse_row(&mut self) -> Result<MathExpr, ParseError> {
        self.parse_atom_seq()
    }

    fn parse_cell(&mut self) -> Result<MathExpr, ParseError> {
        Ok(flatten(self.parse_atom_seq()?))
    }

    fn parse_atom(&mut self) -> Result<MathExpr, ParseError> {
        let mut nucleus = self.parse_nucleus()?;
        loop {
            self.skip_spaces();
            let kind = match self.peek() {
                Some('^') => ScriptKind::Sup,
                Some('_') => ScriptKind::Sub,
                _ => break,
            };
            self.bump();
            self.skip_spaces();
            let script = self.parse_nucleus()?;
            nucleus = attach_script(nucleus, kind, script);
        }
        Ok(nucleus)
    }

    fn parse_nucleus(&mut self) -> Result<MathExpr, ParseError> {
        self.skip_spaces();
        match self.peek() {
            Some('{') => {
                self.bump();
                let inner = self.parse_row()?;
                self.skip_spaces();
                self.eat('}')?;
                Ok(inner)
            }
            Some('\\') => self.parse_command(),
            Some(ch) if !matches!(ch, '}' | '&' | '^' | '_') => {
                self.bump();
                Ok(MathExpr::Ord(ch.to_string()))
            }
            _ => Err(ParseError),
        }
    }

    fn parse_command(&mut self) -> Result<MathExpr, ParseError> {
        self.eat('\\')?;
        if self.starts_with("frac") {
            self.eat_str("frac")?;
            self.skip_spaces();
            let num = self.parse_nucleus()?;
            self.skip_spaces();
            let den = self.parse_nucleus()?;
            return Ok(MathExpr::Frac(Box::new(num), Box::new(den)));
        }
        if self.starts_with("begin") {
            return self.parse_begin_env();
        }
        let name = self.read_command_name();
        if name.is_empty() {
            let ch = self.bump().ok_or(ParseError)?;
            return Ok(MathExpr::Ord(prettify_tokens(&format!("\\{ch}"))));
        }
        Ok(MathExpr::Ord(prettify_tokens(&format!("\\{name}"))))
    }

    fn read_command_name(&mut self) -> String {
        let mut name = String::new();
        while let Some(ch) = self.peek() {
            if ch.is_ascii_alphabetic() {
                name.push(ch);
                self.i += 1;
            } else {
                break;
            }
        }
        name
    }

    fn read_braced_name(&mut self) -> Result<String, ParseError> {
        self.skip_spaces();
        self.eat('{')?;
        let mut name = String::new();
        while let Some(ch) = self.peek() {
            if ch == '}' {
                break;
            }
            name.push(ch);
            self.i += 1;
        }
        self.eat('}')?;
        Ok(name)
    }

    fn parse_begin_env(&mut self) -> Result<MathExpr, ParseError> {
        self.eat_str("begin")?;
        let env = self.read_braced_name()?;
        let delimited = match env.as_str() {
            "matrix" => false,
            "pmatrix" => true,
            _ => return Err(ParseError),
        };
        let mut rows = Vec::new();
        let mut row = Vec::new();
        loop {
            self.skip_spaces();
            if self.starts_with("\\end") {
                break;
            }
            if self.starts_with("\\\\") {
                self.eat_str("\\\\")?;
                rows.push(std::mem::take(&mut row));
                continue;
            }
            if self.peek() == Some('&') {
                self.bump();
                continue;
            }
            row.push(self.parse_cell()?);
            self.skip_spaces();
            if self.peek() == Some('&') {
                self.bump();
            }
        }
        if !row.is_empty() {
            rows.push(row);
        }
        self.eat_str("\\end")?;
        let end_env = self.read_braced_name()?;
        if end_env != env {
            return Err(ParseError);
        }
        if rows.is_empty() {
            rows.push(vec![MathExpr::Ord(String::new())]);
        }
        Ok(MathExpr::Matrix { delimited, rows })
    }
}

// --- Box layout ------------------------------------------------------------

#[derive(Debug, Clone)]
struct MathBox {
    width: f32,
    /// Distance above the baseline.
    height: f32,
    /// Distance below the baseline.
    depth: f32,
    /// Positions relative to the box baseline (`y` positive = up).
    elements: Vec<RelEl>,
}

#[derive(Debug, Clone)]
enum RelEl {
    Text {
        x: f32,
        y: f32,
        face: FaceRef,
        font_size: f32,
        glyphs: Vec<ShapedGlyph>,
    },
    Rule {
        x: f32,
        y: f32,
        width: f32,
        thickness: f32,
    },
}

impl RelEl {
    fn offset(self, dx: f32, dy: f32) -> Self {
        match self {
            Self::Text {
                x,
                y,
                face,
                font_size,
                glyphs,
            } => Self::Text {
                x: x + dx,
                y: y + dy,
                face,
                font_size,
                glyphs,
            },
            Self::Rule {
                x,
                y,
                width,
                thickness,
            } => Self::Rule {
                x: x + dx,
                y: y + dy,
                width,
                thickness,
            },
        }
    }

    fn into_laid(self, top: f32) -> LaidMathEl {
        match self {
            Self::Text {
                x,
                y,
                face,
                font_size,
                glyphs,
            } => LaidMathEl::Text {
                x,
                y: top - y,
                face,
                font_size,
                glyphs,
            },
            Self::Rule {
                x,
                y,
                width,
                thickness,
            } => LaidMathEl::Rule {
                x,
                y: top - y,
                width,
                thickness,
            },
        }
    }
}

fn append_box(dst: &mut Vec<RelEl>, src: MathBox, dx: f32, dy: f32) {
    dst.extend(src.elements.into_iter().map(|el| el.offset(dx, dy)));
}

fn shift_to_top_origin(math: MathBox) -> Vec<LaidMathEl> {
    let top = math.height;
    math.elements
        .into_iter()
        .map(|el| el.into_laid(top))
        .collect()
}

fn layout_expr(
    expr: &MathExpr,
    fonts: &FontBag,
    face: FaceRef,
    font_size: f32,
    glyph_sets: &mut GlyphSets,
) -> Result<MathBox, WeaveError> {
    match expr {
        MathExpr::Ord(text) => layout_ord(text, fonts, face, font_size, glyph_sets),
        MathExpr::Row(items) => layout_row(items, fonts, face, font_size, glyph_sets),
        MathExpr::Frac(num, den) => layout_frac(num, den, fonts, face, font_size, glyph_sets),
        MathExpr::Scripts { base, sup, sub } => layout_scripts(
            base,
            sup.as_deref(),
            sub.as_deref(),
            fonts,
            face,
            font_size,
            glyph_sets,
        ),
        MathExpr::Matrix { delimited, rows } => {
            layout_matrix(*delimited, rows, fonts, face, font_size, glyph_sets)
        }
    }
}

fn layout_opt(
    expr: Option<&MathExpr>,
    fonts: &FontBag,
    face: FaceRef,
    font_size: f32,
    glyph_sets: &mut GlyphSets,
) -> Result<Option<MathBox>, WeaveError> {
    expr.map(|e| layout_expr(e, fonts, face, font_size, glyph_sets))
        .transpose()
}

fn layout_ord(
    text: &str,
    fonts: &FontBag,
    face: FaceRef,
    font_size: f32,
    glyph_sets: &mut GlyphSets,
) -> Result<MathBox, WeaveError> {
    let text = if text.is_empty() { "\u{00A0}" } else { text };
    let glyphs = shape_text(fonts, face, text, font_size)?;
    let set = glyph_sets.entry(face).or_default();
    collect_glyph_set(fonts, face, text, set);
    note_shaped_glyphs(&glyphs, set);
    Ok(MathBox {
        width: shaped_width(&glyphs),
        height: font_size * 0.75,
        depth: font_size * 0.25,
        elements: vec![RelEl::Text {
            x: 0.0,
            y: 0.0,
            face,
            font_size,
            glyphs,
        }],
    })
}

fn layout_row(
    items: &[MathExpr],
    fonts: &FontBag,
    face: FaceRef,
    font_size: f32,
    glyph_sets: &mut GlyphSets,
) -> Result<MathBox, WeaveError> {
    if items.is_empty() {
        return layout_ord("", fonts, face, font_size, glyph_sets);
    }
    let mut x = 0.0;
    let mut height = 0.0_f32;
    let mut depth = 0.0_f32;
    let mut elements = Vec::new();
    let thin = font_size * 0.05;
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            x += thin;
        }
        let box_ = layout_expr(item, fonts, face, font_size, glyph_sets)?;
        height = height.max(box_.height);
        depth = depth.max(box_.depth);
        let w = box_.width;
        append_box(&mut elements, box_, x, 0.0);
        x += w;
    }
    Ok(MathBox {
        width: x,
        height,
        depth,
        elements,
    })
}

fn layout_frac(
    num: &MathExpr,
    den: &MathExpr,
    fonts: &FontBag,
    face: FaceRef,
    font_size: f32,
    glyph_sets: &mut GlyphSets,
) -> Result<MathBox, WeaveError> {
    let script = font_size * 0.9;
    let num_b = layout_expr(num, fonts, face, script, glyph_sets)?;
    let den_b = layout_expr(den, fonts, face, script, glyph_sets)?;
    let thickness = (font_size * 0.045).max(0.5);
    let gap = font_size * 0.12;
    let axis = font_size * 0.25;
    let pad = font_size * 0.12;
    let width = num_b.width.max(den_b.width) + 2.0 * pad;
    let num_x = (width - num_b.width) / 2.0;
    let den_x = (width - den_b.width) / 2.0;
    let num_baseline = axis + thickness / 2.0 + gap + num_b.depth;
    let den_baseline = axis - thickness / 2.0 - gap - den_b.height;
    let height = num_baseline + num_b.height;
    let depth = -den_baseline + den_b.depth;
    let mut elements = Vec::new();
    append_box(&mut elements, num_b, num_x, num_baseline);
    elements.push(RelEl::Rule {
        x: pad * 0.25,
        y: axis,
        width: width - pad * 0.5,
        thickness,
    });
    append_box(&mut elements, den_b, den_x, den_baseline);
    Ok(MathBox {
        width,
        height,
        depth,
        elements,
    })
}

fn layout_scripts(
    base: &MathExpr,
    sup: Option<&MathExpr>,
    sub: Option<&MathExpr>,
    fonts: &FontBag,
    face: FaceRef,
    font_size: f32,
    glyph_sets: &mut GlyphSets,
) -> Result<MathBox, WeaveError> {
    let base_b = layout_expr(base, fonts, face, font_size, glyph_sets)?;
    let script_size = font_size * 0.7;
    let sup_b = layout_opt(sup, fonts, face, script_size, glyph_sets)?;
    let sub_b = layout_opt(sub, fonts, face, script_size, glyph_sets)?;
    let script_w = sup_b
        .as_ref()
        .map(|b| b.width)
        .unwrap_or(0.0)
        .max(sub_b.as_ref().map(|b| b.width).unwrap_or(0.0));
    let width = base_b.width + script_w;
    let base_w = base_b.width;
    let mut height = base_b.height;
    let mut depth = base_b.depth;
    let mut elements = base_b.elements;
    if let Some(sup_b) = sup_b {
        let y = font_size * 0.45;
        height = height.max(y + sup_b.height);
        append_box(&mut elements, sup_b, base_w, y);
    }
    if let Some(sub_b) = sub_b {
        let y = -font_size * 0.2;
        depth = depth.max(-y + sub_b.depth);
        append_box(&mut elements, sub_b, base_w, y);
    }
    Ok(MathBox {
        width,
        height,
        depth,
        elements,
    })
}

fn layout_matrix(
    delimited: bool,
    rows: &[Vec<MathExpr>],
    fonts: &FontBag,
    face: FaceRef,
    font_size: f32,
    glyph_sets: &mut GlyphSets,
) -> Result<MathBox, WeaveError> {
    let cols = rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
    let cell_size = font_size * 0.95;
    let mut cells: Vec<Vec<MathBox>> = Vec::with_capacity(rows.len());
    for row in rows {
        let mut laid = Vec::with_capacity(cols);
        for c in 0..cols {
            let expr = row
                .get(c)
                .cloned()
                .unwrap_or_else(|| MathExpr::Ord(String::new()));
            laid.push(layout_expr(&expr, fonts, face, cell_size, glyph_sets)?);
        }
        cells.push(laid);
    }

    let mut col_w = vec![0.0_f32; cols];
    let mut row_h = vec![0.0_f32; rows.len()];
    let mut row_d = vec![0.0_f32; rows.len()];
    for (r, row) in cells.iter().enumerate() {
        for (c, cell) in row.iter().enumerate() {
            col_w[c] = col_w[c].max(cell.width);
            row_h[r] = row_h[r].max(cell.height);
            row_d[r] = row_d[r].max(cell.depth);
        }
    }

    let col_gap = font_size * 0.55;
    let row_gap = font_size * 0.25;
    let pad = font_size * 0.2;
    let inner_w = col_w.iter().sum::<f32>() + col_gap * cols.saturating_sub(1) as f32;
    let row_totals: Vec<f32> = row_h.iter().zip(&row_d).map(|(h, d)| h + d).collect();
    let inner_h = row_totals.iter().sum::<f32>() + row_gap * rows.len().saturating_sub(1) as f32;
    let delim_w = if delimited { font_size * 0.45 } else { 0.0 };
    let width = inner_w + 2.0 * pad + 2.0 * delim_w;
    let height = inner_h / 2.0 + font_size * 0.1;
    let depth = inner_h / 2.0 + font_size * 0.1;
    let mut elements = Vec::new();

    if delimited {
        let left = layout_ord("(", fonts, face, font_size * 1.35, glyph_sets)?;
        let right = layout_ord(")", fonts, face, font_size * 1.35, glyph_sets)?;
        let right_x = width - right.width;
        append_box(&mut elements, left, 0.0, 0.0);
        append_box(&mut elements, right, right_x, 0.0);
    }

    let origin_x = delim_w + pad;
    let mut y_top = height - pad * 0.5;
    for (r, row) in cells.into_iter().enumerate() {
        let baseline = y_top - row_h[r];
        let mut x = origin_x;
        for (c, cell) in row.into_iter().enumerate() {
            let cell_x = x + (col_w[c] - cell.width) / 2.0;
            let w = col_w[c];
            append_box(&mut elements, cell, cell_x, baseline);
            x += w + col_gap;
        }
        y_top -= row_totals[r] + row_gap;
    }
    Ok(MathBox {
        width,
        height,
        depth,
        elements,
    })
}

#[cfg(test)]
mod parse_tests {
    use super::{MathExpr, parse_math};

    #[test]
    fn parses_frac_and_scripts() {
        let e = parse_math(r"\frac{a^{10}}{b_{ij}}").expect("parse");
        match e {
            MathExpr::Frac(num, den) => {
                assert!(matches!(*num, MathExpr::Scripts { .. }));
                assert!(matches!(*den, MathExpr::Scripts { .. }));
            }
            other => panic!("expected frac, got {other:?}"),
        }
    }

    #[test]
    fn parses_pmatrix() {
        let e = parse_math(r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}").expect("parse");
        match e {
            MathExpr::Matrix { delimited, rows } => {
                assert!(delimited);
                assert_eq!(rows.len(), 2);
                assert_eq!(rows[0].len(), 2);
            }
            other => panic!("expected matrix, got {other:?}"),
        }
    }
}
