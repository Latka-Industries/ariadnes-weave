//! Emit-time options (fonts, later: more policy knobs).

/// How weave resolves font bytes during emit.
///
/// CI and SHA fixtures use [`FontResolveMode::BundledOnly`]. OS lookup with
/// sealed-pack fallback is planned under THI-307.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FontResolveMode {
    /// Only sealed/`include_bytes!` faces — deterministic default.
    #[default]
    BundledOnly,
}

/// Options for [`crate::emit_pdf_with`].
///
/// [`crate::emit_pdf`] is equivalent to `EmitOptions::default()`
/// (`FontResolveMode::BundledOnly`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EmitOptions {
    /// Font resolution policy.
    pub fonts: FontResolveMode,
}

impl EmitOptions {
    /// Deterministic sealed-pack emit (same as [`Default`]).
    #[must_use]
    pub fn bundled_only() -> Self {
        Self {
            fonts: FontResolveMode::BundledOnly,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_bundled_only() {
        assert_eq!(EmitOptions::default(), EmitOptions::bundled_only());
        assert_eq!(EmitOptions::default().fonts, FontResolveMode::BundledOnly);
    }
}
