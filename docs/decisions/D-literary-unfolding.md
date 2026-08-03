# D-literary-unfolding — deterministic manuscript pagination

**Status:** accepted (THI-295)  
**Applies to:** `ariadnes-weave` native emit · profile `manuscript@0`  
**Parent:** [THI-256](https://linear.app/thicclatka/issue/THI-256)

## Guarantee

**Literary unfolding** means: the same print IR + the same print profile version, under native emit (`BundledOnly` default), yields the **same pagination** — chapter opens, page geometry, and body rhythm — every time. Export is a replayable projection, not “hope the browser paginates.”

Byte-stable PDF for pinned fixtures is the acceptance proof (see `tests/determinism.rs`).

Chapter-scoped export (`tes export --chapter N`) lives in Tessera: the host may emit a subset IR. Weave’s contract is on whatever IR it receives.

## Stable under `manuscript@0`

Changing these requires bumping the profile version (e.g. `manuscript@1`):

| Rule | Behavior |
| --- | --- |
| H1 page break | After the first content on a page, each H1 starts a new page (`force_h1_page_break`) — no explicit `BreakHint::PageAlways` required |
| Keep-with-next | H1 and H2 stay with following content (plus explicit `BreakHint::KeepWithNext`) |
| Page | US Letter `612 × 792` pt |
| Margins | `96` pt uniform |
| Body type | `12` pt, leading `24` (double-spaced) |
| Body face | Liberation Serif for unstyled body (`serif_body`) — **not** Courier; Courier would be a future profile version if product wants it |
| Determinism | Fixed IR + profile → identical PDF bytes under `FontResolveMode::BundledOnly` |

## Best-effort (may change without a profile bump)

| Area | Notes |
| --- | --- |
| Widow / orphan glue | Basic first/penultimate line glue; not a literary guarantee yet |
| Soft `BreakHint::Page` | Currently treated like `PageAlways`; soft preference is not distinct |
| Exact wrap of extreme tokens | Hard-broken URLs / overlong words |
| Heading face | Liberation Sans Bold (display), even when body is serif |
| Chromium parity | Out of scope — native layout is source of truth for PDF |

## Acceptance

* `tests/determinism.rs` — manuscript two-chapter fixture: ≥2 pages from profile H1 breaks, SHA pin, US Letter + serif body checks
* Profile metrics: `src/profile.rs` · rules overview: [`docs/profiles.md`](../profiles.md)

## Out of scope

* Reader-mode apps, ePub/Kindle reflow, matching Chromium pixels
