# Layout knobs

Emit uses named optical defaults from per-category TOML files under `defaults/`:

| File | Category | Used for |
|------|----------|----------|
| `prose.toml` | prose | Paragraph/heading/list/quote/code/figure/caption spacing and figure align/width, quote/caption italic, optional text/quote/cite/caption colors, cite underline, optional category font pins, wrap width |
| `table.toml` | table | Cell padding, leading, and block gap |
| `deck.toml` | deck | Slide title/subtitle/body scales and column gaps |
| `math.toml` | math | Fractions, scripts, big-op limits, matrices, arrows, display gaps |
| `page.toml` | page | Footer size/position, bottom clearance, math stroke gray |

Profiles (`print@0`, `deck@0`, …) still own page size, margins, and body font size. Knobs are the hard-coded paddings, gaps, and scale factors inside emit.

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

Same paint rules as figure in-band justify (last soft-wrapped line stays flush-left). Lists / quotes stay left unless they gain their own knobs.

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
| `[op]` | Big operators: glyph `size_factor`; display under/over `limit_size_factor` / `gap_above_factor` / `gap_below_factor`; `after_space_mu` after an op-with-limits atom |
| `[script]` | Side super/subscripts (inline ∑/∏ and all ∫ limits) |
| `[frac]` / `[matrix]` / `[paren]` / `[arrow]` / `[infinity]` | Structured frac, matrix, chrome |

Display math places `^`/`_` as **under/over limits** on ∑ / ∏ / ⋃ / ⋂ / ∐ (TeX `\displaylimits`). ∫ / ∮ use **`\nolimits`**: limits to the right of the sign, at its top and bottom tips. Letter-like side scripts stay for everything else / inline non-ops.

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

## Defaults and overrides

- Bundled defaults load via [`LayoutKnobs::bundled()`] (cached at first use).
- [`EmitOptions::layout`] carries the knob set for a single emit; default is bundled.
- Future work may load overrides from project config; bundled TOMLs remain the baseline.

## Dump bundled values

```bash
cargo run --example dump_knobs
```

Prints flat `category.section.key = value` lines suitable for agents or diffing.

[`LayoutKnobs::bundled()`]: ../src/knobs.rs
[`EmitOptions::layout`]: ../src/options.rs
