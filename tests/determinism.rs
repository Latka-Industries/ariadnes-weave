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
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn emit_is_byte_identical_across_runs() {
    let a = emit_pdf(&hello_doc()).expect("emit a");
    let b = emit_pdf(&hello_doc()).expect("emit b");
    assert_eq!(a, b, "PDF bytes must be deterministic for fixed IR + profile");
}

#[test]
fn hello_print_v0_sha256_fixture() {
    let bytes = emit_pdf(&hello_doc()).expect("emit");
    let hash = sha256_hex(&bytes);
    // Pin: bump intentionally when emit layout/fonts change.
    const EXPECTED: &str =
        "51833aefd9a209573fc7e15f653f71b8396ffed6b0759dee118d45d8156828b7";
    assert_eq!(hash, EXPECTED);
}

#[test]
fn manuscript_h1_starts_new_page() {
    let bytes = emit_pdf(&manuscript_two_chapters()).expect("emit");
    let page_dicts = bytes.windows(10).filter(|w| *w == b"/Type /Pag").count();
    assert!(
        page_dicts >= 2,
        "manuscript@0 should page-break before second H1; got {page_dicts} page dicts"
    );
    let hash = sha256_hex(&bytes);
    const EXPECTED: &str =
        "dfeac558dba14dca3b0b24cea641c0de87b08422d329b7e6322b46c76b9e328c";
    assert_eq!(hash, EXPECTED);
}
