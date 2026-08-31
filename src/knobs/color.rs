//! `#RGB` / `#RRGGBB` aesthetic colors.

use serde::{Deserialize, Serialize};

/// `#RGB` / `#RRGGBB` color for aesthetic knobs (0..=255 channels).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HexColor {
    /// Red 0..=255.
    pub r: u8,
    /// Green 0..=255.
    pub g: u8,
    /// Blue 0..=255.
    pub b: u8,
}

impl HexColor {
    /// Parse `#RGB` or `#RRGGBB` (case-insensitive hex digits).
    ///
    /// # Errors
    ///
    /// Returns a message when the string is not a valid hex color.
    pub fn parse(raw: &str) -> Result<Self, String> {
        let s = raw.trim();
        let hex = s
            .strip_prefix('#')
            .ok_or_else(|| format!("expected #RGB or #RRGGBB, got {raw:?}"))?;
        let full = match hex.len() {
            3 => {
                let mut expanded = String::with_capacity(6);
                for ch in hex.chars() {
                    expanded.push(ch);
                    expanded.push(ch);
                }
                expanded
            }
            6 => hex.to_string(),
            _ => {
                return Err(format!(
                    "expected #RGB or #RRGGBB (3 or 6 hex digits), got {raw:?}"
                ));
            }
        };
        if !full.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(format!("non-hex digit in color {raw:?}"));
        }
        let n = u32::from_str_radix(&full, 16).map_err(|_| format!("invalid hex color {raw:?}"))?;
        Ok(Self {
            r: ((n >> 16) & 0xff) as u8,
            g: ((n >> 8) & 0xff) as u8,
            b: (n & 0xff) as u8,
        })
    }

    /// Canonical `#RRGGBB` form.
    #[must_use]
    pub fn to_hex_string(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }

    /// PDF `set_fill_rgb` components in 0.0..=1.0.
    #[must_use]
    pub fn to_rgb01(self) -> [f32; 3] {
        [
            f32::from(self.r) / 255.0,
            f32::from(self.g) / 255.0,
            f32::from(self.b) / 255.0,
        ]
    }
}

impl Serialize for HexColor {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex_string())
    }
}

impl<'de> Deserialize<'de> for HexColor {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(serde::de::Error::custom)
    }
}
