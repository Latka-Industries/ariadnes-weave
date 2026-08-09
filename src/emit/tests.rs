//! Unit tests for emit (profiles, tables, slides, math, figures).

use super::math::prettify_latex_math;
use super::{emit_pdf, emit_pdf_with};
use crate::error::WeaveError;
use crate::font::{FaceId, FaceRef, FontBag, shape_text, shaped_width};
use crate::ir::{
    BreakHint, EmAmount, FigurePlacement, InlineStyle, LayoutOp, MeasureFrac, PlaceSkip,
    PrintBlock, PrintDocument, PrintImage, PrintMeta, PrintProfileId, RuleWidth,
    SlideRegionContent, TableRow, TextRun, VspaceAmount,
};
use crate::knobs::{
    CaptionBand, CaptionOverflow, FigureAlign, FigureTextAlign, FigureTitleAlign, HexColor,
    LayoutKnobs, TextAlign,
};
use crate::options::EmitOptions;
use crate::profile::{self, PageSize};
use image::{ImageBuffer, ImageFormat, Rgb};

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
                runs: vec![TextRun {
                    text: "Hello, ariadnes-weave".into(),
                    style: InlineStyle::default(),
                    face: None,
                }],
                break_before: BreakHint::None,
            },
            PrintBlock::Paragraph {
                runs: vec![TextRun::plain(
                    "Owned print IR to PDF with Liberation Sans + rustybuzz.",
                )],
            },
        ],
    }
}

fn rgb_png(w: u32, h: u32, pixel: impl Fn(u32, u32) -> Rgb<u8>) -> Vec<u8> {
    let img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_fn(w, h, pixel);
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, ImageFormat::Png)
        .expect("encode png");
    buf.into_inner()
}

fn png_image(w: u32, h: u32, bytes: Vec<u8>) -> PrintImage {
    PrintImage {
        bytes,
        media_type: "image/png".into(),
        width_px: Some(w),
        height_px: Some(h),
    }
}

fn tiny_png_bytes() -> Vec<u8> {
    rgb_png(32, 24, |x, y| Rgb([x as u8 * 7, y as u8 * 9, 180]))
}

fn tiny_png_image() -> PrintImage {
    png_image(32, 24, tiny_png_bytes())
}

