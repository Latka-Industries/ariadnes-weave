//! Deterministic emit fixtures (THI-292) + literary unfolding (THI-295).

use ariadnes_weave::{
    BreakHint, LayoutOp, MeasureFrac, PlaceSkip, PrintBlock, PrintDocument, PrintMeta,
    PrintProfileId, RuleWidth, TextRun, VspaceAmount, emit_pdf,
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
                dest_id: None,
            },
            PrintBlock::Paragraph {
                runs: vec![TextRun::plain(
                    "Owned print IR to PDF with Liberation Sans and rustybuzz shaping — unicode ok.",
                )],
                indent: 0,
                text_align: None,
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
                dest_id: None,
            },
            PrintBlock::Math {
                display: true,
                latex: r"x = \frac{-b \pm \sqrt{b^{2} - 4ac}}{2a}".into(),
            },
        ],
    }
}

/// THI-359: display ∑ with under/over limits.
fn sum_limits_doc() -> PrintDocument {
    PrintDocument {
        meta: PrintMeta {
            title: "Sum limits".into(),
            doc_kind: "note".into(),
            language: Some("en".into()),
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks: vec![
            PrintBlock::Heading {
                level: 1,
                runs: vec![TextRun::plain("Sum with limits")],
                break_before: BreakHint::None,
                dest_id: None,
            },
            PrintBlock::Math {
                display: true,
                latex: r"\sum_{i=1}^{n} i = \frac{n(n+1)}{2}".into(),
            },
        ],
    }
}

/// THI-362: `place frac=1` flush + vspace + full-width rule.
fn layout_place_flush_doc() -> PrintDocument {
    PrintDocument {
        meta: PrintMeta {
            title: "Layout place flush".into(),
            doc_kind: "note".into(),
            language: Some("en".into()),
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks: vec![
            PrintBlock::Paragraph {
                runs: vec![TextRun::plain("Before layout chunk.")],
                indent: 0,
                text_align: None,
            },
            PrintBlock::Layout {
                ops: vec![
                    LayoutOp::Place {
                        skip: PlaceSkip::Frac {
                            frac: MeasureFrac::FULL,
                        },
                        runs: vec![TextRun::plain("▸")],
                    },
                    LayoutOp::Vspace {
                        amount: VspaceAmount::Med,
                    },
                    LayoutOp::Rule {
                        width: RuleWidth::frac(MeasureFrac::FULL),
                    },
                ],
            },
            PrintBlock::Paragraph {
                runs: vec![TextRun::plain("After layout chunk.")],
                indent: 0,
                text_align: None,
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
                dest_id: None,
            },
            PrintBlock::Paragraph {
                runs: vec![TextRun::plain("Body of chapter one.")],
                indent: 0,
                text_align: None,
            },
            PrintBlock::Heading {
                level: 1,
                runs: vec![TextRun::plain("Chapter Two")],
                break_before: BreakHint::None,
                dest_id: None,
            },
            PrintBlock::Paragraph {
                runs: vec![TextRun::plain("Body of chapter two.")],
                indent: 0,
                text_align: None,
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

/// Pin: bump intentionally when emit layout/fonts/version (PDF Producer) change.
const HELLO_PRINT_V0_SHA256: &str =
    "b394657c2c66ce67df0a8378d1cfc7823adeacb29ef1f8f632c09483876b2be9";

/// Pin: bump intentionally when manuscript literary layout/fonts/version change.
const MANUSCRIPT_TWO_CHAPTER_SHA256: &str =
    "3cd4efdd89117f5bce1560e2ad352c845936a2058e0a0fde3ee40bc57d83fee7";

/// Pin: bump intentionally when math box layout/version change.
const QUADRATIC_FORMULA_SHA256: &str =
    "874deb392a9a951737bd4b6311ad0b585f54c845d4fa443ff09ca7daac1c4b9a";

/// Pin: bump intentionally when display ∑/∏ under/over limit layout changes.
const SUM_LIMITS_SHA256: &str = "6fcf00ea19590f855d0e43607570eb9b7c9e91d3213588f2c05452aa849fc336";

/// Pin: bump intentionally when layout place/vspace/rule paint changes.
const LAYOUT_PLACE_FLUSH_SHA256: &str =
    "73224b4e8965a9dba3639dbb4ca432941940ceb47029cbc4bbee838c8b71565a";

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

/// THI-359: display ∑ under/over limits stay byte-stable.
#[test]
fn sum_limits_sha256_fixture() {
    let a = assert_emit_deterministic(&sum_limits_doc());
    assert_eq!(sha256_hex(&a), SUM_LIMITS_SHA256);
}

/// THI-362: `place frac=1` flush paints deterministically with vspace/rule.
#[test]
fn layout_place_flush_sha256_fixture() {
    let a = assert_emit_deterministic(&layout_place_flush_doc());
    assert_eq!(sha256_hex(&a), LAYOUT_PLACE_FLUSH_SHA256);
}
