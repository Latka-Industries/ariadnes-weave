//! `ariadnes-weave` — print IR → deterministic PDF for Tessera.
//!
//! See Tessera `docs/print_ir.md` / D21.
//!
//! Bundled Liberation TTFs with rustybuzz shaping (+ table/figure/math/slide stubs).
//! Richer pagination policy deepens in later issues.

#![deny(missing_docs)]

mod emit;
mod error;
mod font;
mod image_prep;
mod ir;
mod profile;

pub use emit::emit_pdf;
pub use error::WeaveError;
pub use font::FaceId;
pub use ir::{
    BreakHint, FigurePlacement, InlineStyle, ListItem, PrintBlock, PrintDocument, PrintImage,
    PrintMeta, PrintProfileId, SlideRegionContent, TableRow, TextRun,
};
pub use profile::{ProfileMetrics, resolve_metrics};

/// Crate version string (useful in PDF producer / fixture pins).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Sealed CJK fallback pack compiled in (`--features cjk`).
pub const FEATURE_CJK: bool = cfg!(feature = "cjk");

/// Sealed emoji fallback pack compiled in (`--features emoji`).
pub const FEATURE_EMOJI: bool = cfg!(feature = "emoji");

/// Font Awesome Free icon faces compiled in (`--features icons`).
pub const FEATURE_ICONS: bool = cfg!(feature = "icons");
