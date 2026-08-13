//! Conservative ASCII-first soft hyphenation for line wrapping (THI-394).
//!
//! No dictionary / Liang patterns — only pure `[A-Za-z]+` tokens, min 2+3
//! split, prefer the longest prefix that fits as `prefix-` in remaining measure.

use crate::error::WeaveError;
use crate::font::{FaceRef, FontBag, shape_text_with_fallback, shaped_runs_width};

/// Minimum letters before a hyphen break.
const MIN_PREFIX: usize = 2;
/// Minimum letters after a hyphen break (not counting the hyphen).
const MIN_SUFFIX: usize = 3;
/// Shortest word we will attempt to hyphenate.
const MIN_WORD: usize = MIN_PREFIX + MIN_SUFFIX;

/// Split trailing whitespace from a wrap chunk (`word` + spaces).
#[must_use]
pub(super) fn split_trailing_space(chunk: &str) -> (&str, &str) {
    let trimmed = chunk.trim_end_matches(|c: char| c.is_whitespace());
    let word_len = trimmed.len();
    (&chunk[..word_len], &chunk[word_len..])
}

/// True when `word` (trailing whitespace already stripped) may be hyphenated.
///
/// Rejects short tokens, mixed alphanumerics, URLs / paths (`://`, `/`, `:`,
/// `@`, `.`), and non-ASCII letters.
#[must_use]
pub(super) fn is_hyphenable(word: &str) -> bool {
    let word = word.trim_end();
    if word.chars().count() < MIN_WORD {
        return false;
    }
    if word.contains("://") || word.bytes().any(|b| matches!(b, b'/' | b':' | b'@' | b'.')) {
        return false;
    }
    word.chars().all(|c| c.is_ascii_alphabetic())
}

/// Byte/char indices `i` where a break after `word[..i]` is valid.
///
/// ASCII-only words ⇒ byte index equals char index. First part ≥ 2, rest ≥ 3.
#[must_use]
pub(super) fn hyphen_break_points(word: &str) -> Vec<usize> {
    let word = word.trim_end();
    if !is_hyphenable(word) {
        return Vec::new();
    }
    let n = word.len();
    (MIN_PREFIX..=n.saturating_sub(MIN_SUFFIX)).collect()
}

/// Largest hyphenated prefix of `word` that fits in `remaining` points.
///
/// Returns `(prefix_with_hyphen, remainder_without_hyphen)`, or `None` when no
/// valid split fits (or the word is ineligible).
///
/// # Errors
///
/// Returns [`WeaveError::Font`] if shaping fails.
pub(super) fn hyphen_fit(
    fonts: &FontBag,
    face: FaceRef,
    word: &str,
    font_size: f32,
    remaining: f32,
) -> Result<Option<(String, String)>, WeaveError> {
    let word = word.trim_end();
    let points = hyphen_break_points(word);
    if points.is_empty() || remaining <= 0.0 {
        return Ok(None);
    }
    for &i in points.iter().rev() {
        let mut prefix = String::with_capacity(i + 1);
        prefix.push_str(&word[..i]);
        prefix.push('-');
        let w = shaped_runs_width(&shape_text_with_fallback(fonts, face, &prefix, font_size)?);
        if w <= remaining {
            return Ok(Some((prefix, word[i..].to_string())));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::{FaceId, FaceRef, FontBag};

    #[test]
    fn short_and_ineligible_words_have_no_breaks() {
        assert!(hyphen_break_points("test").is_empty()); // len 4
        assert!(hyphen_break_points("ab").is_empty());
        assert!(hyphen_break_points("https://example.com").is_empty());
        assert!(hyphen_break_points("foo/bar").is_empty());
        assert!(hyphen_break_points("user@host").is_empty());
        assert!(hyphen_break_points("file.name").is_empty());
        assert!(hyphen_break_points("abc123").is_empty());
        assert!(hyphen_break_points("café").is_empty());
        assert!(hyphen_break_points("word ").is_empty()); // trim → "word" len 4
        assert!(!is_hyphenable("test"));
        assert!(is_hyphenable("hyphenation"));
    }

    #[test]
    fn ascii_letter_breaks_respect_min_parts() {
        // "hyphenation" len 11 → i in 2..=8
        assert_eq!(
            hyphen_break_points("hyphenation"),
            vec![2, 3, 4, 5, 6, 7, 8]
        );
        // len 5 → only i=2 (first 2, rest 3)
        assert_eq!(hyphen_break_points("abcde"), vec![2]);
    }

    #[test]
    fn split_trailing_space_keeps_spaces() {
        assert_eq!(split_trailing_space("word  "), ("word", "  "));
        assert_eq!(split_trailing_space("word"), ("word", ""));
    }

    #[test]
    fn hyphen_fit_picks_largest_prefix_that_fits() {
        let fonts = FontBag::default();
        let face = FaceRef::Bundled(FaceId::SansRegular);
        let word = "hyphenation";
        assert!(
            hyphen_fit(&fonts, face, word, 12.0, 1.0)
                .expect("shape")
                .is_none()
        );
        let (left, right) = hyphen_fit(&fonts, face, word, 12.0, 10_000.0)
            .expect("shape")
            .expect("fit");
        assert_eq!(left, "hyphenat-");
        assert_eq!(right, "ion");
        assert!(left.ends_with('-'));
        assert!(!right.contains('-'));
    }

    #[test]
    fn hyphen_fit_respects_narrow_remaining() {
        let fonts = FontBag::default();
        let face = FaceRef::Bundled(FaceId::SansRegular);
        let word = "internationalization";
        let full =
            shaped_runs_width(&shape_text_with_fallback(&fonts, face, word, 11.0).expect("shape"));
        let half = full * 0.45;
        let (left, right) = hyphen_fit(&fonts, face, word, 11.0, half)
            .expect("fit")
            .expect("should hyphenate into half width");
        assert!(left.ends_with('-'));
        assert!(!right.is_empty());
        let left_w =
            shaped_runs_width(&shape_text_with_fallback(&fonts, face, &left, 11.0).expect("shape"));
        assert!(left_w <= half + 0.5);
        assert_eq!(format!("{}{right}", &left[..left.len() - 1]), word);
    }
}
