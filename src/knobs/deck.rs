//! Slide / deck knobs (`defaults/deck.toml`).

use serde::{Deserialize, Serialize};

/// Deck / slide knobs (`defaults/deck.toml`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeckKnobs {
    /// Slide frame.
    pub slide: DeckSlideKnobs,
    /// Title region.
    pub title: DeckTitleKnobs,
    /// Subtitle region.
    pub subtitle: DeckSubtitleKnobs,
    /// Body / list regions.
    pub body: DeckBodyKnobs,
    /// Two-column layout.
    pub columns: DeckColumnsKnobs,
}

/// `[slide]` in `deck.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeckSlideKnobs {
    /// Top gap on a new slide (points).
    pub top_gap: f32,
}

/// `[title]` in `deck.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeckTitleKnobs {
    /// Title size scale on `deck@0`.
    pub scale: f32,
    /// Title size scale when not in deck profile.
    pub scale_non_deck: f32,
    /// Gap after title on deck.
    pub gap_after: f32,
    /// Gap after title off deck.
    pub gap_after_non_deck: f32,
}

/// `[subtitle]` in `deck.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeckSubtitleKnobs {
    /// Subtitle size as a factor of body size.
    pub size_factor: f32,
    /// Gap after subtitle (points).
    pub gap_after: f32,
}

/// `[body]` in `deck.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeckBodyKnobs {
    /// Gap after body region text (points).
    pub gap_after: f32,
    /// Tight gap between list-like slide regions (points).
    pub region_gap_after: f32,
    /// Slide list text size factor of body size.
    pub list_size_factor: f32,
}

/// `[columns]` in `deck.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeckColumnsKnobs {
    /// Two-column gap on deck (points).
    pub gap: f32,
    /// Two-column gap off deck (points).
    pub gap_non_deck: f32,
    /// Gap after a columns band on deck (points).
    pub gap_after: f32,
    /// Gap after a columns band off deck (points).
    pub gap_after_non_deck: f32,
    /// Gap after each region inside a column (points).
    pub region_gap_after: f32,
    /// Minimum column width (points).
    pub min_width: f32,
}
