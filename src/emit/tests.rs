//! Unit tests for emit (profiles, tables, slides, math, figures).

use super::math::prettify_latex_math;
use super::{emit_pdf, emit_pdf_with};
use crate::error::WeaveError;
use crate::ir::{
    BreakHint, FigurePlacement, InlineStyle, PrintBlock, PrintDocument, PrintImage, PrintMeta,
    PrintProfileId, SlideRegionContent, TableRow, TextRun,
};
use crate::options::EmitOptions;
use crate::profile;
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

fn tiny_png_bytes() -> Vec<u8> {
    let img: ImageBuffer<Rgb<u8>, Vec<u8>> =
        ImageBuffer::from_fn(32, 24, |x, y| Rgb([x as u8 * 7, y as u8 * 9, 180]));
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, ImageFormat::Png)
        .expect("encode png");
    buf.into_inner()
}

#[test]
fn emits_pdf_magic() {
    let bytes = emit_pdf(&hello_doc()).expect("emit");
    assert!(bytes.starts_with(b"%PDF-"));
    assert!(bytes.windows(5).any(|w| w == b"%%EOF"));
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
fn figure_png_embeds_xobject() {
    let png = tiny_png_bytes();
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
            PrintBlock::Figure {
                image: PrintImage {
                    bytes: png,
                    media_type: "image/png".into(),
                    width_px: Some(32),
                    height_px: Some(24),
                },
                alt: "swatch".into(),
                caption: vec![TextRun::plain("A tiny PNG.")],
                placement: FigurePlacement::Flow,
            },
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
    assert!(s.contains("LiberationSans-Italic") || s.contains("LiberationSerif-Italic"));
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
fn float_near_figure_emits() {
    let png = tiny_png_bytes();
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
            PrintBlock::Figure {
                image: PrintImage {
                    bytes: png,
                    media_type: "image/png".into(),
                    width_px: Some(32),
                    height_px: Some(24),
                },
                alt: "swatch".into(),
                caption: vec![TextRun::plain("Caption.")],
                placement: FigurePlacement::FloatNear,
            },
        ],
    };
    assert!(emit_pdf(&doc).expect("emit").starts_with(b"%PDF-"));
}
