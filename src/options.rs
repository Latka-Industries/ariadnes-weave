//! Emit-time options (fonts, layout knobs, later: more policy).

use std::collections::BTreeMap;

use crate::knobs::LayoutKnobs;

/// How weave resolves font bytes during emit.
///
/// CI and SHA fixtures use [`FontResolveMode::BundledOnly`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FontResolveMode {
    /// Only sealed/`include_bytes!` faces plus explicitly [`EmitOptions::pinned_faces`].
    ///
    /// Pinned faces are host-supplied bytes (GUI/OS loader), not an automatic
    /// system font scan — so fixtures stay deterministic when pins are fixed.
    /// Unknown [`crate::TextRun::face`] ids error at emit time.
    #[default]
    BundledOnly,
    /// Try explicit pins, then the OS font stack (`--features os-fonts`), then
    /// fall back to sealed Liberation for the run's inline style.
    ///
    /// Without the `os-fonts` Cargo feature, emit returns a font error if this
    /// mode is selected.
    OsWithFallback,
}

/// Options for [`crate::emit_pdf_with`].
///
/// [`crate::emit_pdf`] is equivalent to `EmitOptions::default()`
/// (`FontResolveMode::BundledOnly`, bundled [`LayoutKnobs`], no pins).
#[derive(Debug, Clone, PartialEq)]
pub struct EmitOptions {
    /// Font resolution policy.
    pub fonts: FontResolveMode,
    /// Host-pinned TrueType faces keyed by stable id (sorted for determinism).
    ///
    /// Reference from [`crate::TextRun::face`]. Under
    /// [`FontResolveMode::BundledOnly`], unknown ids error at emit time.
    pub pinned_faces: BTreeMap<String, Vec<u8>>,
    /// Optical paddings / gaps / scales (`defaults/*.toml`).
    pub layout: LayoutKnobs,
}

impl Default for EmitOptions {
    fn default() -> Self {
        Self::bundled_only()
    }
}

impl EmitOptions {
    /// Deterministic sealed-pack emit with no pins (same as [`Default`]).
    #[must_use]
    pub fn bundled_only() -> Self {
        Self {
            fonts: FontResolveMode::BundledOnly,
            pinned_faces: BTreeMap::new(),
            layout: LayoutKnobs::bundled(),
        }
    }

    /// OS lookup with Liberation fallback (requires `--features os-fonts`).
    #[must_use]
    pub fn os_with_fallback() -> Self {
        Self {
            fonts: FontResolveMode::OsWithFallback,
            pinned_faces: BTreeMap::new(),
            layout: LayoutKnobs::bundled(),
        }
    }

    /// Pin a TrueType face under `id` (builder-style).
    #[must_use]
    pub fn with_pinned_face(mut self, id: impl Into<String>, bytes: Vec<u8>) -> Self {
        self.pinned_faces.insert(id.into(), bytes);
        self
    }

    /// Replace layout knobs (builder-style).
    #[must_use]
    pub fn with_layout(mut self, layout: LayoutKnobs) -> Self {
        self.layout = layout;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_bundled_only() {
        assert_eq!(EmitOptions::default().fonts, FontResolveMode::BundledOnly);
        assert!(EmitOptions::default().pinned_faces.is_empty());
        assert_eq!(EmitOptions::default().layout, LayoutKnobs::bundled());
    }
}
