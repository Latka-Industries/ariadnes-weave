# Print profiles

Versioned metrics resolved by [`resolve_metrics`](../src/profile.rs). Unknown
`name@version` → `WeaveError::UnsupportedProfile`.

**Axes (product):** mode (prose / manuscript / deck) · page size · margins/coords.
MVP **couples** them into one `name@version` (e.g. `print@0` = A4 technical
prose — historical stub, not a hard lock that default print must be A4).
No `draft` profile. Prefer adding a Letter prose profile or bumping `print@N`
later over renaming into `print-A4` / `print-usletter` as primary brands.

**Versioning:** bump `PrintProfileId.version` when any **stable** pagination or
geometry rule changes. Heuristics listed as best-effort in
[D-literary-unfolding](decisions/D-literary-unfolding.md) may change under the
same version.

## `print@0`

| | |
| --- | --- |
| Page | A4 `595 × 842` pt |
| Margin | `72` pt |
| Body | `11` pt, leading `×1.4`, Liberation Sans |
| H1 page break | no |
| Deck | no |

Default for notes / technical prose.

## `manuscript@0`

Literary / beta-reader manuscript. See
[D-literary-unfolding](decisions/D-literary-unfolding.md).

| | |
| --- | --- |
| Page | US Letter `612 × 792` pt |
| Margin | `96` pt |
| Body | `12` pt, leading `×2.0`, Liberation Serif |
| H1 page break | yes (`force_h1_page_break`) |
| Keep-with-next | H1 / H2 always |
| Deck | no |

## `deck@0`

| | |
| --- | --- |
| Page | 16:9 `960 × 540` pt |
| Margin | `40` pt |
| Body | `22` pt, leading `×1.35`, Liberation Sans |
| H1 page break | no |
| Deck | yes (`Slide` → one page) |

## Break hints

`BreakHint` on blocks / headings:

| Hint | Emit today |
| --- | --- |
| `None` | No forced break (manuscript H1 still breaks via profile) |
| `Page` / `PageAlways` | Forced new page if current page has content (same behavior today) |
| `KeepWithNext` | Glue heading to following content |

Profile-driven H1 breaks do **not** require `PageAlways` on the heading.
