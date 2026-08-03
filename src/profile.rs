//! Versioned print profile metrics (policy, not CSS).
//!
//! Profile rules: [`docs/profiles.md`](../../docs/profiles.md).
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

/// Resolve metrics for a known profile id.
///
/// # Errors
///
/// Returns [`WeaveError::UnsupportedProfile`] for unknown name/version.
pub fn resolve_metrics(profile: &PrintProfileId) -> Result<ProfileMetrics, WeaveError> {
    match (profile.name.as_str(), profile.version) {
        ("print", 0) => Ok(ProfileMetrics {
            page_w: 595.0, // A4
            page_h: 842.0,
            margin: 72.0,
            body_size: 11.0,
            body_leading: 11.0 * 1.4,
            code_size: 9.0,
            serif_body: false,
            force_h1_page_break: false,
            is_deck: false,
        }),
        ("manuscript", 0) => Ok(ProfileMetrics {
            // US Letter, large margins, double-spaced Liberation Serif body.
            page_w: 612.0,
            page_h: 792.0,
            margin: 96.0,
            body_size: 12.0,
            body_leading: 12.0 * 2.0,
            code_size: 10.0,
            serif_body: true,
            force_h1_page_break: true,
            is_deck: false,
        }),
        ("deck", 0) => Ok(ProfileMetrics {
            // 16:9 widescreen (13.333" × 7.5" at 72 pt/in).
            page_w: 960.0,
            page_h: 540.0,
            margin: 40.0,
            body_size: 22.0,
            body_leading: 22.0 * 1.35,
            code_size: 16.0,
            serif_body: false,
            force_h1_page_break: false,
            is_deck: true,
        }),
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
        assert_eq!((m.page_w, m.page_h), (612.0, 792.0));
        assert_eq!(m.margin, 96.0);
        assert_eq!(m.body_size, 12.0);
        assert_eq!(m.body_leading, 24.0);
        assert!(m.serif_body);
        assert!(m.force_h1_page_break);
        assert!(!m.is_deck);
    }
}
