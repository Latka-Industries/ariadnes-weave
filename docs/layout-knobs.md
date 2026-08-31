# Layout knobs

Emit uses named optical defaults from per-category TOML files under `defaults/`:

| File | Category | Used for |
|------|----------|----------|
| `prose.toml` | prose | Paragraph/heading/list/quote/code/figure/caption spacing and figure align/width, quote/caption italic, optional text/quote/cite/caption colors, cite underline, optional category font pins, `[wrap]` hyphen/widow/orphan, `[body_columns].gap`, `[body].line_numbers` gutter (off by default), `[callout]` titled-band rule |
| `table.toml` | table | Cell padding, leading, and block gap |
| `deck.toml` | deck | Slide title/subtitle/body scales and column gaps (deck region columns — not article body) |
| `math.toml` | math | Fractions, `\bar` accent, scripts, big-op limits, matrices, arrows, display gaps |
| `page.toml` | page | Footer/header format+align, even-page overrides, `[numbers].style`, clearance, math stroke gray |

Profiles (`print@0`, `deck@0`, …) still own page size, margins, and body font size. Knobs are the hard-coded paddings, gaps, and scale factors inside emit. Rust types live under `src/knobs/` (`prose.rs` / `table.rs` / `deck.rs` / `math.rs` / `page.rs`), re-exported from [`LayoutKnobs`](../src/knobs/mod.rs).

## Aesthetic colors (optional)

Omit these keys for engine black / no underline (bundled `defaults/prose.toml` omits them):

| Key | Meaning |
|-----|---------|
| `[text].color` | Default body/heading/list fill (`#RGB` / `#RRGGBB`) |
| `[quote].color` | Quote fill; else inherits `[text].color` |
| `[caption].color` | Figure-caption fill; else inherits `[text].color` |
| `[cite].color` | Cite-run fill when `InlineStyle.cite`; else inherits category / text / black |
| `[cite].underline` | Underline cite runs (`false` by default) |

Inherit order for a run: cite color (if cite + set) → category (`quote`, `caption`, or `text`) → black.

Per-run underline also paints when `InlineStyle.underline` is set (independent of cite).

## Caption knobs

`[caption]` applies to `PrintBlock::Figure` caption runs only (v1). Non-figure Tessera caption paragraphs (`Paragraph` + emphasis stand-in) do not pick these up until a `Caption` IR or Tessera bridge lands.

### Body paragraph align

| Key | Meaning |
|-----|---------|
| `[paragraph].text_align` | Body `Paragraph` runs: `left` / `center` / `right` / `justify` (bundled `left`) |

Pack default only. A block may set `PrintBlock::Paragraph.text_align` (or a
`Columns` region default) to override; `None` uses this knob (THI-398).
Lists inherit the same resolution; quotes stay left unless a block/region sets
align.

Same paint rules as figure in-band justify (last soft-wrapped line stays flush-left).

Figure vertical stack:

