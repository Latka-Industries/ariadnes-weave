//! Page chrome knobs (`defaults/page.toml`).

use serde::{Deserialize, Serialize};

/// Page chrome knobs (`defaults/page.toml`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageKnobs {
    /// Page-number / running footer.
    pub footer: PageFooterKnobs,
    /// Running header (bundled off).
    #[serde(default)]
    pub header: PageHeaderKnobs,
    /// Content box clearance.
    pub content: PageContentKnobs,
    /// Stroke / fill gray for rules and math chrome.
    pub chrome: PageChromeKnobs,
    /// Footnote band above footer chrome (THI-410).
    #[serde(default)]
    pub footnote: FootnoteKnobs,
    /// Page-number style for `{page}` / TOC labels (THI-413).
    #[serde(default)]
    pub numbers: PageNumbersKnobs,
}

/// Minimum vertical reserve (pt) when a chrome band is enabled.
pub const CHROME_RESERVE_FLOOR: f32 = 18.0;

impl PageKnobs {
    /// Space reserved at the bottom for an enabled footer.
    #[must_use]
    pub fn footer_reserve(&self) -> f32 {
        if self.footer.enabled {
            self.content.bottom_clearance.max(CHROME_RESERVE_FLOOR)
        } else {
            0.0
        }
    }

    /// Space reserved at the top for an enabled header.
    #[must_use]
    pub fn header_reserve(&self) -> f32 {
        if self.header.enabled {
            self.content.top_clearance.max(CHROME_RESERVE_FLOOR)
        } else {
            0.0
        }
    }

    /// Header + footer reserve for pagination.
    #[must_use]
    pub fn chrome_reserve(&self) -> f32 {
        self.footer_reserve() + self.header_reserve()
    }

    /// Extra body reserve when the document has footnotes (THI-410).
    ///
    /// Precedence (bottom of page): content → footnote band → footer chrome →
    /// margin. The band is skipped when the document has no footnote-kind notes.
    #[must_use]
    pub fn footnote_reserve(&self, has_footnotes: bool) -> f32 {
        if has_footnotes {
            self.footnote.max_band.max(0.0)
        } else {
            0.0
        }
    }

    /// Enabled header/footer bands (for glyph collect / paint).
    #[must_use]
    pub fn bands(&self) -> [&dyn PageChromeBand; 2] {
        [&self.footer, &self.header]
    }
}

/// Shared surface for `[footer]` / `[header]` paint + glyph collect.
pub trait PageChromeBand {
    /// Whether this band is painted.
    fn enabled(&self) -> bool;
    /// Format template (`{page}`, `{pages}`, `{title}`, `{heading}`).
    fn format(&self) -> &str;
    /// Optional even-page format override (THI-413). `None` → [`Self::format`].
    fn format_even(&self) -> Option<&str>;
    /// Font size in points.
    fn font_size(&self) -> f32;
    /// Horizontal alignment within the content width.
    fn align(&self) -> ChromeAlign;
    /// Optional even-page align override (THI-413). `None` → [`Self::align`].
    fn align_even(&self) -> Option<ChromeAlign>;
    /// Baseline as a factor of the corresponding margin.
    fn y_margin_factor(&self) -> f32;

    /// Format for this 1-based page (even pages use `format_even` when set).
    fn format_for_page(&self, page_no: usize) -> &str {
        if page_no.is_multiple_of(2) {
            self.format_even().unwrap_or_else(|| self.format())
        } else {
            self.format()
        }
    }

    /// Align for this 1-based page (even pages use `align_even` when set).
    fn align_for_page(&self, page_no: usize) -> ChromeAlign {
        if page_no.is_multiple_of(2) {
            self.align_even().unwrap_or_else(|| self.align())
        } else {
            self.align()
        }
    }
}

/// Horizontal align for page chrome bands (no justify).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChromeAlign {
    /// Flush left in the content width.
    Left,
    /// Centered (bundled footer default).
    #[default]
    Center,
    /// Flush right.
    Right,
}

impl ChromeAlign {
    /// Horizontal origin for natural-width chrome text within `measure`.
    #[must_use]
    pub fn offset_x(self, measure: f32, item_w: f32) -> f32 {
        match self {
            Self::Left => 0.0,
            Self::Center => ((measure - item_w) / 2.0).max(0.0),
            Self::Right => (measure - item_w).max(0.0),
        }
    }
}

