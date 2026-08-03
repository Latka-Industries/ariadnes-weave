# Sealed font packs

ariadnes-weave embeds TrueType faces under `fonts/` and subsets used glyphs per
PDF. Host-pinned TTFs and optional OS lookup (`--features os-fonts`) sit on top;
sealed packs are the deterministic ladder when a codepoint is missing.

## Always on (default)

| Role | Faces | License |
| --- | --- | --- |
| Sans | Liberation Sans R/B/I/BI | SIL OFL — `fonts/LICENSE-Liberation.txt` |
| Serif | Liberation Serif R/B/I/BI | same |
| Mono | Liberation Mono Regular | same |

## Opt-in Cargo features

| Feature | `FaceId` | Bundle | Notes |
| --- | --- | --- | --- |
| `icons` | `IconSolid` / `IconRegular` / `IconBrands` | Font Awesome Free TTFs | PUA icons; not auto-fallback |
| `cjk` | `CjkSans` | `sealed-cjk-subset.ttf` | Tiny Droid Sans Fallback subset (Apache 2.0) |
| `emoji` | `Emoji` | `sealed-emoji-subset.ttf` | Tiny Noto Emoji B&W subset (SIL OFL) |

Default CI stays on Liberation (+ `icons`). Enable `cjk` / `emoji` only when you
need those scripts; the committed subsets are **smoke-sized** (a handful of
glyphs), not full Noto CJK / color emoji. Grow the subsets (or swap in larger
sealed faces) when product coverage needs it — keep them feature-gated so crates.io
default downloads stay small.

Color-emoji PDF paint (CBDT/COLR) is out of scope; the emoji pack is monochrome
outlines suitable for the existing Type0 / `FontFile2` path.

## Fallback order

For each character in a shaped run:

1. Requested / style face (Liberation, pin, or OS-resolved pin)
2. If that face has no glyph: sealed `emoji` (when `--features emoji` and the
   codepoint looks emoji-like), then sealed `cjk` (when `--features cjk` and
   CJK/kana/hangul/fullwidth)
3. Otherwise stay on the primary face (`.notdef` / tofu)

Whitespace and controls stay on the primary face so mixed Latin+CJK lines do not
churn spans at every space.

## Regenerating subsets

Local recipe (needs `fonttools`):

```bash
pyftsubset /path/to/DroidSansFallbackFull.ttf \
  --text=' Hello中文测试你好世界，。' \
  --output-file=fonts/sealed-cjk-subset.ttf

pyftsubset /path/to/NotoEmoji-Regular.ttf \
  --text=' 😀🔥👍❤️' \
  --output-file=fonts/sealed-emoji-subset.ttf
```

Only **TrueType (`glyf`)** faces are supported for sealed packs today (PDF
`FontFile2`). CFF/OTTO sources must be converted or replaced before bundling.