| Gap | Knob |
|-----|------|
| Prior content → title/image | `[figure].gap_before` (replaces the prior block's trailing gap) |
| Title → image | `[figure].gap_after_title` |
| Image → caption | `[figure].gap_after_image` |
| Caption line box | `[caption].leading_factor` × caption size |
| Caption → following | `[caption].gap_after` |
| Figure with no caption | `[figure].gap_after` / `[figure].alt_gap_after` |

| Key | Meaning |
|-----|---------|
| `[caption].italic` | OR `emphasis` onto caption runs (`true` by default) |
| `[caption].size_factor` | Caption size as a factor of profile body size (bundled `0.9`) |
| `[caption].leading_factor` | Caption line leading as a factor of caption size (bundled `1.15`) |
| `[caption].gap_after` | Gap after a figure caption (points) |
| `[caption].color` | Optional fill (see above) |
| `[caption].font` | Optional category font pin (see below) |
| `[figure].gap_before` | Gap before the figure stack; replaces prior trailing gap (bundled `6`) |
| `[figure].gap_after_title` | Gap between `Figure.title` and the image (bundled `18`) |
| `[figure].gap_after_image` | Gap between image bottom and next item / caption (bundled `2`) |
| `[figure].align` | Horizontal band: `center` / `left` / `right` (bundled `center`) |
| `[figure].max_width_factor` | Cap image width as a factor of content width (`(0, 1]`; bundled `1.0`) |
| `[figure].title_align` | Title **band** placement: `follow` / `left` / `center` / `right` (bundled `follow`) |
| `[figure].title_text_align` | Title **text** within the band: `center` / `left` / `right` / `justify` / `follow` (bundled `center`) |
| `[caption].band` | `match_figure` (image indent + wrap width) or `full_measure` (bundled `match_figure`) |
| `[caption].text_align` | Caption **text** within its measure: `left` / `center` / `right` / `justify` / `follow` (bundled `left`) |
| `[caption].overflow` | Overlong token: `hard_break` or `soft_only` (bundled `hard_break`) |

`align` and `max_width_factor` size/place the image band. Title band placement defaults to `follow`. Title **text** defaults to **center** in that band; caption **text** defaults to **left** (flush to the figure edge) while the caption band still matches the figure. `follow` on text_align means “same as figure `align`” (left/center/right only — never justify). Soft wrap at spaces always runs; `overflow` only controls mid-word splitting. `justify` distributes extra space across inter-word gaps within the band; the last soft-wrapped line of a block stays flush-left.

Figure title lives on `PrintBlock::Figure.title` (empty = none). Prefer that over a prior `Paragraph`+strong stand-in so title can share the figure band.

Size is **`size_factor` vs body**, not absolute points — profiles keep owning body size.

## Math knobs (`defaults/math.toml`)

| Section | Meaning |
|---------|---------|
| `[display]` | Display size factor + pre/post gaps (inline uses `inline_gap_after`) |
| `[metrics]` | Math axis, mu/em, `.notdef` box |
| `[op]` | Big operators: display uses LM Math `.v1` glyphs; `size_factor` only for geo fallback; ∑-family under/over via `limit_size_factor` / `gap_*`; ∫/∮ `\nolimits`; `after_space_mu` after op-with-limits |
| `[script]` | Side super/subscripts (inline ∑/∏ and all ∫ limits) |
| `[frac]` / `[matrix]` / `[paren]` / `[arrow]` / `[infinity]` | Structured frac, matrix, chrome; `[paren].style` (`round`/`square`) applies to `pmatrix`; `bmatrix` is always square |

Display math places `^`/`_` as **under/over limits** on ∑ / ∏ / ⋃ / ⋂ / ∐ (centered). ∫ / ∮ keep tip-side `\nolimits`.

### Figure placement (today)

Horizontal geometry is knob-driven (`align`, `max_width_factor`, `title_align`, caption `band` / `overflow`). Vertical policy stays sealed reading-order:

- `FigurePlacement::Flow` — normal order
- `FigurePlacement::FloatNear` — same order + keep-with-previous glue (not true float / wrap)

Deferred: top/bottom page float, text wrap beside figures, freeform x/y. D24 `\layout{place/vspace/rule}` stays separate from `\figure{}`.

## Category fonts (optional)

Omit these keys to keep Liberation style mapping (bundled `defaults/prose.toml` omits them). Values are pin ids into `EmitOptions.pinned_faces` (same namespace as explicit `TextRun.face` / pack `fonts.toml`):

| Key | Meaning |
|-----|---------|
| `[text].font` | Default pin for body / list runs |
| `[heading].font` | Default pin for all heading levels |
| `[quote].font` | Default pin for quote body runs |
| `[caption].font` | Default pin for figure caption runs |
| `[cite].font` | Default pin for cite-styled runs |

**Precedence:** explicit `TextRun.face` wins. When `face` is unset, cite pin wins if `InlineStyle.cite`; else the mutually exclusive layout category (`heading` / `quote` / `caption` / `text`). Unknown ids use the same `unknown pinned face` error as an explicit pin. Category pins do not inherit across sections (unlike colors).

First cut defers table / code / math / deck and per-level `heading.1` / `heading.2`.

## Soft wrap (`[wrap]` in `defaults/prose.toml`)

| Key | Meaning |
|-----|---------|
| `body_leading_factor` | Leading factor for generic body runs (deck slides, etc.) |
| `min_width` | Floor on wrap measure (points) |
| `hyphenate` | Soft-hyphenate pure ASCII letter words when they do not fit (`true` bundled; `resume@0` densify forces off) |
| `orphan_lines` | Min content lines kept together at paragraph start (bundled `2`; values below 1 → `1`) |
| `widow_lines` | Min content lines kept together at paragraph end (bundled `2`; values below 1 → `1`) |

Hyphenation is conservative ASCII-first (no dictionary crate): words shorter than 5 letters, URLs / paths (`://`, `/`, `:`, `@`, `.`), digits, and non-ASCII letters are skipped. Valid splits keep ≥2 letters before the hyphen and ≥3 after; the engine picks the longest `prefix-` that fits the remaining measure.

## Body columns (`[body_columns]` in `defaults/prose.toml`)

Newspaper / article continuous columns for `PrintBlock::Columns` (THI-391). Distinct from deck slide `[columns]` / `two-column` templates and from `PrintBlock::Row` meta panes.

| Key | Meaning |
|-----|---------|
| `gap` | Space between columns in points (bundled `18`) |

Column count, optional per-block `gap`, and optional `text_align` live on the IR
(`Columns { count, gap, children, text_align }`). Headings, figures, tables,
math, breaks, rows, TOC lines, and nested columns **span** full measure (flush
the current band). Body paragraphs/lists/quotes/code flow down each column.
Child `text_align` wins; else the columns region default; else pack
`[paragraph].text_align`.

## Titled band (`PrintBlock::Callout`)

One paint for theorem/definition/proof (THI-414) and callout/Q&A (THI-412).
Tessera sets `callout_kind`; weave does not fork appearance.

| Key | Meaning |
|-----|---------|
| `[callout].indent` | Extra inset of the band (bundled `0`) |
| `[callout].rule_thickness` | Left rule (bundled `1.5`) |
| `[callout].rule_gap` | Gap between rule and title/body (bundled `8`) |
| `[callout].title_gap` | Gap between title and body (bundled `4`) |

Title runs are painted strong. Kind is not drawn.

## Review line numbers (THI-415)

| Key | Meaning |
|-----|---------|
| `[body].line_numbers` | Number laid-out body lines in a per-column gutter (bundled `false`) |
| `[body].line_number_gutter` | Gutter width reserved at the start of each column (bundled `18`) |
| `[body].line_number_size_factor` | Digit size vs body size (bundled `0.7`) |

Running 1-based count through the document (not reset per page). Headings,
paragraphs, quotes, lists, code, and callout lines are numbered; tables, math,
figures, and footnotes are not. In `Columns`, each column paints its own gutter.

Rust fields on `ProseBodyKnobs` are `numbers` / `gutter_width` / `size_factor`
(serde still reads the `line_*` keys above).

## TOC / destinations / outline

Not knob-driven; IR + emit behavior:

| Surface | Behavior |
|---------|----------|
| `PrintBlock::TocEntry` | Title + optional leaders + flush-right page column; `page_label: None` + `dest_id` → resolve after layout; `GoTo` when dest resolves |
| `Heading` / `Figure` / `Table` `dest_id` | Zero-height dest markers for page resolve (TOC / LOF / LOT) |
| PDF `/Outlines` | Built from headings that carry a resolvable `dest_id` (same Fit dests as TOC `GoTo`) |

Tessera owns `\toc` / `\lof` / `\lot` expansion into these blocks; weave only paints and links.

## Page chrome (`defaults/page.toml`)

Optional running header + page-number footer (THI-392). Tokens in `format`:

| Token | Meaning |
|-------|---------|
| `{page}` | 1-based page index, formatted by `[numbers].style` (`arabic` / `roman` / `roman_upper`) |
| `{page_roman}` | Always lowercase roman (`i`, `ii`, …) |
| `{page_Roman}` | Always uppercase roman (`I`, `II`, …) |
| `{pages}` | total page count (always arabic) |
| `{title}` | `PrintMeta.title` (empty if unset) |
| `{heading}` | Last H1 or H2 whose first line is on this page or an earlier page (empty before the first such heading). H3+ do not change the running head. |

`{chapter}` as a separate numbering system is out of scope (reuse heading text).

| Key | Meaning |
|-----|---------|
| `[footer].enabled` | Draw footer (bundled `true`; `resume@0` densify forces off) |
| `[footer].format` | Template (bundled `"{page} / {pages}"`) |
| `[footer].align` | `left` / `center` / `right` (bundled `center`) |
| `[footer].align_even` / `format_even` | Optional even-page overrides (THI-413; omit to use `align` / `format`) |
| `[footer].font_size` / `y_margin_factor` | Size + baseline as a fraction of bottom margin |
| `[header].*` | Same shape; bundled `enabled = false`, `align = left`, `format = "{title}"` |
| `[numbers].style` | `{page}` / TOC auto-labels: `arabic` (bundled) / `roman` / `roman_upper` |
| `[content].bottom_clearance` | Reserve above bottom margin when footer is on |
| `[content].top_clearance` | Reserve below top margin when header is on |
| `[chrome].stroke_gray` / `fill_gray` | Rules + math chrome gray |
| `[footnote].max_band` | Reserved height above footer for footnote bodies (bundled `72`; 0 when the doc has no footnotes) |
| `[footnote].marker_scale` | Superscript marker size vs surrounding run (bundled `0.7`) |
| `[footnote].rule_thickness` | Hairline above the footnote band (bundled `0.4`) |
| `[footnote].size_factor` | Note body size vs profile body (bundled `0.8`) |
| `[footnote].leading_factor` | Note line leading vs note size (bundled `1.15`) |
| `[footnote].gap_before_rule` | Gap from rule to first note line (bundled `4`) |

Bottom-of-page precedence: body content → footnote band → footer chrome → margin. Footnotes that do not fit `max_band` carry to the next page (simple split; no TeX multipass). Endnotes dump after the last body block as a “Notes” heading plus numbered paragraphs.

## Defaults and overrides

- Bundled defaults load via [`LayoutKnobs::bundled()`] (cached at first use).
- [`EmitOptions::layout`] carries the knob set for a single emit; default is bundled.
- Future work may load overrides from project config; bundled TOMLs remain the baseline.

## Dump bundled values

```bash
cargo run --example dump_knobs
```

Prints flat `category.section.key = value` lines suitable for agents or diffing.

[`LayoutKnobs::bundled()`]: ../src/knobs/mod.rs
[`EmitOptions::layout`]: ../src/options.rs
