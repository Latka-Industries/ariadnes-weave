//! Emit prose PDFs to `tmp/prose_example*.pdf`, including paragraph text_align variants.
//!
//! ```bash
//! cargo run --example prose
//! ```
use ariadnes_weave::{
    BreakHint, EmitOptions, LayoutKnobs, ListItem, PrintBlock, PrintDocument, PrintMeta,
    PrintProfileId, TextAlign, TextRun, emit_pdf, emit_pdf_with,
};

const LOREM: &str = "\
Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor \
incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis \
nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. \
Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu \
fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in \
culpa qui officia deserunt mollit anim id est laborum. Sed ut perspiciatis unde \
omnis iste natus error sit voluptatem accusantium doloremque laudantium, totam \
rem aperiam, eaque ipsa quae ab illo inventore veritatis et quasi architecto \
beatae vitae dicta sunt explicabo. Nemo enim ipsam voluptatem quia voluptas sit \
aspernatur aut odit aut fugit, sed quia consequuntur magni dolores eos qui \
ratione voluptatem sequi nesciunt. Neque porro quisquam est, qui dolorem ipsum \
quia dolor sit amet, consectetur, adipisci velit, sed quia non numquam eius \
modi tempora incidunt ut labore et dolore magnam aliquam quaerat voluptatem.";

fn doc() -> PrintDocument {
    PrintDocument {
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
                    "Hand-built print IR — no Tessera dependency required. The long \
                     paragraph below is for eyeballing [paragraph].text_align \
                     (left / center / right / justify).",
                )],
            },
            PrintBlock::Paragraph {
                runs: vec![TextRun::plain(LOREM)],
            },
            PrintBlock::Paragraph {
                runs: vec![TextRun::plain(LOREM)],
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
    }
}

fn write_pdf(name: &str, bytes: &[u8]) {
    let path = format!("tmp/{name}");
    std::fs::create_dir_all("tmp").ok();
    std::fs::write(&path, bytes).unwrap();
    println!("wrote {path} ({} bytes)", bytes.len());
}

fn write_align(name: &str, doc: &PrintDocument, align: TextAlign) {
    let mut layout = LayoutKnobs::bundled();
    layout.prose.paragraph.text_align = align;
    write_pdf(
        name,
        &emit_pdf_with(doc, &EmitOptions::bundled_only().with_layout(layout)).unwrap(),
    );
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let d = doc();
    write_pdf("prose_example.pdf", &emit_pdf(&d)?);

    for (name, align) in [
        ("prose_example_left.pdf", TextAlign::Left),
        ("prose_example_center.pdf", TextAlign::Center),
        ("prose_example_right.pdf", TextAlign::Right),
        ("prose_example_justify.pdf", TextAlign::Justify),
    ] {
        write_align(name, &d, align);
    }
    Ok(())
}
