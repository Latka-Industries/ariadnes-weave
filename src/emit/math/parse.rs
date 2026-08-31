//! Math AST and a small LaTeX subset parser.

use super::prettify::prettify_tokens;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum MathExpr {
    Ord(String),
    Row(Vec<MathExpr>),
    Frac(Box<MathExpr>, Box<MathExpr>),
    Scripts {
        base: Box<MathExpr>,
        sup: Option<Box<MathExpr>>,
        sub: Option<Box<MathExpr>>,
    },
    /// Upright / roman wrapper (`\mathrm{…}`).
    MathRm(Box<MathExpr>),
    /// Square root (`\sqrt{…}`) with vinculum over the radicand.
    Sqrt(Box<MathExpr>),
    /// Math accent over a nucleus (`\bar{x}`; THI-385 / jimis).
    Accent {
        kind: MathAccent,
        inner: Box<MathExpr>,
    },
    Matrix {
        fence: MatrixFence,
        rows: Vec<Vec<MathExpr>>,
    },
}

/// Accents we layout (dogfood-driven; not a TeX accent wishlist).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MathAccent {
    /// `\bar{…}` — short rule over the nucleus (jimis `\bar{x}_w`).
    Bar,
}

/// Matrix delimiters: none, `pmatrix` (knob style), or `bmatrix` (always square).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MatrixFence {
    None,
    /// `\begin{pmatrix}` — fence shape from `[paren].style`.
    Paren,
    /// `\begin{bmatrix}` — square brackets.
    Bracket,
}

impl MatrixFence {
    pub(super) fn is_delimited(self) -> bool {
        !matches!(self, Self::None)
    }

