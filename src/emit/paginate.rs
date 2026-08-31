//! Break laid items into pages, honoring forced breaks and glue.

use super::types::{ForcedBreak, LaidItem};

/// Pack layout segments into pages that fit within `max_y_span` (content height).
///
/// `chrome_reserve` leaves room for header + footer bands (0 when both are
/// disabled). Items marked [`LaidItem::glue_after`] are peeled back with the
/// next item when a break would otherwise orphan them.
pub(super) fn paginate_items(
    segments: &[(ForcedBreak, Vec<LaidItem>)],
    max_y_span: f32,
    chrome_reserve: f32,
) -> Vec<Vec<LaidItem>> {
    let max_y_span = (max_y_span - chrome_reserve.max(0.0)).max(72.0);
    let mut pages: Vec<Vec<LaidItem>> = Vec::new();
    let mut current: Vec<LaidItem> = Vec::new();
    let mut used = 0.0_f32;

    let flush = |pages: &mut Vec<Vec<LaidItem>>, current: &mut Vec<LaidItem>, used: &mut f32| {
        if !current.is_empty() {
            pages.push(std::mem::take(current));
            *used = 0.0;
        } else if pages.is_empty() {
            pages.push(Vec::new());
        }
    };

    for (forced, items) in segments {
        if matches!(forced, ForcedBreak::Always) && !current.is_empty() {
            flush(&mut pages, &mut current, &mut used);
        }
        for item in items {
            let h = item.height();
            if used + h > max_y_span && !current.is_empty() {
                // Pull glued predecessors onto the next page with `item`.
                let mut peeled: Vec<LaidItem> = Vec::new();
                while current.last().is_some_and(LaidItem::glue_after) {
                    let peeled_item = current.pop().expect("last");
                    used -= peeled_item.height();
                    peeled.push(peeled_item);
                }
                if current.is_empty() {
                    // Entire page was glued — keep it and force a break anyway.
                    while let Some(l) = peeled.pop() {
                        used += l.height();
                        current.push(l);
                    }
                    flush(&mut pages, &mut current, &mut used);
                } else {
                    flush(&mut pages, &mut current, &mut used);
                    while let Some(l) = peeled.pop() {
                        used += l.height();
                        current.push(l);
                    }
                }
                if used + h > max_y_span && !current.is_empty() {
                    flush(&mut pages, &mut current, &mut used);
                }
            }
            used += h;
            current.push(item.clone());
        }
    }

    if current.is_empty() && pages.is_empty() {
        pages.push(Vec::new());
    } else if !current.is_empty() {
        pages.push(current);
    }

    pages
}

/// Last H1/H2 title in effect on each page (empty before the first heading).
///
/// Used to expand `{heading}` in page chrome (THI-409). H3+ do not change the
/// running head. A new H1/H2 on a page applies to that page's chrome.
pub(super) fn running_headings(pages: &[Vec<LaidItem>]) -> Vec<String> {
    let mut current = String::new();
    let mut out = Vec::with_capacity(pages.len());
    for page in pages {
        for item in page {
            if let LaidItem::Text(line) = item
                && let Some(heading) = line.chrome_heading.as_deref()
            {
                heading.clone_into(&mut current);
            }
        }
        out.push(current.clone());
    }
    out
}
