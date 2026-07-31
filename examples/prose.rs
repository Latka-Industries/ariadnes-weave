//! Emit a small prose PDF to `tmp/prose_example.pdf`.
//!
//! ```bash
//! cargo run --example prose
//! ```

use ariadnes_weave::{
    BreakHint, ListItem, PrintBlock, PrintDocument, PrintMeta, PrintProfileId, TextRun, emit_pdf,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let doc = PrintDocument {
        meta: PrintMeta {
            title: "ariadnes-weave prose example".into(),
            doc_kind: "document".into(),
            language: Some("en".into()),
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks: vec![
            PrintBlock::Heading {
                level: 1,
                runs: vec![TextRun::plain("Prose example")],
                break_before: BreakHint::None,
            },
            PrintBlock::Paragraph {
                runs: vec![TextRun::plain(
                    "Hand-built print IR - no Tessera dependency required.",
                )],
            },
            PrintBlock::List {
                ordered: true,
                items: vec![
                    ListItem {
                        runs: vec![TextRun::plain("Heading and paragraph")],
                        children: vec![],
                    },
                    ListItem {
                        runs: vec![TextRun::plain("List, quote, and code")],
                        children: vec![],
                    },
                    ListItem {
                        runs: vec![TextRun::plain("Page breaks via BreakHint")],
                        children: vec![],
                    },
                ],
            },
            PrintBlock::Quote {
                runs: vec![TextRun::plain("Print IR is the PDF source of truth.")],
            },
            PrintBlock::Code {
                lang: Some("text".into()),
                text: "print IR -> ariadnes-weave -> PDF\n".into(),
            },
        ],
    };

    let bytes = emit_pdf(&doc)?;
    std::fs::create_dir_all("tmp")?;
    std::fs::write("tmp/prose_example.pdf", &bytes)?;
    println!(
        "wrote tmp/prose_example.pdf ({} bytes, {})",
        bytes.len(),
        if bytes.starts_with(b"%PDF-") {
            "valid magic"
        } else {
            "missing magic"
        }
    );
    Ok(())
}
