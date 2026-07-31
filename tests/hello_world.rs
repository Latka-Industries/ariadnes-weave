//! Hello-world: heading + paragraph → valid `%PDF-` bytes.

use ariadnes_weave::{
    BreakHint, PrintBlock, PrintDocument, PrintMeta, PrintProfileId, TextRun, emit_pdf,
};

#[test]
fn heading_and_paragraph_emit_pdf_magic() {
    let doc = PrintDocument {
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
                    "Owned print IR to PDF. This MVP uses Helvetica; real profiles will pin crate-bundled TTFs.",
                )],
            }, // ASCII-only for Helvetica MVP path
        ],
    };

    let bytes = emit_pdf(&doc).expect("emit_pdf");
    assert!(
        bytes.starts_with(b"%PDF-"),
        "expected PDF magic, got {:?}",
        bytes.get(..8)
    );

    // Optional artifact for local inspection.
    let out = std::path::Path::new("tmp/hello_world.pdf");
    if let Some(parent) = out.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(out, &bytes).expect("write tmp/hello_world.pdf");
}

#[test]
fn serde_roundtrip_print_document() {
    let doc = PrintDocument {
        meta: PrintMeta {
            title: "Serde".into(),
            doc_kind: "note".into(),
            language: None,
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks: vec![PrintBlock::Paragraph {
            runs: vec![TextRun::plain("roundtrip")],
        }],
    };
    let json = serde_json::to_string(&doc).expect("serialize");
    let back: PrintDocument = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back, doc);
}
