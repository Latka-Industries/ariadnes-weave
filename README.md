# ariadnes-weave

![Rust](https://img.shields.io/badge/rust-1.95-orange.svg)

Native deterministic print engine for [Tessera](https://github.com/Latka-Industries/tessera):
**print IR → PDF**.

HTML + CSS stays Tessera’s browser preview path. This crate owns pagination and
PDF bytes so the same document + print profile yields the same layout (see
Tessera `docs/print_ir.md` / D21).

## Status

**Prose emit (post THI-289):** print IR + standard-font `emit_pdf` for:

* Blocks: `Heading` / `Paragraph` / `List` / `Code` / `Quote` / `Break`
* Inline faces: Helvetica / Bold / Oblique / BoldOblique + Courier for `code`
* Profiles: `print@0` (A4) and `manuscript@0` (US Letter, double-spaced body)
* Forced page breaks + keep-with-next glue for H1/H2; page-number footers
* `Table` → ASCII grid; `Figure` → embedded PNG/JPEG XObject; `Math`/`Slide` placeholders

Not yet: bundled TTFs / rustybuzz shaping, real math/deck layout, float placement.

| Later | Where |
| --- | --- |
| Real pagination, `print@1` / `manuscript@1`, bundled TTFs | THI-294 |
| Tessera `.tes` → print tree | THI-290 |
| Deterministic fixtures / CI | THI-292 |
| Tables, figures, math | THI-291 |

**Bricks:** `pdf-writer` + `rustybuzz` + `ttf-parser` (shaping/TTF wired later;
Helvetica path does not need them yet). Not cosmic-text / krilla for v0.

```bash
mise trust   # rust 1.95 via .mise.toml
cargo test
cargo run --example prose
# tmp/hello_world.pdf, tmp/prose_sample.pdf, tmp/prose_example.pdf
```

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
