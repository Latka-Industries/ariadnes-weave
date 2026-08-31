//! Multi-figure pagination spotcheck → `tmp/figure_pagination_spotcheck*.pdf`
//!
//! Same PNG asset at several `max_width_factor` sizes with body between figures,
//! so page breaks / FloatNear glue are easy to eyeball.
//!
//! ```bash
//! cargo run --example figure_pagination_spotcheck
//! ```
use ariadnes_weave::{
    EmitOptions, FigureAlign, FigurePlacement, LayoutKnobs, PrintBlock, PrintDocument, PrintImage,
    PrintMeta, PrintProfileId, TextRun, emit_pdf, emit_pdf_with,
};
use image::{ImageBuffer, ImageFormat, Rgb};

#[path = "common/mod.rs"]
mod common;
use common::write_pdf;

/// Shared swatch (pixel size = native display size before `max_width_factor`).
fn shared_png() -> Vec<u8> {
    let img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_fn(400, 220, |x, y| {
        Rgb([
            (x % 256) as u8,
            ((x + y) % 256) as u8,
            ((y * 2) % 256) as u8,
        ])
    });
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, ImageFormat::Png).unwrap();
    buf.into_inner()
}

fn shared_image() -> PrintImage {
    PrintImage {
        bytes: shared_png(),
        media_type: "image/png".into(),
        width_px: Some(400),
        height_px: Some(220),
    }
}

fn para(text: &str) -> PrintBlock {
    PrintBlock::Paragraph {
        runs: vec![TextRun::plain(text)],
        indent: 0,
        text_align: None,
    }
}

fn figure(title: &str, caption: &str, placement: FigurePlacement) -> PrintBlock {
    PrintBlock::figure(
        shared_image(),
        "swatch",
        vec![TextRun::strong(title)],
        vec![TextRun::plain(caption)],
        placement,
    )
}

fn filler(n: usize) -> PrintBlock {
    para(&format!(
        "Filler paragraph {n}: enough body copy between figures to push later stacks onto \
         following pages and show widow/orphan glue around FloatNear. Lorem-ish prose keeps \
         the content height honest without needing a novel."
    ))
}

fn doc() -> PrintDocument {
    PrintDocument {
        meta: PrintMeta {
            title: "Figure pagination spotcheck".into(),
            doc_kind: "note".into(),
            language: None,
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks: vec![
            para(
                "Multi-figure pagination: same asset, different max_width_factor per emit. \
                 Default emit uses bundled center align; variants override size only.",
            ),
            figure(
                "Fig A — full band (factor 1.0 in default)",
                "Full-width band figure; following fillers should leave room for breaks.",
                FigurePlacement::Flow,
            ),
            filler(1),
            filler(2),
            figure(
                "Fig B — Flow after fillers",
                "Second Flow figure; may land mid-page or after a break depending on height.",
                FigurePlacement::Flow,
            ),
            filler(3),
            figure(
                "Fig C — FloatNear (keep with previous)",
                "FloatNear glues to prior filler; pagination should prefer not orphaning this stack.",
                FigurePlacement::FloatNear,
            ),
            filler(4),
            filler(5),
            figure(
                "Fig D — another Flow",
                "Fourth copy of the same PNG; watch for page breaks before/after this block.",
                FigurePlacement::Flow,
            ),
            filler(6),
            figure(
                "Fig E — closing Flow",
                "Last figure; useful to see trailing page occupancy.",
                FigurePlacement::Flow,
            ),
            para("Closing body after the last figure."),
        ],
    }
}

fn write_layout(name: &str, doc: &PrintDocument, tweak: impl FnOnce(&mut LayoutKnobs)) {
    let mut layout = LayoutKnobs::bundled();
    tweak(&mut layout);
    write_pdf(
        name,
        &emit_pdf_with(doc, &EmitOptions::bundled_only().with_layout(layout)).unwrap(),
    );
}

type SpotVariant = (&'static str, fn(&mut LayoutKnobs));

fn main() {
    let d = doc();
    write_pdf(
        "figure_pagination_spotcheck_default.pdf",
        &emit_pdf(&d).unwrap(),
    );

    // Same IR; only display size / align changes — compare page counts / breaks.
    let variants: &[SpotVariant] = &[
        ("full", |layout| {
            layout.prose.figure.max_width_factor = 1.0;
        }),
        ("wide", |layout| {
            layout.prose.figure.max_width_factor = 0.75;
        }),
        ("mid", |layout| {
            layout.prose.figure.max_width_factor = 0.5;
        }),
        ("narrow", |layout| {
            layout.prose.figure.max_width_factor = 0.35;
        }),
        ("left_mid", |layout| {
            layout.prose.figure.align = FigureAlign::Left;
            layout.prose.figure.max_width_factor = 0.5;
        }),
    ];
    for &(name, tweak) in variants {
        write_layout(
            &format!("figure_pagination_spotcheck_{name}.pdf"),
            &d,
            tweak,
        );
    }
}
