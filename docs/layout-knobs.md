# Layout knobs

Emit uses named optical defaults from per-category TOML files under `defaults/`:

| File | Category | Used for |
|------|----------|----------|
| `prose.toml` | prose | Paragraph/heading/list/quote/code/figure/caption spacing, quote/caption italic, optional text/quote/cite/caption colors, cite underline, optional category font pins, wrap width |
| `table.toml` | table | Cell padding, leading, and block gap |
| `deck.toml` | deck | Slide title/subtitle/body scales and column gaps |
| `math.toml` | math | Fractions, scripts, matrices, arrows, display gaps |
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

Figure vertical stack (Tessera title chunk = prior `Paragraph` + strong):

| Gap | Knob |
|-----|------|
| Prior content / title → image | `[figure].gap_before` (replaces the prior block's trailing gap) |
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
| `[figure].gap_before` | Gap before the image; replaces prior trailing gap (bundled `6`) |
| `[figure].gap_after_image` | Gap between image bottom and next item / caption (bundled `2`) |

Size is **`size_factor` vs body**, not absolute points — profiles keep owning body size.

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
