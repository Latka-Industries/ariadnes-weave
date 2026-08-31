# Print profiles

Versioned metrics resolved by [`resolve_metrics`](../src/profile.rs). Unknown
`name@version` → `WeaveError::UnsupportedProfile`.

**Axes:** mode · page size · margins — see
[D-print-profile-axes](decisions/D-print-profile-axes.md). MVP **couples** them
into one id; internals compose via `PageSize` + mode rhythm. No `draft` profile.

**Versioning:** bump `PrintProfileId.version` when any **stable** pagination or
geometry rule for that id changes. Heuristics listed as best-effort in
[D-literary-unfolding](decisions/D-literary-unfolding.md) may change under the
same version.

## `print@0`

| | |
| --- | --- |
| Mode | print (Sans, compact) |
| Page | A4 `595 × 842` pt |
| Margin | `72` pt |
| Body | `11` pt, leading `×1.4`, Liberation Sans |
| H1 page break | no |
| Deck | no |

Default technical prose (A4 historical stub). Prefer `print-letter@0` for US Letter.

## `print-letter@0`

| | |
| --- | --- |
| Mode | print (same as `print@0`) |
| Page | US Letter `612 × 792` pt |
| Margin | `72` pt |
| Body | identical to `print@0` |
| H1 page break | no |
| Deck | no |

US Letter technical prose — same mode, different page axis.

## `resume@0`

Dense one-column CV / resume layout (THI-324 dogfood).

| | |
| --- | --- |
| Mode | resume |
| Page | US Letter `612 × 792` pt |
| Margin | `18` pt (~0.25 in) |
| Body | `9.5` pt, leading `11.5` (`×11.5/9.5`), Liberation Sans |
| Headings | dense scale (H1 `18`, H2 `11.5`, …) |
| H1 page break | no |
| Deck | no |

Emit also applies [`LayoutKnobs::densify_resume`](../src/knobs.rs) (tighter
paragraph/heading gaps, list `end_gutter` ≈ 1.25 in, `prose.indent.step = 14`
for sealed band levels, smaller table pad, **no page header/footer**, **no
hyphenation**, dark-blue link fill). Band geometry comes from authored
`PrintBlock::{Paragraph,List,Row}.indent` (`level × step`); style heuristics no
longer invent org/role indent.

Use [`PrintBlock::Row`](../src/ir.rs) (`panes`) for meta lines (`\hfill`
stand-in) instead of two-column tables. Two panes = classic left/right; three
or more share leftover measure among leading panes and flush the last.

## `manuscript@0`

Literary / beta-reader manuscript. See
[D-literary-unfolding](decisions/D-literary-unfolding.md).

| | |
| --- | --- |
| Mode | manuscript |
| Page | US Letter `612 × 792` pt |
| Margin | `96` pt |
| Body | `12` pt, leading `×2.0`, Liberation Serif |
| H1 page break | yes (`force_h1_page_break`) |
| Keep-with-next | H1 / H2 always |
| Deck | no |

## `deck@0`

| | |
| --- | --- |
| Mode | deck |
| Page | 16:9 `960 × 540` pt |
| Margin | `40` pt |
| Body | `22` pt, leading `×1.35`, Liberation Sans |
| H1 page break | no |
| Deck | yes (`Slide` → one page) |

### Slide `layout_id` templates

| Id | Geometry |
| --- | --- |
| `title-body` (default / unknown) | Full-width vertical stack |
| `title-subtitle-body` | Same stack; order title → subtitle → body |
| `two-column` / `title-two-column` | Title (+ optional subtitle) band; then equal `left` / `right` columns |

Column slots: `left` / `col1` / `body-left`, `right` / `col2` / `body-right`. Plain `body` without a side goes to the left column.

## Long-doc surfaces (any prose profile)

These are IR/emit features, not separate profile ids:

| Feature | Notes |
| --- | --- |
| Page chrome | `[header]` / `[footer]` knobs (`{page}` / `{page_roman}` / `{pages}` / `{title}` / `{heading}`); optional `align_even` / `format_even`; `[numbers].style`; `resume@0` densify forces both off |
| Per-block align | Optional `text_align` on `Paragraph` / `List` / `Quote` / `Columns` (else pack `[paragraph].text_align`) |
| Titled band | `PrintBlock::Callout` — one paint for theorem and callout kinds |
| Line numbers | `[body].line_numbers` (off by default); per-column gutter |
| Notes | `PrintBlock::Note` + `TextRun.note_id`; footnote band above footer; endnotes dump after body |
| Hyphen / widows | `[wrap]` knobs; `resume@0` densify forces `hyphenate = false` |
| `TocEntry` | Leaders + page resolve + `GoTo` from `dest_id` |
| PDF outline | `/Outlines` from heading `dest_id`s |
| `Columns` | Continuous body columns; gap from IR or `[body_columns].gap` |
| Figure/table dests | Optional `dest_id` for LOF/LOT page resolve |

See [`layout-knobs.md`](layout-knobs.md).

## Break hints

`BreakHint` on blocks / headings:

| Hint | Emit today |
| --- | --- |
| `None` | No forced break (manuscript H1 still breaks via profile) |
| `Page` / `PageAlways` | Forced new page if current page has content (same behavior today) |
| `KeepWithNext` | Glue heading to following content |

Profile-driven H1 breaks do **not** require `PageAlways` on the heading.
