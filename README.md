# ariadnes-weave

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
* Profiles: `print@0` (A4) and `manuscript@0` (US Letter, double-spaced, H1 page breaks)
* Forced page breaks + keep-with-next + basic widow/orphan glue; page-number footers
* Deterministic emit: sorted font object order + SHA-256 fixtures in `tests/determinism.rs`
* `Table` → drawn grid + wrapped cell text; `Figure` → PNG/JPEG XObject; `Slide` → one page per slide (region slots); `Math` still a placeholder

Not yet: real math glyphs, figure float placement, deck-specific page size profile.

| Later | Where |
| --- | --- |
| Tessera `.tes` → print tree | THI-290 |
| Math layout quality | THI-291 |
| Deck page size / richer regions | THI-293 |
| Literary pagination polish | THI-295 |

**Bricks:** `pdf-writer` + `rustybuzz` + `ttf-parser`. Fonts under `fonts/`
(Liberation, SIL Open Font License). Not cosmic-text / krilla for v0.

```bash
mise trust   # rust 1.95 via .mise.toml
cargo test
cargo clippy --all-targets -- -D warnings
cargo run --example prose
# tmp/hello_world.pdf, tmp/prose_sample.pdf, tmp/prose_example.pdf
```

CI (`.github/workflows/ci.yml`): `cargo fmt --check`, clippy `-D warnings`, and `cargo test`
(including SHA-256 fixtures in `tests/determinism.rs`).

## API

```rust
use ariadnes_weave::{
    emit_pdf, BreakHint, ListItem, PrintBlock, PrintDocument, PrintMeta, PrintProfileId,
    TextRun,
};

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
