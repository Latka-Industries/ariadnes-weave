//! `ariadnes-weave` — print IR → deterministic PDF for Tessera.
//!
//! See Tessera `docs/print_ir.md` / D21.
//!
//! # Emit
//!
//! * [`emit_pdf`] — sealed Liberation faces ([`FontResolveMode::BundledOnly`]).
//! * [`emit_pdf_with`] — same, plus optional [`EmitOptions::pinned_faces`] for
//!   host-supplied TTFs selected via [`TextRun::face`].
//!
//! # Features
//!
//! * `icons` — Font Awesome Free Solid / Regular / Brands (`FaceId::Icon*`).
//! * `cjk` / `emoji` — stubs for future sealed packs (not bundled yet).
//!
//! OS fontconfig lookup is tracked separately (THI-311); pins cover the
//! “GUI already resolved the bytes” path.

#![deny(missing_docs)]

mod emit;
mod error;
mod font;
mod image_prep;
mod ir;
mod options;
mod profile;

pub use emit::{emit_pdf, emit_pdf_with};
pub use error::WeaveError;
pub use font::{FaceId, FaceRef, FontBag};
pub use ir::{
    BreakHint, FigurePlacement, InlineStyle, ListItem, PrintBlock, PrintDocument, PrintImage,
    PrintMeta, PrintProfileId, SlideRegionContent, TableRow, TextRun,
};
pub use options::{EmitOptions, FontResolveMode};
pub use profile::{ProfileMetrics, resolve_metrics};

/// Crate version string (useful in PDF producer / fixture pins).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Sealed CJK fallback pack compiled in (`--features cjk`).
pub const FEATURE_CJK: bool = cfg!(feature = "cjk");

/// Sealed emoji fallback pack compiled in (`--features emoji`).
pub const FEATURE_EMOJI: bool = cfg!(feature = "emoji");

/// Font Awesome Free icon faces compiled in (`--features icons`).
pub const FEATURE_ICONS: bool = cfg!(feature = "icons");