/// `[footer]` in `page.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageFooterKnobs {
    /// Footer font size (points).
    pub font_size: f32,
    /// Footer baseline as a factor of bottom margin.
    pub y_margin_factor: f32,
    /// Draw the footer (bundled `true`; resume turns off).
    #[serde(default = "default_footer_enabled")]
    pub enabled: bool,
    /// Left / center / right within the content width.
    #[serde(default)]
    pub align: ChromeAlign,
    /// Template with `{page}`, `{pages}`, `{title}`, `{heading}`.
    #[serde(default = "default_footer_format")]
    pub format: String,
    /// Even-page format override (THI-413). `None` → [`Self::format`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format_even: Option<String>,
    /// Even-page align override (THI-413). `None` → [`Self::align`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align_even: Option<ChromeAlign>,
}

macro_rules! impl_page_chrome_band {
    ($ty:ty) => {
        impl PageChromeBand for $ty {
            fn enabled(&self) -> bool {
                self.enabled
            }
            fn format(&self) -> &str {
                &self.format
            }
            fn format_even(&self) -> Option<&str> {
                self.format_even.as_deref()
            }
            fn font_size(&self) -> f32 {
                self.font_size
            }
            fn align(&self) -> ChromeAlign {
                self.align
            }
            fn align_even(&self) -> Option<ChromeAlign> {
                self.align_even
            }
            fn y_margin_factor(&self) -> f32 {
                self.y_margin_factor
            }
        }
    };
}

impl_page_chrome_band!(PageFooterKnobs);

fn default_footer_enabled() -> bool {
    true
}

fn default_footer_format() -> String {
    "{page} / {pages}".into()
}

/// `[header]` in `page.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageHeaderKnobs {
    /// Header font size (points).
    #[serde(default = "default_header_font_size")]
    pub font_size: f32,
    /// Header baseline as a factor of top margin (from the top edge).
    #[serde(default = "default_header_y_factor")]
    pub y_margin_factor: f32,
    /// Draw the header (bundled `false`; resume forces off).
    #[serde(default)]
    pub enabled: bool,
    /// Left / center / right within the content width.
    #[serde(default = "default_header_align")]
    pub align: ChromeAlign,
    /// Template with `{page}`, `{pages}`, `{title}`, `{heading}`.
    #[serde(default = "default_header_format")]
    pub format: String,
    /// Even-page format override (THI-413). `None` → [`Self::format`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format_even: Option<String>,
    /// Even-page align override (THI-413). `None` → [`Self::align`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align_even: Option<ChromeAlign>,
}

impl_page_chrome_band!(PageHeaderKnobs);

impl Default for PageHeaderKnobs {
    fn default() -> Self {
        Self {
            font_size: default_header_font_size(),
            y_margin_factor: default_header_y_factor(),
            enabled: false,
            align: default_header_align(),
            format: default_header_format(),
            format_even: None,
            align_even: None,
        }
    }
}

fn default_header_font_size() -> f32 {
    9.0
}

fn default_header_y_factor() -> f32 {
    0.55
}

fn default_header_align() -> ChromeAlign {
    ChromeAlign::Left
}

fn default_header_format() -> String {
    "{title}".into()
}

/// `[numbers]` in `page.toml` — roman vs arabic page labels (THI-413).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageNumbersKnobs {
    /// Style for `{page}` and auto-resolved TOC labels.
    #[serde(default)]
    pub style: PageNumberStyle,
}

impl Default for PageNumbersKnobs {
    fn default() -> Self {
        Self {
            style: PageNumberStyle::Arabic,
        }
    }
}

/// Page index style for chrome `{page}` and TOC labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PageNumberStyle {
    /// Decimal `1`, `2`, `3` (bundled).
    #[default]
    Arabic,
    /// Lowercase roman `i`, `ii`, `iii`.
    Roman,
    /// Uppercase roman `I`, `II`, `III`.
    RomanUpper,
}

impl PageNumberStyle {
    /// Format a 1-based page index.
    #[must_use]
    pub fn format(self, page_no: usize) -> String {
        match self {
            Self::Arabic => page_no.to_string(),
            Self::Roman => to_roman(page_no, false),
            Self::RomanUpper => to_roman(page_no, true),
        }
    }
}

/// Convert `n` (1-based) to roman numerals. Values outside 1..=3999 fall back to arabic.
#[must_use]
pub fn to_roman(n: usize, upper: bool) -> String {
    if !(1..=3999).contains(&n) {
        return n.to_string();
    }
    let table: &[(&str, usize)] = if upper {
        &[
            ("M", 1000),
            ("CM", 900),
            ("D", 500),
            ("CD", 400),
            ("C", 100),
            ("XC", 90),
            ("L", 50),
            ("XL", 40),
            ("X", 10),
            ("IX", 9),
            ("V", 5),
            ("IV", 4),
            ("I", 1),
        ]
    } else {
        &[
            ("m", 1000),
            ("cm", 900),
            ("d", 500),
            ("cd", 400),
            ("c", 100),
            ("xc", 90),
            ("l", 50),
            ("xl", 40),
            ("x", 10),
            ("ix", 9),
            ("v", 5),
            ("iv", 4),
            ("i", 1),
        ]
    };
    let mut rest = n;
    let mut out = String::new();
    for &(sym, val) in table {
        while rest >= val {
            out.push_str(sym);
            rest -= val;
        }
    }
    out
}

