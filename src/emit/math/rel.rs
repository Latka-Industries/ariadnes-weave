//! Relative math elements + box helpers shared by layout and geo.

use crate::font::{
    FaceId, FaceRef, FontBag, ShapedGlyph, collect_glyph_set, note_shaped_glyphs, shape_text,
    shaped_width,
};
use crate::knobs::{MathArrowKnobs, MathInfinityKnobs, MathKnobs, MathParenKnobs};
use ttf_parser::Face as TtfFace;

use super::super::types::{GlyphSets, LaidMathEl, MathSymKind};
use crate::error::WeaveError;

pub(super) struct MathCtx<'a> {
    pub(super) fonts: &'a FontBag,
    pub(super) face: FaceRef,
    pub(super) knobs: &'a MathKnobs,
    pub(super) glyph_sets: &'a mut GlyphSets,
    /// Display math (under/over limits for big ops); inline keeps side scripts.
    pub(super) display: bool,
}

impl MathCtx<'_> {
    /// Math axis above the baseline (TeX-ish); fraction bars and big ops share this.
    pub(super) fn axis(&self, font_size: f32) -> f32 {
        font_size * self.knobs.metrics.axis_factor
    }

    /// Convert math units (18 mu = 1 em ≈ `font_size`) to PDF points.
    pub(super) fn mu(&self, n: f32, font_size: f32) -> f32 {
        font_size * n / self.knobs.metrics.mu_per_em
    }

    pub(super) fn letter_ink(&self, font_size: f32) -> InkBox {
        letter_ink_ref(self, font_size)
    }

    pub(super) fn with_upright_face<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let upright = upright_face(self.face);
        let saved = self.face;
        self.face = upright;
        let out = f(self);
        self.face = saved;
        out
    }
}

impl MathArrowKnobs {
    pub(super) fn stroke_thickness(&self, font_size: f32) -> f32 {
        clamped_stroke(
            font_size,
            self.thickness_factor,
            self.thickness_min,
            self.thickness_max,
        )
    }
}

impl MathInfinityKnobs {
    pub(super) fn clamp_scale(&self, scale: f32) -> f32 {
        scale.clamp(self.scale_min, self.scale_max)
    }
}

impl MathParenKnobs {
    pub(super) fn delim_width(&self, half_h: f32) -> f32 {
        (half_h * self.width_factor).clamp(self.width_min, self.width_max)
    }

    pub(super) fn stroke_thickness(&self, font_size: f32) -> f32 {
        clamped_stroke(
            font_size,
            self.thickness_factor,
            self.thickness_min,
            self.thickness_max,
        )
    }
}

pub(super) fn clamped_stroke(font_size: f32, factor: f32, min: f32, max: f32) -> f32 {
    (font_size * factor).clamp(min, max)
}

pub(super) fn rule_thickness(font_size: f32, factor: f32, min: f32) -> f32 {
    (font_size * factor).max(min)
}

#[derive(Debug, Clone)]
pub(super) struct MathBox {
    pub(super) width: f32,
    /// Distance above the baseline.
    pub(super) height: f32,
    /// Distance below the baseline.
    pub(super) depth: f32,
    /// Positions relative to the box baseline (`y` positive = up).
    pub(super) elements: Vec<RelEl>,
}

/// Drawn element; coords relative to the math box baseline (`y`/`axis` positive = up).
#[derive(Debug, Clone)]
pub(super) enum RelEl {
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
    /// Stroked paren; `axis` is the math-axis offset from the baseline.
    Paren {
        x: f32,
        axis: f32,
        half_h: f32,
        width: f32,
        thickness: f32,
        left: bool,
    },
    /// Geometric arrow; `y` is the shaft midline relative to the baseline.
    Arrow {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        thickness: f32,
        left: bool,
    },
    /// Geometric integral; `axis` is the math-axis offset from the baseline.
    Integral {
        x: f32,
        axis: f32,
        half_h: f32,
        width: f32,
        thickness: f32,
        contour: bool,
    },
    /// Geometric radical; vinculum is a separate [`Self::Rule`].
    Radical {
        x: f32,
        y: f32,
        height: f32,
        width: f32,
        thickness: f32,
    },
    /// Geometric symbol Liberation lacks; `y` is the midline from baseline.
    Sym {
        kind: MathSymKind,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        thickness: f32,
    },
}

