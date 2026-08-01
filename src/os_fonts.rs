//! OS font stack lookup via [`fontdb`] (`--features os-fonts`).

use fontdb::{Database, Family, Query, Stretch, Style, Weight};

use crate::ir::InlineStyle;

/// Loaded system font database for one emit.
pub struct OsFontDb {
    db: Database,
}

impl OsFontDb {
    /// Scan platform font directories / Fontconfig into an in-memory DB.
    #[must_use]
    pub fn load_system() -> Self {
        let mut db = Database::new();
        db.load_system_fonts();
        Self { db }
    }

    /// Best TrueType match for `family` + style flags.
    ///
    /// Returns `None` when nothing matches, the face is CFF/`OTTO`, a font
    /// collection (`ttcf`), or a non-zero face index (unsupported by our
    /// subset/embed path today).
    #[must_use]
    pub fn resolve_ttf(&self, family: &str, style: InlineStyle) -> Option<Vec<u8>> {
        let weight = if style.strong {
            Weight::BOLD
        } else {
            Weight::NORMAL
        };
        let font_style = if style.emphasis {
            Style::Italic
        } else {
            Style::Normal
        };
        let id = self.db.query(&Query {
            families: &[Family::Name(family)],
            weight,
            stretch: Stretch::Normal,
            style: font_style,
        })?;
        self.db
            .with_face_data(id, |data, face_index| {
                if face_index != 0 || data.len() < 4 {
                    return None;
                }
                let magic = &data[..4];
                // Reject CFF OpenType and TrueType collections for now.
                if magic == b"OTTO" || magic == b"ttcf" {
                    return None;
                }
                // Accept Windows TT (`\0\x01\0\0`) and Apple `true`.
                if magic != [0, 1, 0, 0] && magic != b"true" {
                    return None;
                }
                Some(data.to_vec())
            })
            .flatten()
    }
}

/// Stable pin id for an OS-resolved face (family + style bits).
#[must_use]
pub fn os_pin_key(family: &str, style: InlineStyle) -> String {
    format!(
        "{family}#s{}e{}",
        u8::from(style.strong),
        u8::from(style.emphasis)
    )
}
