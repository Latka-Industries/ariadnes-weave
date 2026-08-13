//! Spot-check: dense long-word paragraphs with hyphenation on vs off.
//!
//! Writes `tmp/hyphen_on.pdf` and `tmp/hyphen_off.pdf` at a high paragraph
//! indent so the wrap measure is narrow and soft hyphens are obvious.
//!
//! ```bash
//! cargo run --example hyphen_spotcheck
//! ```

use ariadnes_weave::{
    EmitOptions, LayoutKnobs, PrintBlock, PrintDocument, PrintMeta, PrintProfileId, TextRun,
    emit_pdf_with,
};

#[path = "common/mod.rs"]
mod common;
use common::write_pdf;

fn dense_doc() -> PrintDocument {
    let para = "The internationalization of antidisestablishmentarianism and \
         supercalifragilisticexpialidocious pneumonoultramicroscopicsilicovolcanoconiosis \
         challenges hyphenation when the measure is deliberately narrow. Repeated long \
         words force soft breaks: incomprehensibility counterrevolutionaries \
         electroencephalographically. More filler prose keeps several lines wrapping \
         across the indented band so on/off hyphenation is easy to eyeball.";
    PrintDocument {
        meta: PrintMeta {
            title: "Hyphen spotcheck".into(),
            doc_kind: "note".into(),
            language: Some("en".into()),
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks: vec![
            PrintBlock::Paragraph {
                runs: vec![TextRun::plain(para)],
                indent: 3,
            },
            PrintBlock::Paragraph {
                runs: vec![TextRun::plain(para)],
                indent: 3,
            },
            PrintBlock::Paragraph {
                runs: vec![TextRun::plain(para)],
                indent: 3,
            },
        ],
    }
}

fn write_variant(name: &str, hyphenate: bool) {
    let doc = dense_doc();
    let mut layout = LayoutKnobs::bundled();
    layout.prose.indent.step = 120.0;
    layout.prose.wrap.hyphenate = hyphenate;
    let bytes =
        emit_pdf_with(&doc, &EmitOptions::bundled_only().with_layout(layout)).expect("emit");
    write_pdf(name, &bytes);
}

fn main() {
    write_variant("hyphen_on.pdf", true);
    write_variant("hyphen_off.pdf", false);
}
