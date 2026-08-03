# ariadnes-weave

[![Crates.io](https://img.shields.io/crates/v/ariadnes-weave.svg)](https://crates.io/crates/ariadnes-weave)
[![docs.rs](https://img.shields.io/docsrs/ariadnes-weave)](https://docs.rs/ariadnes-weave)
![Build](https://github.com/Latka-Industries/ariadnes-weave/workflows/Build/badge.svg)
![Rust](https://img.shields.io/badge/rust-1.95-orange.svg)

Native deterministic print engine for [Tessera](https://github.com/Latka-Industries/tessera):
**print IR → PDF**.

HTML + CSS stays Tessera’s browser preview path. This crate owns pagination and
PDF bytes so the same document + print profile yields the same layout (see
Tessera `docs/print_ir.md` / D21).

Consumers: Tessera `0.2` (`--features native-pdf`) depends on this crate from
crates.io. Local suite work can still path-dep when cutting paired releases.

## Status (0.2.3)

**Prose + structure emit** via `emit_pdf` / `emit_pdf_with`:

* Blocks: `Heading` / `Paragraph` / `List` / `Code` / `Quote` / `Break` /
  `Table` / `Figure` / `Slide` / `Math`
* Faces: Liberation Sans (R/B/I/BI), Serif (R/B/I/BI) for `manuscript@0`, Mono
  for `code`; optional Font Awesome Free behind `--features icons`; optional
  sealed CJK / emoji subsets behind `--features cjk` / `emoji` (script fallback)
* Host fonts: pin TTFs on `EmitOptions` and select with `TextRun::face` /
  `TextRun::pinned` (`FaceRef` / `FontBag`); optional `--features os-fonts` +
  `FontResolveMode::OsWithFallback` for OS lookup with Liberation fallback
* Shaping: `rustybuzz` + Type0 / CIDFontType2 / Identity-H; `subsetter` for
  used glyphs only
* Profiles: `print@0` (A4 prose), `print-letter@0` (US Letter prose),
  `manuscript@0` (Letter, double-spaced, H1 page breaks —
  [literary unfolding](docs/decisions/D-literary-unfolding.md)),
  `deck@0` (16:9); axes in [`docs/profiles.md`](docs/profiles.md) /
  [D-print-profile-axes](docs/decisions/D-print-profile-axes.md)
* Forced page breaks + keep-with-next + basic widow/orphan glue; page-number
  footers
* Deterministic emit: sorted font object order + SHA-256 fixtures in
  `tests/determinism.rs`
* `Table` → drawn grid + wrapped cells; `Figure` → PNG/JPEG XObject
  `FloatNear` glue); `Slide` → one page with `layout_id` templates (`title-body`,
  `two-column`, …); `Math` → structured layout for
  `\frac`, multi-char scripts, and simple `matrix`/`pmatrix` (not full TeX)

Not yet: full TeX/MathML, OS font collection/CFF polish, full-coverage CJK/emoji
packs (only tiny sealed subsets behind features), color-emoji PDF paint.

| Later | Where |
| --- | --- |
| OS font scan polish / collections / CFF | [THI-311](https://linear.app/thicclatka/issue/THI-311) |
| Larger / regional sealed CJK packs; color emoji | follow-ons under [THI-308](https://linear.app/thicclatka/issue/THI-308) |

**Bricks:** `pdf-writer` + `rustybuzz` + `ttf-parser`. Fonts under `fonts/`
(Liberation + optional FA / CJK / emoji; see [`docs/fonts.md`](docs/fonts.md)).
Not cosmic-text / krilla for v0.

**Font packs:** default sealed set = Liberation sans/serif/mono. Opt in with
`--features icons` for Font Awesome Free (`FaceId::Icon*`), `--features cjk` for
`FaceId::CjkSans`, `--features emoji` for `FaceId::Emoji`. Tessera may expose
these as `weave-icons` / `weave-cjk` / `weave-emoji`. Script fallback uses sealed
CJK/emoji when the primary face lacks a glyph. Host/GUI faces use pins; OS lookup
is behind `--features os-fonts`.

```bash
mise trust   # rust 1.95 via .mise.toml
cargo test
cargo test --features icons
cargo test --features cjk,emoji
cargo clippy --all-targets -- -D warnings
cargo run --example prose
mise samples   # regenerates tmp/*.pdf (prose, math, hello, prose_sample, …)
```

CI (`.github/workflows/ci.yml`, same layout as Tessera): fmt / clippy / test on
Ubuntu · macOS · Windows (+ `--features icons`, `cjk`, `emoji`), plus an MSRV
`cargo check` on 1.95. Path filters skip docs-only pushes. SHA-256 fixtures live in
`tests/determinism.rs`.

## API

```rust
use ariadnes_weave::{
    emit_pdf, emit_pdf_with, BreakHint, EmitOptions, ListItem, PrintBlock, PrintDocument,
    PrintMeta, PrintProfileId, TextRun,
};

// emit_pdf(&doc) == emit_pdf_with(&doc, &EmitOptions::bundled_only())
// Pin a host-loaded TTF:
//   let opts = EmitOptions::bundled_only().with_pinned_face("ui", ttf_bytes);
//   TextRun::pinned("…", "ui")  // or TextRun { face: Some("ui".into()), … }

let doc = PrintDocument {
    meta: PrintMeta {
        title: "Hello".into(),
        doc_kind: "note".into(),
        language: None,
        source_doc_id: None,
    },
    profile: PrintProfileId::print_v0(),
    blocks: vec![
        PrintBlock::Heading {
            level: 1,
            runs: vec![TextRun::plain("Title")],
            break_before: BreakHint::None,
        },
        PrintBlock::Paragraph {
            runs: vec![TextRun::plain("Body.")],
        },
        PrintBlock::List {
            ordered: false,
            items: vec![ListItem {
                runs: vec![TextRun::plain("Item")],
                children: vec![],
            }],
        },
    ],
};
let pdf: Vec<u8> = emit_pdf(&doc)?;
assert!(pdf.starts_with(b"%PDF-"));
```

## License

Dual-licensed under MIT or Apache-2.0.