    /// Paint style for delimited fences (`pmatrix` follows knobs; `bmatrix` is square).
    pub(super) fn paint_style(
        self,
        paren: &crate::knobs::MathParenKnobs,
    ) -> Option<crate::knobs::MathParenStyle> {
        match self {
            Self::None => None,
            Self::Paren => Some(paren.style),
            Self::Bracket => Some(crate::knobs::MathParenStyle::Square),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum ScriptKind {
    Sup,
    Sub,
}

#[derive(Debug)]
pub(super) struct ParseError;

pub(super) fn parse_math(input: &str) -> Result<MathExpr, ParseError> {
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
        MathExpr::MathRm(inner) => MathExpr::MathRm(Box::new(flatten(*inner))),
        MathExpr::Sqrt(inner) => MathExpr::Sqrt(Box::new(flatten(*inner))),
        MathExpr::Accent { kind, inner } => MathExpr::Accent {
            kind,
            inner: Box::new(flatten(*inner)),
        },
        MathExpr::Matrix { fence, rows } => MathExpr::Matrix {
            fence,
            rows: rows
                .into_iter()
                .map(|r| r.into_iter().map(flatten).collect())
                .collect(),
        },
        MathExpr::Ord(text) => MathExpr::Ord(text),
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

fn ord_from_command(name: &str) -> MathExpr {
    MathExpr::Ord(prettify_tokens(&format!("\\{name}")))
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
            None | Some('}' | '&') => true,
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
        if self.starts_with("bar") {
            self.eat_str("bar")?;
            self.skip_spaces();
            let inner = self.parse_nucleus()?;
            return Ok(MathExpr::Accent {
                kind: MathAccent::Bar,
                inner: Box::new(inner),
            });
        }
        if self.starts_with("mathrm") {
            self.eat_str("mathrm")?;
            self.skip_spaces();
            let inner = self.parse_nucleus()?;
            return Ok(MathExpr::MathRm(Box::new(inner)));
        }
        if self.starts_with("sqrt") {
            self.eat_str("sqrt")?;
            self.skip_spaces();
            let inner = self.parse_nucleus()?;
            return Ok(MathExpr::Sqrt(Box::new(inner)));
        }
        if self.starts_with("begin") {
            return self.parse_begin_env();
        }
        let name = self.read_command_name();
        if name.is_empty() {
            let ch = self.bump().ok_or(ParseError)?;
            return Ok(ord_from_command(&ch.to_string()));
        }
        Ok(ord_from_command(&name))
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
        let fence = match env.as_str() {
            "matrix" => MatrixFence::None,
            "pmatrix" => MatrixFence::Paren,
            "bmatrix" => MatrixFence::Bracket,
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
        Ok(MathExpr::Matrix { fence, rows })
    }
}

#[cfg(test)]
mod parse_tests {
    use super::{MathAccent, MathExpr, parse_math};

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
            MathExpr::Matrix { fence, rows } => {
                assert_eq!(fence, super::MatrixFence::Paren);
                assert_eq!(rows.len(), 2);
                assert_eq!(rows[0].len(), 2);
            }
            other => panic!("expected matrix, got {other:?}"),
        }
    }

    #[test]
    fn parses_bmatrix() {
        let e = parse_math(r"\begin{bmatrix} 1 & 2 \\ 3 & 4 \end{bmatrix}").expect("parse");
        match e {
            MathExpr::Matrix { fence, .. } => {
                assert_eq!(fence, super::MatrixFence::Bracket);
            }
            other => panic!("expected matrix, got {other:?}"),
        }
    }

    #[test]
    fn parses_int_scripts() {
        let e = parse_math(r"\int_{0}^{1}").expect("parse");
        match e {
            MathExpr::Scripts { base, sup, sub } => {
                assert_eq!(*base, MathExpr::Ord("∫".into()));
                assert!(sup.is_some() && sub.is_some());
            }
            other => panic!("expected scripts, got {other:?}"),
        }
    }

    #[test]
    fn parses_mathrm() {
        let e = parse_math(r"\mathrm{after}").expect("parse");
        match e {
            MathExpr::MathRm(inner) => match *inner {
                MathExpr::Row(items) => assert_eq!(items.len(), 5),
                MathExpr::Ord(s) => assert_eq!(s, "after"),
                other => panic!("unexpected mathrm inner: {other:?}"),
            },
            other => panic!("expected mathrm, got {other:?}"),
        }
    }

    #[test]
    fn parses_partial_row() {
        let e = parse_math(r"\partial_{t} \rho = \Phi(\chi)").expect("parse");
        match &e {
            MathExpr::Row(items) => {
                assert!(matches!(&items[0], MathExpr::Scripts { .. }), "{items:?}");
            }
            MathExpr::Scripts { .. } => {}
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn parses_partial_sub() {
        let e = parse_math(r"\partial_{t}").expect("parse");
        match e {
            MathExpr::Scripts { base, sub, .. } => {
                assert_eq!(*base, MathExpr::Ord("∂".into()));
                assert_eq!(sub.as_deref(), Some(&MathExpr::Ord("t".into())));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn parses_sqrt() {
        let e = parse_math(r"\sqrt{b^{2}-4ac}").expect("parse");
        match e {
            MathExpr::Sqrt(inner) => assert!(matches!(*inner, MathExpr::Row(_))),
            other => panic!("expected sqrt, got {other:?}"),
        }
    }

    #[test]
    fn parses_bar_then_subscript() {
        // jimis: `\bar{x}_w` — accent is the nucleus; `_w` attaches after.
        let e = parse_math(r"\bar{x}_w").expect("parse");
        match e {
            MathExpr::Scripts { base, sub, sup } => {
                assert!(sup.is_none());
                assert_eq!(sub.as_deref(), Some(&MathExpr::Ord("w".into())));
                match *base {
                    MathExpr::Accent {
                        kind: MathAccent::Bar,
                        inner,
                    } => assert_eq!(*inner, MathExpr::Ord("x".into())),
                    other => panic!("expected bar accent, got {other:?}"),
                }
            }
            other => panic!("expected scripts around bar, got {other:?}"),
        }
    }

    #[test]
    fn parses_jimis_weighted_mean() {
        let e = parse_math(r"\bar{x}_w = \frac{\sum m_i x_i}{\sum m_i}").expect("parse");
        match e {
            MathExpr::Row(items) => {
                assert!(matches!(&items[0], MathExpr::Scripts { .. }), "{items:?}");
                assert!(items.iter().any(|it| matches!(it, MathExpr::Frac(_, _))));
            }
            other => panic!("expected row, got {other:?}"),
        }
    }
}
