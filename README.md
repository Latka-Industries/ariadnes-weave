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

## Status

**Prose + structure emit:** print IR + Liberation TTF `emit_pdf` for:

* Blocks: `Heading` / `Paragraph` / `List` / `Code` / `Quote` / `Break` / `Table` / `Figure` / `Slide`
* Faces: Liberation Sans (R/B/I/BI), Serif (R/B/I/BI) for `manuscript@0`, Mono for `code`
* Shaping: `rustybuzz` + Type0 / CIDFontType2 / Identity-H; `subsetter` for used glyphs only
* Profiles: `print@0` (A4), `manuscript@0` (US Letter, double-spaced, H1 page breaks), `deck@0` (16:9 landscape)
* Forced page breaks + keep-with-next + basic widow/orphan glue; page-number footers
* Deterministic emit: sorted font object order + SHA-256 fixtures in `tests/determinism.rs`
* `Table` → drawn grid + wrapped cell text; `Figure` → PNG/JPEG XObject (+ `FloatNear` glue); `Slide` → one page per slide; `Math` → centered prettified LaTeX tokens (not full TeX)

Not yet: real TeX/MathML layout, OS font provider, CJK/emoji sealed packs.

| Later | Where |
| --- | --- |
| Tessera `.tes` → print tree | THI-290 |
| Math layout quality | THI-291 |
| Deck page size / richer regions | THI-293 |
| Literary pagination polish | THI-295 |
| OS + GUI font provider (`EmitOptions` scaffold) | THI-307 |
| Sealed `cjk` / `emoji` packs | THI-308 |
| Real math layout | THI-310 |

**Bricks:** `pdf-writer` + `rustybuzz` + `ttf-parser`. Fonts under `fonts/`
(Liberation + optional Font Awesome Free; SIL OFL). Not cosmic-text / krilla for v0.

**Font packs:** default sealed set = Liberation sans/serif/mono. Opt in with
`--features icons` for Font Awesome Free (Solid / Regular / Brands) as
`FaceId::IconSolid` / `IconRegular` / `IconBrands`. Large script packs stay
behind `cjk` / `emoji` so default binaries stay small. OS/GUI faces are separate
([THI-307](https://linear.app/thicclatka/issue/THI-307);
pack layout [THI-308](https://linear.app/thicclatka/issue/THI-308)).

```bash
mise trust   # rust 1.95 via .mise.toml
cargo test
cargo test --features icons
cargo clippy --all-targets -- -D warnings
cargo run --example prose
# tmp/hello_world.pdf, tmp/prose_sample.pdf, tmp/prose_example.pdf
```

CI (`.github/workflows/ci.yml`): `cargo fmt --check`, clippy `-D warnings`, and
`cargo test` (+ `--features icons`), including SHA-256 fixtures in
`tests/determinism.rs`.

## API

```rust
use ariadnes_weave::{
    emit_pdf, emit_pdf_with, BreakHint, EmitOptions, ListItem, PrintBlock, PrintDocument,
    PrintMeta, PrintProfileId, TextRun,
};
// emit_pdf(&doc) == emit_pdf_with(&doc, &EmitOptions::bundled_only())

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