fn note_doc(title: &str, blocks: Vec<PrintBlock>) -> PrintDocument {
    PrintDocument {
        meta: PrintMeta {
            title: title.into(),
            doc_kind: "note".into(),
            language: None,
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks,
    }
}

fn figure_block(
    image: PrintImage,
    caption: impl Into<String>,
    placement: FigurePlacement,
) -> PrintBlock {
    PrintBlock::Figure {
        image,
        alt: "swatch".into(),
        title: Vec::new(),
        caption: vec![TextRun::plain(caption)],
        placement,
    }
}

fn figure_with_caption(caption: impl Into<String>, placement: FigurePlacement) -> PrintBlock {
    figure_block(tiny_png_image(), caption, placement)
}

fn emit_with_layout_tweak(doc: &PrintDocument, tweak: impl FnOnce(&mut LayoutKnobs)) -> Vec<u8> {
    let mut layout = LayoutKnobs::bundled();
    tweak(&mut layout);
    emit_pdf_with(doc, &EmitOptions::bundled_only().with_layout(layout)).expect("emit")
}

#[test]
fn emits_pdf_magic() {
    let bytes = emit_pdf(&hello_doc()).expect("emit");
    assert!(bytes.starts_with(b"%PDF-"));
    assert!(bytes.windows(5).any(|w| w == b"%%EOF"));
}

#[test]
fn prose_word_spacing_is_wider_than_glued_words() {
    let fonts = FontBag::from_pinned(&Default::default()).expect("fonts");
    let face = FaceRef::Bundled(FaceId::SansRegular);
    // Layout keeps trailing spaces from wrap chunks; glued words must be narrower.
    let spaced = shaped_width(&shape_text(&fonts, face, "Hello world", 12.0).expect("shape"));
    let glued = shaped_width(&shape_text(&fonts, face, "Helloworld", 12.0).expect("shape"));
    assert!(
        spaced > glued + 1.0,
        "expected inter-word space advance: spaced={spaced} glued={glued}"
    );

    let doc = PrintDocument {
        meta: PrintMeta {
            title: "Spaces".into(),
            doc_kind: "note".into(),
            language: None,
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks: vec![PrintBlock::Paragraph {
            runs: vec![TextRun::plain("Hello world from weave")],
        }],
    };
    let bytes = emit_pdf(&doc).expect("emit");
    assert!(bytes.starts_with(b"%PDF-"));
}

#[test]
fn emit_pdf_with_bundled_only_matches_emit_pdf() {
    let doc = hello_doc();
    let a = emit_pdf(&doc).expect("emit");
    let b = emit_pdf_with(&doc, &EmitOptions::bundled_only()).expect("emit_with");
    assert_eq!(a, b);
}

#[test]
fn pinned_face_from_emit_options() {
    let opts = EmitOptions::bundled_only().with_pinned_face(
        "mono",
        include_bytes!("../../fonts/LiberationMono-Regular.ttf").to_vec(),
    );
    let doc = PrintDocument {
        meta: PrintMeta {
            title: "Pinned".into(),
            doc_kind: "note".into(),
            language: None,
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks: vec![PrintBlock::Paragraph {
            runs: vec![TextRun::pinned("Pinned mono run", "mono")],
        }],
    };
    let bytes = emit_pdf_with(&doc, &opts).expect("emit pinned");
    assert!(bytes.starts_with(b"%PDF-"));
    // Resource name for first pin is P0.
    assert!(bytes.windows(2).any(|w| w == b"/P"));
}

#[test]
fn unknown_pinned_face_errors() {
    let doc = PrintDocument {
        meta: PrintMeta {
            title: "Bad pin".into(),
            doc_kind: "note".into(),
            language: None,
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks: vec![PrintBlock::Paragraph {
            runs: vec![TextRun::pinned("x", "no-such-face")],
        }],
    };
    let err = emit_pdf_with(&doc, &EmitOptions::default()).expect_err("unknown pin");
    assert!(
        matches!(err, WeaveError::Font(ref msg) if msg.contains("unknown pinned face")),
        "{err:?}"
    );
}

#[test]
fn category_heading_font_pin_without_run_face() {
    let mono = include_bytes!("../../fonts/LiberationMono-Regular.ttf").to_vec();
    let mut layout = LayoutKnobs::bundled();
    layout.prose.heading.font = Some("display".into());
    let opts = EmitOptions::bundled_only()
        .with_layout(layout)
        .with_pinned_face("display", mono);
    let doc = PrintDocument {
        meta: PrintMeta {
            title: "Category heading".into(),
            doc_kind: "note".into(),
            language: None,
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks: vec![PrintBlock::Heading {
            level: 1,
            runs: vec![TextRun::plain("Display heading")],
            break_before: BreakHint::None,
        }],
    };
    let bytes = emit_pdf_with(&doc, &opts).expect("emit category heading font");
    assert!(bytes.starts_with(b"%PDF-"));
    assert!(
        bytes.windows(2).any(|w| w == b"/P"),
        "category heading font should embed pinned resource"
    );
}

#[test]
fn explicit_run_face_wins_over_category_font() {
    let display = include_bytes!("../../fonts/LiberationMono-Regular.ttf").to_vec();
    let other = include_bytes!("../../fonts/LiberationSerif-Regular.ttf").to_vec();
    let mut layout = LayoutKnobs::bundled();
    layout.prose.heading.font = Some("display".into());
    let opts = EmitOptions::bundled_only()
        .with_layout(layout)
        .with_pinned_face("display", display)
        .with_pinned_face("other", other);
    let doc = PrintDocument {
        meta: PrintMeta {
            title: "Override".into(),
            doc_kind: "note".into(),
            language: None,
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks: vec![PrintBlock::Heading {
            level: 1,
            runs: vec![TextRun::pinned("Explicit other", "other")],
            break_before: BreakHint::None,
        }],
    };
    let category_only = {
        let mut layout = LayoutKnobs::bundled();
        layout.prose.heading.font = Some("display".into());
        let opts = EmitOptions::bundled_only()
            .with_layout(layout)
            .with_pinned_face(
                "display",
                include_bytes!("../../fonts/LiberationMono-Regular.ttf").to_vec(),
            );
        let doc = PrintDocument {
            meta: PrintMeta {
                title: "Override".into(),
                doc_kind: "note".into(),
                language: None,
                source_doc_id: None,
            },
            profile: PrintProfileId::print_v0(),
            blocks: vec![PrintBlock::Heading {
                level: 1,
                runs: vec![TextRun::plain("Explicit other")],
                break_before: BreakHint::None,
            }],
        };
        emit_pdf_with(&doc, &opts).expect("category only")
    };
    let overridden = emit_pdf_with(&doc, &opts).expect("explicit wins");
    assert_ne!(
        category_only, overridden,
        "explicit TextRun.face should change PDF vs category default"
    );
    assert!(overridden.windows(2).any(|w| w == b"/P"));
}

#[test]
fn category_cite_font_pin() {
    let mono = include_bytes!("../../fonts/LiberationMono-Regular.ttf").to_vec();
    let mut layout = LayoutKnobs::bundled();
    layout.prose.cite.font = Some("cite-face".into());
    let opts = EmitOptions::bundled_only()
        .with_layout(layout)
        .with_pinned_face("cite-face", mono);
    let doc = PrintDocument {
        meta: PrintMeta {
            title: "Cite font".into(),
            doc_kind: "note".into(),
            language: None,
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks: vec![PrintBlock::Paragraph {
            runs: vec![TextRun {
                text: "[1]".into(),
                style: InlineStyle {
                    cite: true,
                    ..InlineStyle::default()
                },
                face: None,
            }],
        }],
    };
    let bytes = emit_pdf_with(&doc, &opts).expect("cite category font");
    assert!(bytes.windows(2).any(|w| w == b"/P"));
}

#[test]
fn unknown_category_font_errors() {
    let mut layout = LayoutKnobs::bundled();
    layout.prose.text.font = Some("no-such-face".into());
    let doc = PrintDocument {
        meta: PrintMeta {
            title: "Bad category".into(),
            doc_kind: "note".into(),
            language: None,
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks: vec![PrintBlock::Paragraph {
            runs: vec![TextRun::plain("x")],
        }],
    };
    let err = emit_pdf_with(&doc, &EmitOptions::bundled_only().with_layout(layout))
        .expect_err("unknown category pin");
    assert!(
        matches!(err, WeaveError::Font(ref msg) if msg.contains("unknown pinned face")),
        "{err:?}"
    );
}

#[test]
fn os_with_fallback_requires_feature() {
    let doc = PrintDocument {
        meta: PrintMeta {
            title: "OS".into(),
            doc_kind: "note".into(),
            language: None,
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks: vec![PrintBlock::Paragraph {
            runs: vec![TextRun::plain("hi")],
        }],
    };
    let err = emit_pdf_with(&doc, &EmitOptions::os_with_fallback());
    #[cfg(feature = "os-fonts")]
    {
        assert!(err.is_ok(), "os-fonts enabled: {err:?}");
    }
    #[cfg(not(feature = "os-fonts"))]
    {
        let err = err.expect_err("feature gate");
        assert!(
            matches!(err, WeaveError::Font(ref msg) if msg.contains("os-fonts")),
            "{err:?}"
        );
    }
}

#[cfg(feature = "os-fonts")]
#[test]
fn os_missing_family_falls_back_to_liberation() {
    let doc = PrintDocument {
        meta: PrintMeta {
            title: "OS fallback".into(),
            doc_kind: "note".into(),
            language: None,
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks: vec![PrintBlock::Paragraph {
            runs: vec![TextRun::pinned(
                "still emits",
                "DefinitelyNotARealFontFamily_ariadnes_weave_311",
            )],
        }],
    };
    let bytes = emit_pdf_with(&doc, &EmitOptions::os_with_fallback()).expect("fallback emit");
    assert!(bytes.starts_with(b"%PDF-"));
}

#[test]
fn subsetted_hello_pdf_is_compact() {
    let bytes = emit_pdf(&hello_doc()).expect("emit");
    // Full Liberation Sans alone is ~400KB; subset prose should stay well under that.
    assert!(
        bytes.len() < 80_000,
        "expected subsetted PDF < 80KB, got {}",
        bytes.len()
    );
}

#[test]
fn hard_breaks_overlong_token() {
    let long = "A".repeat(80);
    let doc = PrintDocument {
        meta: PrintMeta {
            title: "Long".into(),
            doc_kind: "note".into(),
            language: None,
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks: vec![PrintBlock::Paragraph {
            runs: vec![TextRun::plain(long)],
        }],
    };
    let bytes = emit_pdf(&doc).expect("emit overlong");
    assert!(bytes.starts_with(b"%PDF-"));
}

#[test]
fn rejects_unknown_profile() {
    let mut doc = hello_doc();
    doc.profile = PrintProfileId {
        name: "manuscript".into(),
        version: 1,
    };
    assert!(matches!(
        emit_pdf(&doc).unwrap_err(),
        WeaveError::UnsupportedProfile { .. }
    ));
}

#[test]
fn accepts_manuscript_v0() {
    let mut doc = hello_doc();
    doc.profile = PrintProfileId::manuscript_v0();
    assert!(emit_pdf(&doc).expect("emit").starts_with(b"%PDF-"));
}

#[test]
fn accepts_print_letter_v0_us_letter() {
    let mut doc = hello_doc();
    doc.profile = PrintProfileId::print_letter_v0();
    let bytes = emit_pdf(&doc).expect("emit");
    let (w, h) = PageSize::UsLetter.dimensions();
    let text = String::from_utf8_lossy(&bytes);
    assert!(
        text.contains(&format!("/MediaBox [0 0 {} {}]", w as i32, h as i32)),
        "print-letter@0 must use US Letter MediaBox"
    );
}

#[test]
fn styled_runs_embed_bold_font() {
    let mut doc = hello_doc();
    doc.blocks.push(PrintBlock::Paragraph {
        runs: vec![
            TextRun::plain("plain "),
            TextRun {
                text: "bold".into(),
                style: InlineStyle {
                    strong: true,
                    ..InlineStyle::default()
                },
                face: None,
            },
        ],
    });
    let bytes = emit_pdf(&doc).expect("emit");
    let s = String::from_utf8_lossy(&bytes);
    assert!(s.contains("LiberationSans-Bold"));
    assert!(s.contains("/Subtype /Type0") || s.contains("/Subtype/Type0"));
}

#[test]
fn emits_unicode_em_dash() {
    let doc = PrintDocument {
        meta: PrintMeta {
            title: "Dash".into(),
            doc_kind: "note".into(),
            language: None,
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks: vec![PrintBlock::Paragraph {
            runs: vec![TextRun::plain("alpha — omega")],
        }],
    };
    let bytes = emit_pdf(&doc).expect("emit unicode");
    assert!(bytes.starts_with(b"%PDF-"));
}

#[test]
fn table_draws_grid_paths() {
    let doc = PrintDocument {
        meta: PrintMeta {
            title: "T".into(),
            doc_kind: "note".into(),
            language: None,
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks: vec![PrintBlock::Table {
            rows: vec![
                TableRow {
                    cells: vec!["Name".into(), "Value".into()],
                },
                TableRow {
                    cells: vec!["alpha".into(), "1".into()],
                },
            ],
        }],
    };
    let bytes = emit_pdf(&doc).expect("emit");
    assert!(bytes.starts_with(b"%PDF-"));
    let s = String::from_utf8_lossy(&bytes);
    assert!(s.contains("LiberationSans"));
    // Table grid sets a distinctive stroke width before path ops.
    assert!(
        s.contains("0.6 w"),
        "expected table stroke width in content stream"
    );
}

#[test]
fn slide_emits_own_page() {
    let doc = PrintDocument {
        meta: PrintMeta {
            title: "Deck".into(),
            doc_kind: "deck".into(),
            language: None,
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks: vec![
            PrintBlock::Paragraph {
                runs: vec![TextRun::plain("Before slides.")],
            },
            PrintBlock::Slide {
                layout_id: "title-body".into(),
                regions: vec![
                    SlideRegionContent {
                        slot: "title".into(),
                        text: "First slide".into(),
                    },
                    SlideRegionContent {
                        slot: "body".into(),
                        text: "Bullet ideas live here.".into(),
                    },
                ],
            },
            PrintBlock::Slide {
                layout_id: "title-body".into(),
                regions: vec![SlideRegionContent {
                    slot: "title".into(),
                    text: "Second slide".into(),
                }],
            },
            PrintBlock::Paragraph {
                runs: vec![TextRun::plain("After slides.")],
            },
        ],
    };
    let bytes = emit_pdf(&doc).expect("emit");
    let page_dicts = bytes.windows(10).filter(|w| *w == b"/Type /Pag").count();
    assert!(
        page_dicts >= 3,
        "prose + 2 slides should span >= 3 pages; got {page_dicts}"
    );
}

#[test]
fn manuscript_emphasis_uses_serif_italic() {
    let doc = PrintDocument {
        meta: PrintMeta {
            title: "MS".into(),
            doc_kind: "manuscript".into(),
            language: None,
            source_doc_id: None,
        },
        profile: PrintProfileId::manuscript_v0(),
        blocks: vec![PrintBlock::Paragraph {
            runs: vec![
                TextRun::plain("plain "),
                TextRun {
                    text: "emph".into(),
                    style: InlineStyle {
                        emphasis: true,
                        ..InlineStyle::default()
                    },
                    face: None,
                },
                TextRun::plain(" "),
                TextRun {
                    text: "strong".into(),
                    style: InlineStyle {
                        strong: true,
                        ..InlineStyle::default()
                    },
                    face: None,
                },
            ],
        }],
    };
    let bytes = emit_pdf(&doc).expect("emit");
    let s = String::from_utf8_lossy(&bytes);
    assert!(s.contains("LiberationSerif-Italic"));
    assert!(s.contains("LiberationSerif-Bold"));
}

#[test]
fn quote_body_italic_knob() {
    let doc = quote_only_doc("Quoted body for italic knob.");
    let italic_pdf = emit_pdf(&doc).expect("default italic");
    let roman_pdf = emit_quote_with_italic(&doc, false);

    let italic_s = String::from_utf8_lossy(&italic_pdf);
    assert!(
        italic_s.contains("LiberationSans-Italic"),
        "default [quote].italic should embed italic face for quote body"
    );
    assert_ne!(
        italic_pdf, roman_pdf,
        "default italic quote PDF should differ from italic = false"
    );

    let roman_s = String::from_utf8_lossy(&roman_pdf);
    // Decorative quote marks still use italic; body uses roman (`LiberationSans`).
    assert!(roman_s.contains("LiberationSans-Italic"));
    assert!(
        embeds_roman_liberation_sans(&roman_s),
        "italic = false should keep quote body on roman LiberationSans"
    );
}

fn quote_only_doc(body: &str) -> PrintDocument {
    PrintDocument {
        meta: PrintMeta {
            title: "Quote".into(),
            doc_kind: "note".into(),
            language: None,
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks: vec![PrintBlock::Quote {
            runs: vec![TextRun::plain(body)],
        }],
    }
}

fn emit_quote_with_italic(doc: &PrintDocument, italic: bool) -> Vec<u8> {
    emit_with_layout_tweak(doc, |layout| {
        layout.prose.quote.italic = italic;
    })
}

fn embeds_roman_liberation_sans(pdf: &str) -> bool {
    // PostScript name is `LiberationSans` (not `…-Regular`); avoid matching `-Italic`/`-Bold`.
    pdf.match_indices("+LiberationSans")
        .any(|(i, needle)| !pdf[i + needle.len()..].starts_with('-'))
}

#[test]
fn aesthetic_colors_and_cite_underline_affect_emit() {
    let doc = PrintDocument {
        meta: PrintMeta {
            title: "Aesthetic".into(),
            doc_kind: "note".into(),
            language: None,
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks: vec![
            PrintBlock::Paragraph {
                runs: vec![TextRun::plain("Body text.")],
            },
            PrintBlock::Quote {
                runs: vec![TextRun::plain("Quoted.")],
            },
            PrintBlock::Paragraph {
                runs: vec![TextRun {
                    text: "[1]".into(),
                    style: InlineStyle {
                        cite: true,
                        ..InlineStyle::default()
                    },
                    face: None,
                }],
            },
        ],
    };

    let baseline = emit_pdf(&doc).expect("baseline");

    let mut layout = LayoutKnobs::bundled();
    layout.prose.text.color = Some(HexColor::parse("#336699").unwrap());
    layout.prose.quote.color = Some(HexColor::parse("#226644").unwrap());
    layout.prose.cite.color = Some(HexColor::parse("#990000").unwrap());
    layout.prose.cite.underline = true;
    let colored =
        emit_pdf_with(&doc, &EmitOptions::bundled_only().with_layout(layout)).expect("colored");

    assert_ne!(baseline, colored);
    let s = String::from_utf8_lossy(&colored);
    // pdf-writer emits components as floats; accept either 0-1 fractions or nearby forms.
    assert!(
        s.contains("0.2") && s.contains("0.4") && s.contains("0.6"),
        "expected text color RGB components in content stream: {s}"
    );
}

#[test]
fn caption_knobs_affect_figure_caption_paint() {
    let doc = PrintDocument {
        meta: PrintMeta {
            title: "Caption knobs".into(),
            doc_kind: "note".into(),
            language: None,
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks: vec![figure_with_caption(
            "Figure caption.",
            FigurePlacement::Flow,
        )],
    };

    let baseline = emit_pdf(&doc).expect("baseline");
    let baseline_s = String::from_utf8_lossy(&baseline);
    assert!(
        baseline_s.contains("LiberationSans-Italic"),
        "default [caption].italic should italicize figure captions"
    );

    let mut layout = LayoutKnobs::bundled();
    layout.prose.caption.color = Some(HexColor::parse("#336699").unwrap());
    layout.prose.caption.italic = false;
    layout.prose.caption.size_factor = 1.2;
    let styled =
        emit_pdf_with(&doc, &EmitOptions::bundled_only().with_layout(layout)).expect("styled");

    assert_ne!(baseline, styled);
    let s = String::from_utf8_lossy(&styled);
    assert!(
        s.contains("0.2") && s.contains("0.4") && s.contains("0.6"),
        "expected caption color RGB components in content stream: {s}"
    );
    assert!(
        embeds_roman_liberation_sans(&s),
        "caption.italic = false should keep caption body on roman LiberationSans"
    );
}

#[test]
fn figure_align_and_max_width_factor_affect_emit() {
    // Mid-width: narrower than full content (~403pt) so align shifts x, but wide
    // enough that max_width_factor 0.4 forces fit_width to scale.
    let image = png_image(
        280,
        80,
        rgb_png(280, 80, |x, y| Rgb([(x % 256) as u8, (y % 256) as u8, 90])),
    );
    let doc = note_doc(
        "Figure align",
        vec![figure_block(
            image,
            "Aligned caption.",
            FigurePlacement::Flow,
        )],
    );

    let baseline = emit_pdf(&doc).expect("baseline");

    let left_pdf = emit_with_layout_tweak(&doc, |layout| {
        layout.prose.figure.align = FigureAlign::Left;
    });
    assert_ne!(
        baseline, left_pdf,
        "figure.align = left should change emit vs bundled center"
    );

    let narrow_pdf = emit_with_layout_tweak(&doc, |layout| {
        layout.prose.figure.max_width_factor = 0.4;
    });
    assert_ne!(
        baseline, narrow_pdf,
        "figure.max_width_factor < 1 should change emit"
    );

    let right_pdf = emit_with_layout_tweak(&doc, |layout| {
        layout.prose.figure.align = FigureAlign::Right;
        layout.prose.figure.max_width_factor = 0.5;
    });
    assert_ne!(left_pdf, right_pdf);
    assert_ne!(narrow_pdf, right_pdf);
}

#[test]
fn figure_gap_after_title_affects_emit() {
    let image = png_image(
        280,
        80,
        rgb_png(280, 80, |x, y| Rgb([(x % 256) as u8, (y % 256) as u8, 3])),
    );
    let doc = note_doc(
        "gap title",
        vec![PrintBlock::Figure {
            image,
            alt: "a".into(),
            title: vec![TextRun::plain("Title")],
            caption: vec![TextRun::plain("Cap")],
            placement: FigurePlacement::Flow,
        }],
    );
    let tight = emit_with_layout_tweak(&doc, |l| l.prose.figure.gap_after_title = 0.0);
    let loose = emit_with_layout_tweak(&doc, |l| l.prose.figure.gap_after_title = 24.0);
    assert_ne!(tight, loose, "gap_after_title should change emit");
}

#[test]
fn figure_title_align_and_caption_band_affect_emit() {
    let image = png_image(
        280,
        80,
        rgb_png(280, 80, |x, y| Rgb([(x % 256) as u8, (y % 256) as u8, 90])),
    );
    let title = TextRun::strong("Figure title on the figure");
    let doc = note_doc(
        "Figure title",
        vec![PrintBlock::Figure {
            image,
            alt: "mid".into(),
            title: vec![title],
            caption: vec![TextRun::plain(
                "Caption under a mid-width image for band checks.",
            )],
            placement: FigurePlacement::Flow,
        }],
    );

    let follow = emit_with_layout_tweak(&doc, |layout| {
        layout.prose.figure.align = FigureAlign::Center;
        layout.prose.figure.title_align = FigureTitleAlign::Follow;
    });
    let title_left = emit_with_layout_tweak(&doc, |layout| {
        layout.prose.figure.align = FigureAlign::Center;
        layout.prose.figure.title_align = FigureTitleAlign::Left;
    });
    assert_ne!(
        follow, title_left,
        "title_align = left should differ from follow when figure is centered"
    );

    let match_band = emit_with_layout_tweak(&doc, |layout| {
        layout.prose.figure.align = FigureAlign::Center;
        layout.prose.figure.max_width_factor = 0.4;
        layout.prose.caption.band = CaptionBand::MatchFigure;
    });
    let full_band = emit_with_layout_tweak(&doc, |layout| {
        layout.prose.figure.align = FigureAlign::Center;
        layout.prose.figure.max_width_factor = 0.4;
        layout.prose.caption.band = CaptionBand::FullMeasure;
    });
    assert_ne!(
        match_band, full_band,
        "caption.band = full_measure should differ from match_figure"
    );

    let follow_text = emit_with_layout_tweak(&doc, |layout| {
        layout.prose.figure.align = FigureAlign::Center;
        layout.prose.figure.max_width_factor = 0.5;
        layout.prose.figure.title_text_align = FigureTextAlign::Follow;
        layout.prose.caption.text_align = FigureTextAlign::Follow;
    });
    let left_text = emit_with_layout_tweak(&doc, |layout| {
        layout.prose.figure.align = FigureAlign::Center;
        layout.prose.figure.max_width_factor = 0.5;
        layout.prose.figure.title_text_align = FigureTextAlign::Left;
        layout.prose.caption.text_align = FigureTextAlign::Left;
    });
    assert_ne!(
        follow_text, left_text,
        "in-band text_align follow (center) should differ from forced left"
    );

    let defaults = emit_pdf(&doc).expect("defaults");
    let title_left = emit_with_layout_tweak(&doc, |layout| {
        layout.prose.figure.title_text_align = FigureTextAlign::Left;
    });
    assert_ne!(
        defaults, title_left,
        "bundled title_text_align = center should differ from left"
    );
}

#[test]
fn caption_and_title_justify_affect_emit() {
    let image = png_image(
        280,
        80,
        rgb_png(280, 80, |x, y| Rgb([(x % 256) as u8, (y % 256) as u8, 90])),
    );
    let doc = note_doc(
        "Justify text_align",
        vec![PrintBlock::Figure {
            image,
            alt: "mid".into(),
            title: vec![TextRun::strong(
                "Figure title with several words for justify gaps",
            )],
            caption: vec![TextRun::plain(
                "Wrapping caption so soft breaks create intermediate lines that should \
                 stretch inter-word gaps under justify while the last line stays left.",
            )],
            placement: FigurePlacement::Flow,
        }],
    );

    let left = emit_with_layout_tweak(&doc, |layout| {
        layout.prose.figure.max_width_factor = 0.42;
        layout.prose.caption.text_align = FigureTextAlign::Left;
        layout.prose.figure.title_text_align = FigureTextAlign::Left;
    });
    let justify = emit_with_layout_tweak(&doc, |layout| {
        layout.prose.figure.max_width_factor = 0.42;
        layout.prose.caption.text_align = FigureTextAlign::Justify;
        layout.prose.figure.title_text_align = FigureTextAlign::Justify;
    });
    assert_ne!(
        left, justify,
        "justify text_align should change emit vs left on wrapping caption/title"
    );
}

#[test]
fn paragraph_text_align_affects_emit() {
    let lorem = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod \
         tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, \
         quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo \
         consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse \
         cillum dolore eu fugiat nulla pariatur.";
    let doc = note_doc(
        "Paragraph align",
        vec![PrintBlock::Paragraph {
            runs: vec![TextRun::plain(lorem)],
        }],
    );

    let left = emit_with_layout_tweak(&doc, |layout| {
        layout.prose.paragraph.text_align = TextAlign::Left;
    });
    let center = emit_with_layout_tweak(&doc, |layout| {
        layout.prose.paragraph.text_align = TextAlign::Center;
    });
    let right = emit_with_layout_tweak(&doc, |layout| {
        layout.prose.paragraph.text_align = TextAlign::Right;
    });
    let justify = emit_with_layout_tweak(&doc, |layout| {
        layout.prose.paragraph.text_align = TextAlign::Justify;
    });
    assert_ne!(left, center, "paragraph center should differ from left");
    assert_ne!(left, right, "paragraph right should differ from left");
    assert_ne!(left, justify, "paragraph justify should differ from left");
}

#[test]
fn caption_overflow_soft_only_differs_from_hard_break() {
    // Narrow band + long unbreakable token exercises overflow policy.
    let image = png_image(
        280,
        40,
        rgb_png(280, 40, |x, y| Rgb([(x % 256) as u8, y as u8, 40])),
    );
    let long = "X".repeat(80);
    let doc = note_doc(
        "Caption overflow",
        vec![PrintBlock::Figure {
            image,
            alt: "mid".into(),
            title: Vec::new(),
            caption: vec![TextRun::plain(long)],
            placement: FigurePlacement::Flow,
        }],
    );

    let hard = emit_with_layout_tweak(&doc, |layout| {
        layout.prose.figure.max_width_factor = 0.2;
        layout.prose.caption.overflow = CaptionOverflow::HardBreak;
    });
    let soft = emit_with_layout_tweak(&doc, |layout| {
        layout.prose.figure.max_width_factor = 0.2;
        layout.prose.caption.overflow = CaptionOverflow::SoftOnly;
    });
    assert_ne!(
        hard, soft,
        "caption.overflow soft_only should change emit vs hard_break"
    );
}

#[test]
fn inline_style_underline_without_cite() {
    let plain = PrintDocument {
        meta: PrintMeta {
            title: "Underline".into(),
            doc_kind: "note".into(),
            language: None,
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks: vec![PrintBlock::Paragraph {
            runs: vec![TextRun::plain("No underline.")],
        }],
    };
    let underlined = PrintDocument {
        meta: plain.meta.clone(),
        profile: plain.profile.clone(),
        blocks: vec![PrintBlock::Paragraph {
            runs: vec![TextRun {
                text: "Underlined.".into(),
                style: InlineStyle {
                    underline: true,
                    ..InlineStyle::default()
                },
                face: None,
            }],
        }],
    };

    let plain_pdf = emit_pdf(&plain).expect("plain");
    let under_pdf = emit_pdf(&underlined).expect("underlined");
    assert_ne!(
        plain_pdf, under_pdf,
        "InlineStyle.underline should change emit without requiring cite"
    );
    // Underline paint strokes a line; plain paragraph body text alone should not.
    let under_s = String::from_utf8_lossy(&under_pdf);
    assert!(
        under_s.contains(" re") || under_s.contains("\nS\n") || under_s.contains(" S\n"),
        "expected stroke ops from underline paint: {under_s}"
    );
}

#[test]
fn figure_png_embeds_xobject() {
    let doc = PrintDocument {
        meta: PrintMeta {
            title: "Fig".into(),
            doc_kind: "note".into(),
            language: None,
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks: vec![
            PrintBlock::Heading {
                level: 1,
                runs: vec![TextRun::plain("With figure")],
                break_before: BreakHint::None,
            },
            figure_with_caption("A tiny PNG.", FigurePlacement::Flow),
        ],
    };
    let bytes = emit_pdf(&doc).expect("emit");
    assert!(bytes.starts_with(b"%PDF-"));
    let s = String::from_utf8_lossy(&bytes);
    assert!(s.contains("/Subtype /Image") || s.contains("/Subtype/Image"));
    std::fs::create_dir_all("tmp").ok();
    std::fs::write("tmp/figure_sample.pdf", &bytes).ok();
}

#[test]
fn deck_v0_is_landscape_16x9() {
    let metrics = profile::resolve_metrics(&PrintProfileId::deck_v0()).expect("deck");
    assert!(metrics.is_deck);
    assert!((metrics.page_w / metrics.page_h - 16.0 / 9.0).abs() < 0.01);
    let doc = PrintDocument {
        meta: PrintMeta {
            title: "Deck".into(),
            doc_kind: "deck".into(),
            language: None,
            source_doc_id: None,
        },
        profile: PrintProfileId::deck_v0(),
        blocks: vec![PrintBlock::Slide {
            layout_id: "title-body".into(),
            regions: vec![
                SlideRegionContent {
                    slot: "title".into(),
                    text: "Landscape deck".into(),
                },
                SlideRegionContent {
                    slot: "body".into(),
                    text: "Large type on 16:9.".into(),
                },
            ],
        }],
    };
    let bytes = emit_pdf(&doc).expect("emit deck");
    assert!(bytes.starts_with(b"%PDF-"));
    std::fs::create_dir_all("tmp").ok();
    std::fs::write("tmp/deck_sample.pdf", &bytes).ok();
}

#[test]
fn two_column_slide_emits_both_sides() {
    let doc = PrintDocument {
        meta: PrintMeta {
            title: "Two column".into(),
            doc_kind: "deck".into(),
            language: None,
            source_doc_id: None,
        },
        profile: PrintProfileId::deck_v0(),
        blocks: vec![PrintBlock::Slide {
            layout_id: "two-column".into(),
            regions: vec![
                SlideRegionContent {
                    slot: "title".into(),
                    text: "Split layout".into(),
                },
                SlideRegionContent {
                    slot: "left".into(),
                    text: "LEFT_COLUMN_UNIQUE".into(),
                },
                SlideRegionContent {
                    slot: "right".into(),
                    text: "RIGHT_COLUMN_UNIQUE".into(),
                },
            ],
        }],
    };
    let bytes = emit_pdf(&doc).expect("emit");
    // Content streams are binary-ish; ensure we did not fall back to labeled prefixes.
    let s = String::from_utf8_lossy(&bytes);
    assert!(
        !s.contains("left:") && !s.contains("right:"),
        "two-column must not label left/right as body slots"
    );
    assert!(bytes.starts_with(b"%PDF-"));
    std::fs::create_dir_all("tmp").ok();
    std::fs::write("tmp/deck_two_column.pdf", &bytes).ok();
}

#[test]
fn unknown_slide_layout_falls_back_to_stack() {
    let doc = PrintDocument {
        meta: PrintMeta {
            title: "Fallback".into(),
            doc_kind: "deck".into(),
            language: None,
            source_doc_id: None,
        },
        profile: PrintProfileId::deck_v0(),
        blocks: vec![PrintBlock::Slide {
            layout_id: "nope-not-a-layout".into(),
            regions: vec![
                SlideRegionContent {
                    slot: "title".into(),
                    text: "Still works".into(),
                },
                SlideRegionContent {
                    slot: "body".into(),
                    text: "Stacked body.".into(),
                },
            ],
        }],
    };
    assert!(emit_pdf(&doc).expect("emit").starts_with(b"%PDF-"));
}

#[test]
fn math_prettify_and_emit() {
    assert_eq!(prettify_latex_math(r"$E = mc^2$"), "E = mc²");
    assert!(prettify_latex_math(r"\alpha + \beta").contains('α'));
    let doc = PrintDocument {
        meta: PrintMeta {
            title: "Math".into(),
            doc_kind: "note".into(),
            language: None,
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks: vec![PrintBlock::Math {
            display: true,
            latex: r"E = mc^2".into(),
        }],
    };
    let bytes = emit_pdf(&doc).expect("emit math");
    assert!(bytes.starts_with(b"%PDF-"));
    let s = String::from_utf8_lossy(&bytes);
    assert!(
        s.contains("LatinModernMath"),
        "math should embed Latin Modern Math"
    );
}

#[test]
fn math_frac_draws_rule() {
    let doc = PrintDocument {
        meta: PrintMeta {
            title: "Frac".into(),
            doc_kind: "note".into(),
            language: None,
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks: vec![PrintBlock::Math {
            display: true,
            latex: r"\frac{a^{10}}{b_{ij}}".into(),
        }],
    };
    let bytes = emit_pdf(&doc).expect("emit frac");
    assert!(bytes.starts_with(b"%PDF-"));
    let s = String::from_utf8_lossy(&bytes);
    // Fraction bar uses a stroked path (line width + move/line).
    assert!(s.contains(" m"), "expected path move in content stream");
    assert!(s.contains(" l"), "expected path line in content stream");
}

#[test]
fn math_pmatrix_emits() {
    let doc = PrintDocument {
        meta: PrintMeta {
            title: "Matrix".into(),
            doc_kind: "note".into(),
            language: None,
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks: vec![PrintBlock::Math {
            display: true,
            latex: r"\begin{pmatrix} 1 & 0 \\ 0 & 1 \end{pmatrix}".into(),
        }],
    };
    let bytes = emit_pdf(&doc).expect("emit matrix");
    assert!(bytes.starts_with(b"%PDF-"));
}

#[test]
fn math_sum_display_limits_emit() {
    let display = PrintDocument {
        meta: PrintMeta {
            title: "Sum display".into(),
            doc_kind: "note".into(),
            language: None,
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks: vec![PrintBlock::Math {
            display: true,
            latex: r"\sum_{i=1}^{n} i".into(),
        }],
    };
    let inline = PrintDocument {
        meta: PrintMeta {
            title: "Sum inline".into(),
            doc_kind: "note".into(),
            language: None,
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks: vec![PrintBlock::Math {
            display: false,
            latex: r"\sum_{i=1}^{n} i".into(),
        }],
    };
    let d = emit_pdf(&display).expect("emit display sum");
    let i = emit_pdf(&inline).expect("emit inline sum");
    assert!(d.starts_with(b"%PDF-"));
    assert!(i.starts_with(b"%PDF-"));
    // Under/over vs side scripts must not produce identical pages.
    assert_ne!(
        d, i,
        "display under/over limits should differ from inline side scripts"
    );
}

#[test]
fn math_prod_and_int_emit() {
    for latex in [
        r"\prod_{k=1}^{n} k",
        r"\int_{0}^{1} x^{2} dx",
        r"\oint_{C} F",
        r"\sum_{n=1}^{\infty} \frac{1}{n^{2}}",
        r"\Delta t = \mathrm{after}",
        r"A \cap B \neq C",
        r"\partial_{t} \rho = \Phi(\chi)",
        r"\sqrt{b^{2} - 4ac}",
        r"x \in A \subseteq B \cup C \neq \emptyset",
        r"\forall x \exists y (x \circ y) \notin \nabla",
        r"A \subset B \supset C \supseteq D \mp E",
        r"\bigcup_{i=1}^{n} A_{i} \bigcap_{j} B_{j} \coprod_{k} C_{k}",
    ] {
        let doc = PrintDocument {
            meta: PrintMeta {
                title: "Op".into(),
                doc_kind: "note".into(),
                language: None,
                source_doc_id: None,
            },
            profile: PrintProfileId::print_v0(),
            blocks: vec![PrintBlock::Math {
                display: true,
                latex: latex.into(),
            }],
        };
        let bytes = emit_pdf(&doc).unwrap_or_else(|e| panic!("emit {latex}: {e}"));
        assert!(bytes.starts_with(b"%PDF-"), "{latex}");
    }
}

#[test]
fn math_int_display_limits_differ_from_inline() {
    let display = PrintDocument {
        meta: PrintMeta {
            title: "Int display".into(),
            doc_kind: "note".into(),
            language: None,
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks: vec![PrintBlock::Math {
            display: true,
            latex: r"\int_{0}^{1} x dx".into(),
        }],
    };
    let inline = PrintDocument {
        meta: PrintMeta {
            title: "Int inline".into(),
            doc_kind: "note".into(),
            language: None,
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks: vec![PrintBlock::Math {
            display: false,
            latex: r"\int_{0}^{1} x dx".into(),
        }],
    };
    let d = emit_pdf(&display).expect("display int");
    let i = emit_pdf(&inline).expect("inline int");
    assert_ne!(
        d, i,
        "display ∫ (larger) should still differ from inline even with side scripts"
    );
}

#[test]
fn float_near_figure_emits() {
    let doc = PrintDocument {
        meta: PrintMeta {
            title: "Float".into(),
            doc_kind: "note".into(),
            language: None,
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks: vec![
            PrintBlock::Paragraph {
                runs: vec![TextRun::plain("See the figure nearby.")],
            },
            figure_with_caption("Caption.", FigurePlacement::FloatNear),
        ],
    };
    assert!(emit_pdf(&doc).expect("emit").starts_with(b"%PDF-"));
}

#[cfg(feature = "cjk")]
#[test]
fn sealed_cjk_fallback_embeds_in_pdf() {
    let doc = PrintDocument {
        meta: PrintMeta {
            title: "CJK".into(),
            doc_kind: "note".into(),
            language: Some("zh".into()),
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks: vec![PrintBlock::Paragraph {
            runs: vec![TextRun::plain("Hello 中文测试")],
        }],
    };
    let bytes = emit_pdf(&doc).expect("emit cjk");
    assert!(bytes.starts_with(b"%PDF-"));
    let s = String::from_utf8_lossy(&bytes);
    assert!(
        s.contains("SealedCjkSans"),
        "expected sealed CJK face in PDF font names"
    );
    std::fs::create_dir_all("tmp").ok();
    std::fs::write("tmp/cjk_sample.pdf", &bytes).ok();
}

#[cfg(feature = "emoji")]
#[test]
fn sealed_emoji_fallback_embeds_in_pdf() {
    let doc = PrintDocument {
        meta: PrintMeta {
            title: "Emoji".into(),
            doc_kind: "note".into(),
            language: None,
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks: vec![PrintBlock::Paragraph {
            runs: vec![TextRun::plain("Hello 😀🔥")],
        }],
    };
    let bytes = emit_pdf(&doc).expect("emit emoji");
    assert!(bytes.starts_with(b"%PDF-"));
    let s = String::from_utf8_lossy(&bytes);
    assert!(
        s.contains("SealedNotoEmoji"),
        "expected sealed emoji face in PDF font names"
    );
    std::fs::create_dir_all("tmp").ok();
    std::fs::write("tmp/emoji_sample.pdf", &bytes).ok();
}

#[test]
fn layout_place_frac_flush_emits() {
    let doc = PrintDocument {
        meta: PrintMeta {
            title: "Layout flush".into(),
            doc_kind: "note".into(),
            language: None,
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks: vec![PrintBlock::Layout {
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
        }],
    };
    let bytes = emit_pdf(&doc).expect("emit layout");
    assert!(bytes.starts_with(b"%PDF-"));
    let s = String::from_utf8_lossy(&bytes);
    assert!(s.contains(" m"), "expected path move for rule");
    assert!(s.contains(" l"), "expected path line for rule");
}

#[test]
fn layout_place_mid_frac_and_em_emits() {
    let doc = PrintDocument {
        meta: PrintMeta {
            title: "Layout place".into(),
            doc_kind: "note".into(),
            language: None,
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks: vec![PrintBlock::Layout {
            ops: vec![
                LayoutOp::Place {
                    skip: PlaceSkip::Frac {
                        frac: MeasureFrac::HALF,
                    },
                    runs: vec![TextRun::plain("mid")],
                },
                LayoutOp::Place {
                    skip: PlaceSkip::Em { em: EmAmount::ONE },
                    runs: vec![TextRun::plain("after em")],
                },
                LayoutOp::Vspace {
                    amount: VspaceAmount::Small,
                },
                LayoutOp::Vspace {
                    amount: VspaceAmount::Big,
                },
                LayoutOp::Vspace {
                    amount: VspaceAmount::Em {
                        em: EmAmount::from_em(1.5),
                    },
                },
                LayoutOp::Rule {
                    width: RuleWidth {
                        frac: Some(MeasureFrac::HALF),
                        em: Some(EmAmount::from_milli(500)),
                    },
                },
            ],
        }],
    };
    let bytes = emit_pdf(&doc).expect("emit layout place");
    assert!(bytes.starts_with(b"%PDF-"));
}

#[test]
fn layout_invalid_frac_errors() {
    let doc = PrintDocument {
        meta: PrintMeta {
            title: "Bad frac".into(),
            doc_kind: "note".into(),
            language: None,
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks: vec![PrintBlock::Layout {
            ops: vec![LayoutOp::Place {
                skip: PlaceSkip::Frac {
                    frac: MeasureFrac::from_bps(10_001),
                },
                runs: vec![TextRun::plain("x")],
            }],
        }],
    };
    let err = emit_pdf(&doc).expect_err("bad frac");
    assert!(matches!(err, WeaveError::InvalidLayoutFrac(10_001)));
}

#[test]
fn layout_empty_rule_width_errors() {
    let doc = PrintDocument {
        meta: PrintMeta {
            title: "Empty rule".into(),
            doc_kind: "note".into(),
            language: None,
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks: vec![PrintBlock::Layout {
            ops: vec![LayoutOp::Rule {
                width: RuleWidth {
                    frac: None,
                    em: None,
                },
            }],
        }],
    };
    let err = emit_pdf(&doc).expect_err("empty rule");
    assert!(matches!(err, WeaveError::EmptyRuleWidth));
}

#[test]
fn measure_frac_try_from_f32() {
    assert_eq!(MeasureFrac::try_from_f32(1.0).unwrap(), MeasureFrac::FULL);
    assert_eq!(MeasureFrac::try_from_f32(0.5).unwrap(), MeasureFrac::HALF);
    assert!(matches!(
        MeasureFrac::try_from_f32(1.5),
        Err(WeaveError::InvalidLayoutFrac(_))
    ));
}
