# ariadnes-weave

![Rust](https://img.shields.io/badge/rust-1.95-orange.svg)

Native deterministic print engine for [Tessera](https://github.com/Latka-Industries/tessera):
**print IR → PDF**.

HTML + CSS stays Tessera’s browser preview path. This crate owns pagination and
PDF bytes so the same document + print profile yields the same layout (see
Tessera `docs/print_ir.md` / D21).

## Status

Scaffold only (`0.1.0`). Print IR types, layout, and PDF emit land next
(THI-289+).

```bash
mise trust   # rust 1.95 via .mise.toml
cargo check
```

## License

Dual-licensed under MIT or Apache-2.0.