impl RelEl {
    fn offset(self, dx: f32, dy: f32) -> Self {
        self.map_xy(|x, y| (x + dx, y + dy))
    }

    /// Remap horizontal + vertical placement (`y` or `axis`).
    fn map_xy(self, f: impl FnOnce(f32, f32) -> (f32, f32)) -> Self {
        // Macro keeps this under clippy::too_many_lines once rustfmt expands arms.
        macro_rules! go {
            ($($pat:pat => ($x:ident, $y:ident) $rebuild:expr),+ $(,)?) => {
                match self {
                    $($pat => {
                        let ($x, $y) = f($x, $y);
                        $rebuild
                    }),+
                }
            };
        }
        go! {
            Self::Text { x, y, face, font_size, glyphs } => (x, y)
                Self::Text { x, y, face, font_size, glyphs },
            Self::Rule { x, y, width, thickness } => (x, y)
                Self::Rule { x, y, width, thickness },
            Self::Paren { x, axis, half_h, width, thickness, left } => (x, axis)
                Self::Paren { x, axis, half_h, width, thickness, left },
            Self::Arrow { x, y, width, height, thickness, left } => (x, y)
                Self::Arrow { x, y, width, height, thickness, left },
            Self::Integral { x, axis, half_h, width, thickness, contour } => (x, axis)
                Self::Integral { x, axis, half_h, width, thickness, contour },
            Self::Radical { x, y, height, width, thickness } => (x, y)
                Self::Radical { x, y, height, width, thickness },
            Self::Sym { kind, x, y, width, height, thickness } => (x, y)
                Self::Sym { kind, x, y, width, height, thickness },
        }
    }

    fn into_laid(self, top: f32) -> LaidMathEl {
        let y = |v| top - v;
        macro_rules! go {
            ($($pat:pat => $out:expr),+ $(,)?) => {
                match self { $($pat => $out),+ }
            };
        }
        go! {
            Self::Text { x, y: yy, face, font_size, glyphs } => LaidMathEl::Text {
                x, y: y(yy), face, font_size, glyphs,
            },
            Self::Rule { x, y: yy, width, thickness } => LaidMathEl::Rule {
                x, y: y(yy), width, thickness,
            },
            Self::Paren { x, axis, half_h, width, thickness, left } => LaidMathEl::Paren {
                x, axis_y: y(axis), half_h, width, thickness, left,
            },
            Self::Arrow { x, y: yy, width, height, thickness, left } => LaidMathEl::Arrow {
                x, y: y(yy), width, height, thickness, left,
            },
            Self::Integral { x, axis, half_h, width, thickness, contour } => LaidMathEl::Integral {
                x, axis_y: y(axis), half_h, width, thickness, contour,
            },
            Self::Radical { x, y: yy, height, width, thickness } => LaidMathEl::Radical {
                x, y: y(yy), height, width, thickness,
            },
            Self::Sym { kind, x, y: yy, width, height, thickness } => LaidMathEl::Sym {
                kind, x, y: y(yy), width, height, thickness,
            },
        }
    }
}

pub(super) fn offset_elements(elements: Vec<RelEl>, dx: f32, dy: f32) -> Vec<RelEl> {
    elements.into_iter().map(|el| el.offset(dx, dy)).collect()
}

pub(super) fn h_center(container: f32, content: f32) -> f32 {
    (container - content) / 2.0
}

pub(super) fn box_from_midline(
    mid_y: f32,
    half_extent: f32,
    width: f32,
    elements: Vec<RelEl>,
) -> MathBox {
    MathBox {
        width,
        height: mid_y + half_extent,
        depth: (half_extent - mid_y).max(0.0),
        elements,
    }
}

