//! Spot-check: body + title + figure caption → `tmp/caption_spotcheck*.pdf`
//!
//! Tessera-shaped IR: body paragraph, title as `Paragraph`+`strong`, figure caption.
//!
//! ```bash
//! cargo run --example caption_spotcheck
//! ```
use ariadnes_weave::{
    EmitOptions, FigureAlign, FigurePlacement, HexColor, InlineStyle, LayoutKnobs, PrintBlock,
    PrintDocument, PrintImage, PrintMeta, PrintProfileId, TextRun, emit_pdf, emit_pdf_with,
};
use image::{ImageBuffer, ImageFormat, Rgb};

fn tiny_png() -> Vec<u8> {
    let img: ImageBuffer<Rgb<u8>, Vec<u8>> =
        ImageBuffer::from_fn(120, 80, |x, y| Rgb([x as u8 * 2, y as u8 * 3, 180]));
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, ImageFormat::Png).unwrap();
    buf.into_inner()
}

fn title_run(text: &str) -> TextRun {
    TextRun {
        text: text.into(),
        style: InlineStyle {
            strong: true,
            ..InlineStyle::default()
        },
        face: None,
    }
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
            },
            // Tessera text-chunk `title` stand-in: Paragraph + strong (not Heading).
            PrintBlock::Paragraph {
                runs: vec![title_run("Figure title (Tessera title chunk → strong)")],
            },
            PrintBlock::Figure {
                image: PrintImage {
                    bytes: tiny_png(),
                    media_type: "image/png".into(),
                    width_px: Some(120),
                    height_px: Some(80),
                },
                alt: "swatch".into(),
                caption: vec![TextRun::plain(
                    "Figure caption (Figure.caption + [caption] knobs).",
                )],
                placement: FigurePlacement::Flow,
            },
            PrintBlock::Paragraph {
                runs: vec![TextRun::plain("Body after the figure.")],
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

fn write_layout(name: &str, doc: &PrintDocument, mut tweak: impl FnMut(&mut LayoutKnobs)) {
    let mut layout = LayoutKnobs::bundled();
    tweak(&mut layout);
    write_pdf(
        name,
        &emit_pdf_with(doc, &EmitOptions::bundled_only().with_layout(layout)).unwrap(),
    );
}

fn main() {
    let d = doc();
    write_pdf("caption_spotcheck_default.pdf", &emit_pdf(&d).unwrap());
    write_layout("caption_spotcheck_roman.pdf", &d, |layout| {
        layout.prose.caption.italic = false;
    });
    write_layout("caption_spotcheck_color.pdf", &d, |layout| {
        layout.prose.caption.color = Some(HexColor::parse("#336699").unwrap());
        layout.prose.caption.size_factor = 1.15;
    });
    write_layout("caption_spotcheck_center_narrow.pdf", &d, |layout| {
        layout.prose.figure.align = FigureAlign::Center;
        layout.prose.figure.max_width_factor = 0.45;
    });
}
