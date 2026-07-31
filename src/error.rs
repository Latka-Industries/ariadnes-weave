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

    /// Text could not be encoded for a PDF standard font (WinAnsi / ASCII subset).
    #[error("text not encodable with MVP Helvetica path: {0}")]
    UnencodableText(String),
}
