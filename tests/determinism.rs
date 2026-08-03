//! Deterministic emit fixtures (THI-292) + literary unfolding (THI-295).

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

/// Two H1 chapters under `manuscript@0` — literary unfolding acceptance IR.
///
/// Breaks come from `force_h1_page_break`, not `BreakHint::PageAlways`.
/// Chapter-scoped Tessera export is host-side; weave accepts this IR shape.
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

fn assert_emit_deterministic(doc: &PrintDocument) -> Vec<u8> {
    let a = emit_pdf(doc).expect("emit a");
    let b = emit_pdf(doc).expect("emit b");
    assert_eq!(a, b, "PDF must be byte-identical across runs");
    a
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

fn page_dict_count(pdf: &[u8]) -> usize {
    let needle = b"/Type /Page";
    let mut count = 0;
    let mut i = 0;
    while i + needle.len() <= pdf.len() {
        if pdf[i..i + needle.len()] == *needle {
            // Exclude `/Type /Pages` (catalog).
            if pdf.get(i + needle.len()) != Some(&b's') {
                count += 1;
            }
            i += needle.len();
        } else {
            i += 1;
        }
    }
    count
}

/// Pin: bump intentionally when emit layout/fonts change.
const HELLO_PRINT_V0_SHA256: &str =
    "c616fb5bb6d8634e444541f9dafa9e284e903d1310d4c4c68ad8ef052d6ae9b5";

/// Pin: bump intentionally when manuscript literary layout/fonts change.
const MANUSCRIPT_TWO_CHAPTER_SHA256: &str =
    "56f55a6ff7828c97da27d912362f9941a2aded10f467236edc34584020f2ad43";

/// Pin: bump intentionally when math box layout changes.
const QUADRATIC_FORMULA_SHA256: &str =
    "ddfbe59e163b1c7b93d91c472aa765deb8ff93e801def21d69470c7334206d6b";

#[test]
fn emit_is_byte_identical_across_runs() {
    assert_emit_deterministic(&hello_doc());
}

#[test]
fn hello_print_v0_sha256_fixture() {
    let bytes = emit_pdf(&hello_doc()).expect("emit");
    assert_eq!(sha256_hex(&bytes), HELLO_PRINT_V0_SHA256);
}

/// THI-295: same IR + `manuscript@0` → stable literary unfolding.
#[test]
fn manuscript_literary_unfolding_fixture() {
    let a = assert_emit_deterministic(&manuscript_two_chapters());

    let pages = page_dict_count(&a);
    assert_eq!(
        pages, 2,
        "manuscript@0 should page-break before second H1; got {pages} pages"
    );

    let text = String::from_utf8_lossy(&a);
    assert!(
        text.contains("/MediaBox [0 0 612 792]"),
        "manuscript@0 must use US Letter MediaBox"
    );
    assert!(
        text.contains("LiberationSerif"),
        "manuscript@0 body must embed Liberation Serif"
    );
    assert_eq!(sha256_hex(&a), MANUSCRIPT_TWO_CHAPTER_SHA256);
}

/// Contrast: same chapter IR under `print@0` does not force H1 page breaks.
#[test]
fn print_profile_does_not_force_h1_chapter_break() {
    let mut doc = manuscript_two_chapters();
    doc.profile = PrintProfileId::print_v0();
    let bytes = emit_pdf(&doc).expect("emit");
    assert_eq!(
        page_dict_count(&bytes),
        1,
        "print@0 should keep both H1s on one page for this short IR"
    );
}

#[test]
fn quadratic_formula_sha256_fixture() {
    let a = assert_emit_deterministic(&quadratic_formula_doc());
    assert_eq!(sha256_hex(&a), QUADRATIC_FORMULA_SHA256);
}
