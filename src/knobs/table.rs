//! Table layout knobs (`defaults/table.toml`).

use serde::{Deserialize, Serialize};

/// Table layout knobs (`defaults/table.toml`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableKnobs {
    /// Cell padding / leading.
    pub cell: TableCellKnobs,
    /// Outer table block gap.
    pub block: TableBlockKnobs,
}

/// `[cell]` in `table.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableCellKnobs {
    /// Cell padding (points).
    pub pad: f32,
    /// Cell line leading factor (capped by body leading).
    pub leading_factor: f32,
    /// Minimum inner cell content width (points).
    pub min_inner_width: f32,
}

/// `[block]` in `table.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableBlockKnobs {
    /// Gap after the table (points).
    pub gap_after: f32,
}
