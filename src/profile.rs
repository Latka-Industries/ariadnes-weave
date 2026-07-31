//! Versioned print profile metrics (policy, not CSS).

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
        }),
        ("manuscript", 0) => Ok(ProfileMetrics {
            // US Letter, larger margins, double-spaced body (beta-reader-ish).
            page_w: 612.0,
            page_h: 792.0,
            margin: 96.0,
            body_size: 12.0,
            body_leading: 12.0 * 2.0,
            code_size: 10.0,
            serif_body: true,
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
