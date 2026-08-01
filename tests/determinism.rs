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
    "35e551b906e9f17549ab22b907b29cb9f4ae945c34c1288a8c9c3bac7b86da3a";

/// Pin: bump intentionally when emit layout/fonts change.
const MANUSCRIPT_TWO_CHAPTER_SHA256: &str =
    "4a43228d8f93922ecc43f791d4773d949715713416ddfc0a9f4740acf08015e0";

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
