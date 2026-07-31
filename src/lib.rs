//! `ariadnes-weave` — print IR → deterministic PDF for Tessera.
//!
//! See Tessera `docs/print_ir.md` / D21.
//!
//! MVP (THI-289): prose IR types + Helvetica `emit_pdf`. Real pagination,
//! bundled fonts, and profiles land in later issues (THI-294+).

#![deny(missing_docs)]

mod emit;
mod error;
mod ir;

pub use emit::emit_pdf;
pub use error::WeaveError;
pub use ir::{
    BreakHint, InlineStyle, PrintBlock, PrintDocument, PrintMeta, PrintProfileId, TextRun,
};

/// Crate version string (useful in PDF producer / fixture pins).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