/// Expand chrome `format` tokens for one page.
///
/// Unknown `{…}` tokens are left unchanged. Empty `title` / `heading` yield
/// empty substitutions. `{page}` follows `number_style`; `{page_roman}` /
/// `{page_Roman}` are always roman (THI-413).
#[must_use]
pub fn expand_chrome_format(
    format: &str,
    page_no: usize,
    page_count: usize,
    title: &str,
    heading: &str,
    number_style: PageNumberStyle,
) -> String {
    let mut out = String::with_capacity(format.len() + title.len() + heading.len() + 8);
    let mut rest = format;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else {
            out.push('{');
            rest = after;
            continue;
        };
        let key = &after[..end];
        match key {
            "page" => out.push_str(&number_style.format(page_no)),
            "page_roman" => out.push_str(&to_roman(page_no, false)),
            "page_Roman" => out.push_str(&to_roman(page_no, true)),
            "pages" => out.push_str(&page_count.to_string()),
            "title" => out.push_str(title),
            "heading" => out.push_str(heading),
            _ => {
                out.push('{');
                out.push_str(key);
                out.push('}');
            }
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod chrome_format_tests {
    use super::{PageNumberStyle, expand_chrome_format, to_roman};

    #[test]
    fn expands_page_pages_title() {
        assert_eq!(
            expand_chrome_format(
                "{title} — {page}/{pages}",
                2,
                5,
                "Notes",
                "",
                PageNumberStyle::Arabic
            ),
            "Notes — 2/5"
        );
    }

    #[test]
    fn empty_title_and_unknown_token() {
        assert_eq!(
            expand_chrome_format(
                "{title}|{page}|{bogus}",
                1,
                1,
                "",
                "",
                PageNumberStyle::Arabic
            ),
            "|1|{bogus}"
        );
    }

    #[test]
    fn expands_heading_token() {
        assert_eq!(
            expand_chrome_format(
                "{heading} · {page}",
                3,
                8,
                "Doc",
                "Methods",
                PageNumberStyle::Arabic
            ),
            "Methods · 3"
        );
        assert_eq!(
            expand_chrome_format("{heading}", 1, 1, "Doc", "", PageNumberStyle::Arabic),
            ""
        );
    }

    #[test]
    fn roman_page_style_and_tokens() {
        assert_eq!(to_roman(4, false), "iv");
        assert_eq!(to_roman(14, true), "XIV");
        assert_eq!(
            expand_chrome_format("{page}", 4, 12, "", "", PageNumberStyle::Roman),
            "iv"
        );
        assert_eq!(
            expand_chrome_format("{page}", 4, 12, "", "", PageNumberStyle::RomanUpper),
            "IV"
        );
        assert_eq!(
            expand_chrome_format(
                "{page_roman}/{page_Roman}",
                9,
                9,
                "",
                "",
                PageNumberStyle::Arabic
            ),
            "ix/IX"
        );
    }
}

/// `[content]` in `page.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageContentKnobs {
    /// Extra clearance above bottom margin when painting (points).
    pub bottom_clearance: f32,
    /// Extra clearance below top margin when a header is enabled (points).
    #[serde(default)]
    pub top_clearance: f32,
}

/// `[chrome]` in `page.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageChromeKnobs {
    /// Default stroke gray for rules / math chrome.
    pub stroke_gray: f32,
    /// Default fill gray for math chrome.
    pub fill_gray: f32,
}

/// `[footnote]` in `page.toml` (THI-410).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FootnoteKnobs {
    /// Maximum footnote band height above footer chrome (points).
    pub max_band: f32,
    /// Marker size as a factor of the surrounding run size.
    pub marker_scale: f32,
    /// Rule thickness above the footnote band (points).
    pub rule_thickness: f32,
    /// Note body size as a factor of profile body size.
    pub size_factor: f32,
    /// Note line leading as a factor of note size.
    pub leading_factor: f32,
    /// Gap between the rule and the first note line (points).
    pub gap_before_rule: f32,
}

impl Default for FootnoteKnobs {
    fn default() -> Self {
        Self {
            max_band: 72.0,
            marker_scale: 0.7,
            rule_thickness: 0.4,
            size_factor: 0.8,
            leading_factor: 1.15,
            gap_before_rule: 4.0,
        }
    }
}
