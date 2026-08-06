//! Emit the THI-362 layout flush fixture to `tmp/layout_place_flush.pdf`.
//!
//! ```bash
//! cargo run --example layout_flush
//! ```

use ariadnes_weave::{
    LayoutOp, MeasureFrac, PlaceSkip, PrintBlock, PrintDocument, PrintMeta, PrintProfileId,
    RuleWidth, TextRun, VspaceAmount, emit_pdf,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let doc = PrintDocument {
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
            },
        ],
    };

    let bytes = emit_pdf(&doc)?;
    std::fs::create_dir_all("tmp")?;
    std::fs::write("tmp/layout_place_flush.pdf", &bytes)?;
    println!("wrote tmp/layout_place_flush.pdf ({} bytes)", bytes.len());
    Ok(())
}
