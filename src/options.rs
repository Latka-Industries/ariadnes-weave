//! Emit-time options (fonts, later: more policy knobs).

use std::collections::BTreeMap;

/// How weave resolves font bytes during emit.
///
/// CI and SHA fixtures use [`FontResolveMode::BundledOnly`]. OS lookup with
/// sealed-pack fallback is planned under THI-307.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FontResolveMode {
    /// Only sealed/`include_bytes!` faces plus explicitly [`EmitOptions::pinned_faces`].
    ///
    /// Pinned faces are host-supplied bytes (GUI/OS loader), not an automatic
    /// system font scan — so fixtures stay deterministic when pins are fixed.
    #[default]
    BundledOnly,
}

/// Options for [`crate::emit_pdf_with`].
///
/// [`crate::emit_pdf`] is equivalent to `EmitOptions::default()`
/// (`FontResolveMode::BundledOnly`, no pins).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EmitOptions {
    /// Font resolution policy.
    pub fonts: FontResolveMode,
    /// Host-pinned TrueType faces keyed by stable id (sorted for determinism).
    ///
    /// Reference from [`crate::TextRun::face`]. Unknown ids error at emit time.
    pub pinned_faces: BTreeMap<String, Vec<u8>>,
}

impl EmitOptions {
    /// Deterministic sealed-pack emit with no pins (same as [`Default`]).
    #[must_use]
    pub fn bundled_only() -> Self {
        Self {
            fonts: FontResolveMode::BundledOnly,
            pinned_faces: BTreeMap::new(),
        }
    }

    /// Pin a TrueType face under `id` (builder-style).
    #[must_use]
    pub fn with_pinned_face(mut self, id: impl Into<String>, bytes: Vec<u8>) -> Self {
        self.pinned_faces.insert(id.into(), bytes);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_bundled_only() {
        assert_eq!(EmitOptions::default(), EmitOptions::bundled_only());
        assert_eq!(EmitOptions::default().fonts, FontResolveMode::BundledOnly);
        assert!(EmitOptions::default().pinned_faces.is_empty());
    }
}
