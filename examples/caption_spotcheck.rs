//! Spot-check: body + figure title/caption → `tmp/caption_spotcheck*.pdf`
//!
//! Includes caption in-band text align variants (left / center / right / justify)
//! on a wrapping caption, plus figure band knobs.
//!
//! ```bash
//! cargo run --example caption_spotcheck
//! ```
use ariadnes_weave::{
    CaptionBand, CaptionOverflow, EmitOptions, FigureAlign, FigurePlacement, FigureTextAlign,
    FigureTitleAlign, HexColor, LayoutKnobs, PrintBlock, PrintDocument, PrintImage, PrintMeta,
    PrintProfileId, TextRun, emit_pdf, emit_pdf_with,
};
use image::{ImageBuffer, ImageFormat, Rgb};

#[path = "common/mod.rs"]
mod common;
use common::write_pdf;

/// Mid-width (~280pt): align shifts are visible, and `max_width_factor` 0.4 still shrinks.
fn figure_png() -> Vec<u8> {
    let img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_fn(280, 100, |x, y| {
        Rgb([(x % 256) as u8, ((y * 3) % 256) as u8, 160])
    });
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, ImageFormat::Png).unwrap();
    buf.into_inner()
}

fn wrapping_caption() -> TextRun {
    TextRun::plain(
        "Wrapping caption for text_align: left sits on the band start, center balances \
         each line in the band, right packs to the band end, justify stretches gaps \
         across the measure. Same figure asset and narrow max_width so wrap is obvious \
         across several lines.",
    )
}

fn doc() -> PrintDocument {
    PrintDocument {
        meta: PrintMeta {
            title: "Caption spotcheck".into(),
            doc_kind: "note".into(),
            language: None,
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks: vec![
            PrintBlock::Paragraph {
                runs: vec![TextRun::plain(
                    "Body copy for size contrast — ordinary paragraph before the figure.",
                )],
                indent: 0,
            },
            PrintBlock::Figure {
                image: PrintImage {
                    bytes: figure_png(),
                    media_type: "image/png".into(),
                    width_px: Some(280),
                    height_px: Some(100),
                },
                alt: "swatch".into(),
                title: vec![TextRun::strong(
                    "Figure title with enough words that a narrow band wraps and \
                     title_text_align justify can stretch intermediate lines",
                )],
                caption: vec![wrapping_caption()],
                placement: FigurePlacement::Flow,
            },
            PrintBlock::Paragraph {
                runs: vec![TextRun::plain("Body after the figure.")],
                indent: 0,
            },
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

fn caption_text_align(align: FigureTextAlign) -> impl FnOnce(&mut LayoutKnobs) {
    move |layout| {
        layout.prose.figure.max_width_factor = 0.42;
        layout.prose.caption.text_align = align;
    }
}

fn main() {
    let d = doc();
    write_pdf("caption_spotcheck_default.pdf", &emit_pdf(&d).unwrap());

    let variants: &[SpotVariant] = &[
        ("roman", |layout| {
            layout.prose.caption.italic = false;
        }),
        ("color", |layout| {
            layout.prose.caption.color = Some(HexColor::parse("#336699").unwrap());
            layout.prose.caption.size_factor = 1.15;
        }),
        ("left", |layout| {
            layout.prose.figure.align = FigureAlign::Left;
        }),
        ("right", |layout| {
            layout.prose.figure.align = FigureAlign::Right;
        }),
        ("narrow", |layout| {
            layout.prose.figure.max_width_factor = 0.4;
        }),
        ("center_narrow", |layout| {
            layout.prose.figure.max_width_factor = 0.4;
        }),
        ("title_left", |layout| {
            layout.prose.figure.title_align = FigureTitleAlign::Left;
        }),
        ("text_left_on_center", |layout| {
            layout.prose.figure.max_width_factor = 0.45;
            layout.prose.figure.title_text_align = FigureTextAlign::Left;
            layout.prose.caption.text_align = FigureTextAlign::Left;
        }),
        ("caption_full", |layout| {
            layout.prose.figure.max_width_factor = 0.4;
            layout.prose.caption.band = CaptionBand::FullMeasure;
        }),
        ("overflow_soft", |layout| {
            layout.prose.figure.max_width_factor = 0.25;
            layout.prose.caption.overflow = CaptionOverflow::SoftOnly;
        }),
    ];
    for &(name, tweak) in variants {
        write_layout(&format!("caption_spotcheck_{name}.pdf"), &d, tweak);
    }

    // Caption in-band text_align (narrow band so wrap shows the difference).
    for (name, align) in [
        ("caption_text_left", FigureTextAlign::Left),
        ("caption_text_center", FigureTextAlign::Center),
        ("caption_text_right", FigureTextAlign::Right),
        ("caption_text_justify", FigureTextAlign::Justify),
    ] {
        write_layout(
            &format!("caption_spotcheck_{name}.pdf"),
            &d,
            caption_text_align(align),
        );
    }

    write_layout("caption_spotcheck_title_text_justify.pdf", &d, |layout| {
        layout.prose.figure.max_width_factor = 0.42;
        layout.prose.figure.title_text_align = FigureTextAlign::Justify;
        // Multi-word title so justify has gaps to stretch.
    });
}
