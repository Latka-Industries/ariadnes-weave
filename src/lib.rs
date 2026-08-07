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
//! * `os-fonts` — OS font stack via `fontdb` ([`FontResolveMode::OsWithFallback`]).
//! * `cjk` — sealed CJK sans subset (`FaceId::CjkSans`) used as script fallback.
//! * `emoji` — sealed B&W emoji subset (`FaceId::Emoji`) used as script fallback.

#![deny(missing_docs)]

mod emit;
mod error;
mod font;
mod image_prep;
mod ir;
mod knobs;
mod options;
#[cfg(feature = "os-fonts")]
mod os_fonts;
mod profile;
mod resolve_fonts;

pub use emit::{emit_pdf, emit_pdf_with};
pub use error::WeaveError;
pub use font::{FaceId, FaceRef, FontBag};
pub use ir::{
    BreakHint, EmAmount, FigurePlacement, InlineStyle, LayoutOp, ListItem, MeasureFrac, PlaceSkip,
    PrintBlock, PrintDocument, PrintImage, PrintMeta, PrintProfileId, RuleWidth,
    SlideRegionContent, TableRow, TextRun, VspaceAmount,
};
pub use knobs::{
    DeckKnobs, FigureAlign, HexColor, LayoutKnobs, MathKnobs, PageKnobs, ProseCaptionKnobs,
    ProseCiteKnobs, ProseFontCategory, ProseKnobs, ProsePaintCategory, ProseTextKnobs, TableKnobs,
};
pub use options::{EmitOptions, FontResolveMode};
pub use profile::{PageSize, ProfileMetrics, resolve_metrics};

/// Crate version string (useful in PDF producer / fixture pins).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Sealed CJK fallback pack compiled in (`--features cjk`).
pub const FEATURE_CJK: bool = cfg!(feature = "cjk");

/// Sealed emoji fallback pack compiled in (`--features emoji`).
pub const FEATURE_EMOJI: bool = cfg!(feature = "emoji");

/// Font Awesome Free icon faces compiled in (`--features icons`).
pub const FEATURE_ICONS: bool = cfg!(feature = "icons");

/// OS font database compiled in (`--features os-fonts`).
pub const FEATURE_OS_FONTS: bool = cfg!(feature = "os-fonts");
