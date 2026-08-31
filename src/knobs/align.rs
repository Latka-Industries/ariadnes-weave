//! Figure / caption / in-band alignment enums.

use serde::{Deserialize, Serialize};

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