pub(super) fn append_box(dst: &mut Vec<RelEl>, src: MathBox, dx: f32, dy: f32) {
    dst.extend(offset_elements(src.elements, dx, dy));
}

pub(super) fn shift_to_top_origin(math: MathBox) -> Vec<LaidMathEl> {
    let top = math.height;
    math.elements
        .into_iter()
        .map(|el| el.into_laid(top))
        .collect()
}

pub(super) fn shift_box_vert(box_: MathBox, dy: f32) -> MathBox {
    MathBox {
        width: box_.width,
        height: (box_.height + dy).max(0.0),
        depth: (box_.depth - dy).max(0.0),
        elements: offset_elements(box_.elements, 0.0, dy),
    }
}

pub(super) fn pad_box_h(box_: MathBox, left: f32, right: f32) -> MathBox {
    MathBox {
        width: box_.width + left + right,
        height: box_.height,
        depth: box_.depth,
        elements: offset_elements(box_.elements, left, 0.0),
    }
}

#[derive(Clone, Copy)]
pub(super) struct InkBox {
    pub(super) above: f32,
    pub(super) below: f32,
}

impl InkBox {
    pub(super) fn span(self) -> f32 {
        self.above + self.below
    }

    pub(super) fn center(self) -> f32 {
        (self.above - self.below) / 2.0
    }
}

pub(super) fn char_ink(fonts: &FontBag, face: FaceRef, ch: char, font_size: f32) -> Option<InkBox> {
    let data = fonts.ttf_bytes(face).ok()?;
    let ttf = TtfFace::parse(data, 0).ok()?;
    let gid = ttf.glyph_index(ch)?;
    let bb = ttf.glyph_bounding_box(gid)?;
    let scale = font_size / f32::from(ttf.units_per_em());
    Some(InkBox {
        above: f32::from(bb.y_max) * scale,
        below: (-f32::from(bb.y_min) * scale).max(0.0),
    })
}

fn letter_ink_ref(ctx: &MathCtx, font_size: f32) -> InkBox {
    char_ink(ctx.fonts, ctx.face, 'α', font_size)
        .or_else(|| char_ink(ctx.fonts, ctx.face, 'x', font_size))
        .unwrap_or(InkBox {
            above: font_size * ctx.knobs.metrics.fallback_ink_above_factor,
            below: 0.0,
        })
}

pub(super) fn upright_face(face: FaceRef) -> FaceRef {
    match face {
        FaceRef::Bundled(FaceId::Math) => FaceRef::Bundled(FaceId::Math),
        FaceRef::Bundled(
            FaceId::SerifRegular
            | FaceId::SerifBold
            | FaceId::SerifItalic
            | FaceId::SerifBoldItalic,
        ) => FaceRef::Bundled(FaceId::SerifRegular),
        FaceRef::Bundled(_) => FaceRef::Bundled(FaceId::SansRegular),
        other @ FaceRef::Pinned(_) => other,
    }
}

/// Shape an Ord glyph run (no Bin/Rel axis centering).
pub(super) fn layout_ord_raw(
    text: &str,
    ctx: &mut MathCtx,
    font_size: f32,
) -> Result<MathBox, WeaveError> {
    let text = if text.is_empty() { "\u{00A0}" } else { text };
    let glyphs = shape_text(ctx.fonts, ctx.face, text, font_size)?;
    let set = ctx.glyph_sets.entry(ctx.face).or_default();
    collect_glyph_set(ctx.fonts, ctx.face, text, set);
    note_shaped_glyphs(&glyphs, set);
    Ok(MathBox {
        width: shaped_width(&glyphs),
        height: font_size * ctx.knobs.metrics.notdef_height_factor,
        depth: font_size * ctx.knobs.metrics.notdef_depth_factor,
        elements: vec![RelEl::Text {
            x: 0.0,
            y: 0.0,
            face: ctx.face,
            font_size,
            glyphs,
        }],
    })
}
