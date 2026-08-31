//! Richer prose sample: list / code / quote / page breaks (Liberation TTFs).

use ariadnes_weave::{
    BreakHint, InlineStyle, ListItem, PrintBlock, PrintDocument, PrintMeta, PrintProfileId,
    TextRun, emit_pdf,
};

fn prose_doc() -> PrintDocument {
    PrintDocument {
        meta: PrintMeta {
            title: "Prose sample".into(),
            doc_kind: "document".into(),
            language: Some("en".into()),
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks: vec![
            PrintBlock::Heading {
                level: 1,
                runs: vec![TextRun::plain("Chapter One")],
                break_before: BreakHint::None,
                dest_id: None,
            },
            PrintBlock::Paragraph {
                runs: vec![
                    TextRun::plain("A short paragraph with "),
                    TextRun {
                        text: "bold".into(),
                        style: InlineStyle {
                            strong: true,
                            ..InlineStyle::default()
                        },
                        face: None,
                        link_uri: None,
                        note_id: None,
                    },
                    TextRun::plain(" and "),
                    TextRun {
                        text: "italic".into(),
                        style: InlineStyle {
                            emphasis: true,
                            ..InlineStyle::default()
                        },
                        face: None,
                        link_uri: None,
                        note_id: None,
                    },
                    TextRun::plain(" before a list, a quote, and a code block."),
                ],
                indent: 0,
                text_align: None,
            },
            PrintBlock::List {
                ordered: false,
                items: vec![
                    ListItem {
                        runs: vec![TextRun::plain("First item")],
                        children: vec![],
                    },
                    ListItem {
                        runs: vec![TextRun::plain("Second item")],
                        children: vec![PrintBlock::List {
                            ordered: true,
                            items: vec![ListItem {
                                runs: vec![TextRun::plain("Nested numbered")],
                                children: vec![],
                            }],
                            indent: 0,
                            text_align: None,
                        }],
                    },
                    ListItem {
                        runs: vec![TextRun::plain("Third item")],
                        children: vec![],
                    },
                ],
                indent: 0,
                text_align: None,
            },
            PrintBlock::Quote {
                runs: vec![TextRun::plain("Quoted line for the MVP path.")],
                text_align: None,
            },
            PrintBlock::Code {
                lang: Some("rust".into()),
                text: "fn main() {\n    println!(\"weave\");\n}\n".into(),
            },
            PrintBlock::Break(BreakHint::PageAlways),
            PrintBlock::Heading {
                level: 1,
                runs: vec![TextRun::plain("Chapter Two")],
                break_before: BreakHint::PageAlways,
                dest_id: None,
            },
            PrintBlock::Paragraph {
                runs: vec![TextRun::plain("Second page after an explicit break.")],
                indent: 0,
                text_align: None,
            },
        ],
    }
}

#[test]
fn prose_sample_emits_multipage_pdf() {
    let bytes = emit_pdf(&prose_doc()).expect("emit_pdf");
    assert!(bytes.starts_with(b"%PDF-"));
    // Two page dictionaries expected after chapter break.
    let page_count = bytes.windows(10).filter(|w| *w == b"/Type /Pag").count();
    assert!(
        page_count >= 2,
        "expected >= 2 /Type /Page objects, got {page_count}"
    );

    let out = std::path::Path::new("tmp/prose_sample.pdf");
    if let Some(parent) = out.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(out, &bytes).expect("write tmp/prose_sample.pdf");
}

#[test]
fn prose_sample_serde_roundtrip() {
    let doc = prose_doc();
    let json = serde_json::to_string_pretty(&doc).expect("serialize");
    let back: PrintDocument = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back, doc);
    assert!(json.contains("\"kind\": \"list\""));
    assert!(json.contains("\"kind\": \"code\""));
    assert!(json.contains("\"kind\": \"quote\""));
}

#[test]
fn manuscript_profile_emits() {
    let mut doc = prose_doc();
    doc.profile = PrintProfileId::manuscript_v0();
    doc.meta.doc_kind = "manuscript".into();
    let bytes = emit_pdf(&doc).expect("emit");
    assert!(bytes.starts_with(b"%PDF-"));
    let out = std::path::Path::new("tmp/prose_manuscript.pdf");
    if let Some(parent) = out.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(out, &bytes).expect("write tmp/prose_manuscript.pdf");
}
