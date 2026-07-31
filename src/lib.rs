//! `ariadnes-weave` — print IR → deterministic PDF for Tessera.
//!
//! See Tessera `docs/print_ir.md` / D21.
//!
//! Helvetica-family emit for prose (+ placeholders for table/figure/math/slide).
//! Bundled TTFs and full pagination policy deepen in later issues.

#![deny(missing_docs)]

mod emit;
mod error;
mod ir;
mod profile;

pub use emit::emit_pdf;
pub use error::WeaveError;
pub use ir::{
    BreakHint, FigurePlacement, InlineStyle, ListItem, PrintBlock, PrintDocument, PrintImage,
    PrintMeta, PrintProfileId, SlideRegionContent, TableRow, TextRun,
};
pub use profile::{ProfileMetrics, resolve_metrics};

/// Crate version string (useful in PDF producer / fixture pins).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
