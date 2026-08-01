//! Deterministic emit fixtures (lean THI-292 slice — no Chromium).

use ariadnes_weave::{
    BreakHint, PrintBlock, PrintDocument, PrintMeta, PrintProfileId, TextRun, emit_pdf,
};
use sha2::{Digest, Sha256};

fn hello_doc() -> PrintDocument {
    PrintDocument {
        meta: PrintMeta {
            title: "Hello Weave".into(),
            doc_kind: "note".into(),
            language: Some("en".into()),
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks: vec![
            PrintBlock::Heading {
                level: 1,
                runs: vec![TextRun::plain("Hello, ariadnes-weave")],
                break_before: BreakHint::None,
            },
            PrintBlock::Paragraph {
                runs: vec![TextRun::plain(
                    "Owned print IR to PDF with Liberation Sans and rustybuzz shaping — unicode ok.",
                )],
            },
        ],
    }
}

fn quadratic_formula_doc() -> PrintDocument {
    PrintDocument {
        meta: PrintMeta {
            title: "Quadratic".into(),
            doc_kind: "note".into(),
            language: Some("en".into()),
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks: vec![
            PrintBlock::Heading {
                level: 1,
                runs: vec![TextRun::plain("Quadratic formula")],
                break_before: BreakHint::None,
            },
            PrintBlock::Math {
                display: true,
                latex: r"x = \frac{-b \pm \sqrt{b^{2} - 4ac}}{2a}".into(),
            },
        ],
    }
}

fn manuscript_two_chapters() -> PrintDocument {
    PrintDocument {
        meta: PrintMeta {
            title: "Manuscript chapters".into(),
            doc_kind: "manuscript".into(),
            language: Some("en".into()),
            source_doc_id: None,
        },
        profile: PrintProfileId::manuscript_v0(),
        blocks: vec![
            PrintBlock::Heading {
                level: 1,
                runs: vec![TextRun::plain("Chapter One")],
                break_before: BreakHint::None,
            },
            PrintBlock::Paragraph {
                runs: vec![TextRun::plain("Body of chapter one.")],
            },
            PrintBlock::Heading {
                level: 1,
                runs: vec![TextRun::plain("Chapter Two")],
                break_before: BreakHint::None,
            },
            PrintBlock::Paragraph {
                runs: vec![TextRun::plain("Body of chapter two.")],
            },
        ],
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest {
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// Pin: bump intentionally when emit layout/fonts change.
const HELLO_PRINT_V0_SHA256: &str =
    "78dff97ab02e9241abb08162fced533cdcfc80e2723f0480232ed89f4095bd71";

/// Pin: bump intentionally when emit layout/fonts change.
const MANUSCRIPT_TWO_CHAPTER_SHA256: &str =
    "22ec4075c78b2651784b9a751a8fce66ab2a2f7507cc587272c383a989f1113a";

/// Pin: bump intentionally when math box layout changes.
const QUADRATIC_FORMULA_SHA256: &str =
    "39f2ba38bd622f8ca627a441832a0b30a85231067c4b471afbfd0395b40e06c8";

#[test]
fn emit_is_byte_identical_across_runs() {
    let a = emit_pdf(&hello_doc()).expect("emit a");
    let b = emit_pdf(&hello_doc()).expect("emit b");
    assert_eq!(
        a, b,
        "PDF bytes must be deterministic for fixed IR + profile"
    );
}

#[test]
fn hello_print_v0_sha256_fixture() {
    let bytes = emit_pdf(&hello_doc()).expect("emit");
    assert_eq!(sha256_hex(&bytes), HELLO_PRINT_V0_SHA256);
}

#[test]
fn manuscript_h1_starts_new_page() {
    let bytes = emit_pdf(&manuscript_two_chapters()).expect("emit");
    let page_dicts = bytes.windows(10).filter(|w| *w == b"/Type /Pag").count();
    assert!(
        page_dicts >= 2,
        "manuscript@0 should page-break before second H1; got {page_dicts} page dicts"
    );
    assert_eq!(sha256_hex(&bytes), MANUSCRIPT_TWO_CHAPTER_SHA256);
}

#[test]
fn quadratic_formula_sha256_fixture() {
    let a = emit_pdf(&quadratic_formula_doc()).expect("emit a");
    let b = emit_pdf(&quadratic_formula_doc()).expect("emit b");
    assert_eq!(a, b, "math PDF must be deterministic");
    assert_eq!(sha256_hex(&a), QUADRATIC_FORMULA_SHA256);
}
