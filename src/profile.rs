//! Versioned print profile metrics (policy, not CSS).
//!
//! Profiles are **coupled** `name@version` ids that compose three axes:
//! mode (type + pagination) · page size · margins. See
//! [`docs/decisions/D-print-profile-axes.md`](../../docs/decisions/D-print-profile-axes.md)
//! and [`docs/profiles.md`](../../docs/profiles.md).
//!
//! Literary unfolding (`manuscript@0`):
//! [`docs/decisions/D-literary-unfolding.md`](../../docs/decisions/D-literary-unfolding.md).

use crate::error::WeaveError;
use crate::ir::PrintProfileId;

/// Layout metrics resolved from a [`PrintProfileId`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProfileMetrics {
    /// Page width in PDF points.
    pub page_w: f32,
    /// Page height in PDF points.
    pub page_h: f32,
    /// Uniform margin in PDF points.
    pub margin: f32,
    /// Body text size.
    pub body_size: f32,
    /// Body line leading (size × factor baked in).
    pub body_leading: f32,
    /// Code block size.
    pub code_size: f32,
    /// Prefer Liberation Serif for unstyled body runs (`manuscript@0`).
    pub serif_body: bool,
    /// Force a page break before each H1 after the first content on a page.
    pub force_h1_page_break: bool,
    /// Deck/slide page geometry (landscape, larger type).
    pub is_deck: bool,
}

impl ProfileMetrics {
    /// Content width inside margins.
    #[must_use]
    pub fn content_width(self) -> f32 {
        self.page_w - 2.0 * self.margin
    }

    /// Usable vertical span inside margins.
    #[must_use]
    pub fn content_height(self) -> f32 {
        self.page_h - 2.0 * self.margin
    }
}

/// Page geometry axis (PDF points). Orthogonal to [`DocMode`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageSize {
    /// ISO A4 — `595 × 842`.
    A4,
    /// US Letter — `612 × 792`.
    UsLetter,
    /// 16:9 landscape at 72 pt/in — `960 × 540`.
    Widescreen16x9,
}

impl PageSize {
    /// `(width, height)` in PDF points.
    #[must_use]
    pub const fn dimensions(self) -> (f32, f32) {
        match self {
            Self::A4 => (595.0, 842.0),
            Self::UsLetter => (612.0, 792.0),
            Self::Widescreen16x9 => (960.0, 540.0),
        }
    }
}

/// Document mode axis: type rhythm + pagination policy (not page size).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DocMode {
    /// Technical prose — Sans, compact leading, no forced H1 breaks.
    Print,
    /// Literary / beta-reader — Serif, double-spaced, H1 page breaks.
    Manuscript,
    /// Slide deck — large type, one slide per page.
    Deck,
}

impl DocMode {
    /// Default uniform margin for this mode (margin axis, coupled in MVP ids).
    const fn default_margin(self) -> f32 {
        match self {
            Self::Print => 72.0,
            Self::Manuscript => 96.0,
            Self::Deck => 40.0,
        }
    }

    /// `(body_size, leading_factor, code_size, serif_body, force_h1, is_deck)`.
    const fn type_rhythm(self) -> (f32, f32, f32, bool, bool, bool) {
        match self {
            Self::Print => (11.0, 1.4, 9.0, false, false, false),
            Self::Manuscript => (12.0, 2.0, 10.0, true, true, false),
            Self::Deck => (22.0, 1.35, 16.0, false, false, true),
        }
    }
}

/// Compose metrics from orthogonal axes (used by coupled profile ids).
fn compose(mode: DocMode, page: PageSize, margin: f32) -> ProfileMetrics {
    let (page_w, page_h) = page.dimensions();
    let (body_size, lead_f, code_size, serif_body, force_h1_page_break, is_deck) =
        mode.type_rhythm();
    ProfileMetrics {
        page_w,
        page_h,
        margin,
        body_size,
        body_leading: body_size * lead_f,
        code_size,
        serif_body,
        force_h1_page_break,
        is_deck,
    }
}

fn compose_default(mode: DocMode, page: PageSize) -> ProfileMetrics {
    compose(mode, page, mode.default_margin())
}

/// Resolve metrics for a known profile id.
///
/// # Errors
///
/// Returns [`WeaveError::UnsupportedProfile`] for unknown name/version.
pub fn resolve_metrics(profile: &PrintProfileId) -> Result<ProfileMetrics, WeaveError> {
    match (profile.name.as_str(), profile.version) {
        ("print", 0) => Ok(compose_default(DocMode::Print, PageSize::A4)),
        ("print-letter", 0) => Ok(compose_default(DocMode::Print, PageSize::UsLetter)),
        ("manuscript", 0) => Ok(compose_default(DocMode::Manuscript, PageSize::UsLetter)),
        ("deck", 0) => Ok(compose_default(DocMode::Deck, PageSize::Widescreen16x9)),
        _ => Err(WeaveError::UnsupportedProfile {
            name: profile.name.clone(),
            version: profile.version,
        }),
    }
}

/// Heading size for a profile (shared scale; manuscript bumps body only).
#[must_use]
pub fn heading_size(level: u8, metrics: &ProfileMetrics) -> f32 {
    let base: f32 = match level {
        1 => 18.0,
        2 => 14.0,
        3 => 12.0,
        _ => 11.0,
    };
    // Manuscript keeps display sizes close to print but not smaller than body.
    base.max(metrics.body_size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::PrintProfileId;

    #[test]
    fn manuscript_v0_literary_metrics() {
        let m = resolve_metrics(&PrintProfileId::manuscript_v0()).expect("manuscript@0");
        assert_eq!((m.page_w, m.page_h), PageSize::UsLetter.dimensions());
        assert_eq!(m.margin, 96.0);
        assert_eq!(m.body_size, 12.0);
        assert_eq!(m.body_leading, 24.0);
        assert!(m.serif_body);
        assert!(m.force_h1_page_break);
        assert!(!m.is_deck);
    }

    #[test]
    fn print_and_print_letter_share_mode_differ_by_page() {
        let a4 = resolve_metrics(&PrintProfileId::print_v0()).expect("print@0");
        let letter = resolve_metrics(&PrintProfileId::print_letter_v0()).expect("print-letter@0");
        assert_eq!((a4.page_w, a4.page_h), PageSize::A4.dimensions());
        assert_eq!(
            (letter.page_w, letter.page_h),
            PageSize::UsLetter.dimensions()
        );
        assert_eq!(a4.body_size, letter.body_size);
        assert_eq!(a4.body_leading, letter.body_leading);
        assert_eq!(a4.margin, letter.margin);
        assert_eq!(a4.serif_body, letter.serif_body);
        assert_eq!(a4.force_h1_page_break, letter.force_h1_page_break);
        assert!(!a4.is_deck && !letter.is_deck);
    }

    #[test]
    fn compose_is_orthogonal() {
        let m = compose(DocMode::Print, PageSize::UsLetter, 72.0);
        assert_eq!((m.page_w, m.page_h), PageSize::UsLetter.dimensions());
        assert_eq!(m.body_size, 11.0);
        assert!(!m.force_h1_page_break);
    }
}
