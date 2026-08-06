//! Error types for print IR → PDF.

use thiserror::Error;

/// Errors from [`crate::emit_pdf`].
#[derive(Debug, Error)]
pub enum WeaveError {
    /// Profile id is not known to this crate version.
    #[error("unsupported print profile: {name}@{version}")]
    UnsupportedProfile {
        /// Profile name (e.g. `print`).
        name: String,
        /// Profile version.
        version: u32,
    },

    /// Block kind not implemented in this MVP emit path.
    #[error("unsupported print block: {0}")]
    UnsupportedBlock(&'static str),

    /// Layout `frac` outside `0..=10_000` basis points (`0.0..=1.0`).
    #[error("invalid layout frac: {0} bps (must be 0..=10000)")]
    InvalidLayoutFrac(u16),

    /// [`crate::ir::RuleWidth`] with neither `frac` nor `em`.
    #[error("layout rule width requires frac and/or em")]
    EmptyRuleWidth,

    /// Bundled font parse / shape / embed failure.
    #[error("font error: {0}")]
    Font(String),

    /// Image bytes could not be decoded or are an unsupported format.
    #[error("unsupported or invalid image: {0}")]
    BadImage(String),
}
