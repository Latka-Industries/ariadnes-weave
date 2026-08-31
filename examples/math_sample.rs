//! Emit a titled math corpus PDF to `tmp/thi359_math_sample.pdf`.
//!
//! Covers structured layout (frac / scripts / matrices), display-style
//! under/over limits for big ops, and an expanded Unicode token map (THI-359).
//!
//! ```bash
//! cargo run --example math_sample
//! # or: mise samples
//! ```

use ariadnes_weave::{
    BreakHint, PrintBlock, PrintDocument, PrintMeta, PrintProfileId, TextRun, emit_pdf,
};

#[path = "common/mod.rs"]
mod common;
use common::write_pdf;

fn h1(text: &str) -> PrintBlock {
    PrintBlock::Heading {
        level: 1,
        runs: vec![TextRun::plain(text)],
        break_before: BreakHint::None,
        dest_id: None,
    }
}

fn h2(text: &str) -> PrintBlock {
    PrintBlock::Heading {
        level: 2,
        runs: vec![TextRun::plain(text)],
        break_before: BreakHint::None,
        dest_id: None,
    }
}

fn note(text: &str) -> PrintBlock {
    PrintBlock::Paragraph {
        runs: vec![TextRun::plain(text)],
        indent: 0,
        text_align: None,
    }
}

fn math(latex: &str) -> PrintBlock {
    PrintBlock::Math {
        display: true,
        latex: latex.into(),
    }
}

fn math_inline(latex: &str) -> PrintBlock {
    PrintBlock::Math {
        display: false,
        latex: latex.into(),
    }
}

fn push_case(blocks: &mut Vec<PrintBlock>, title: &str, source: &str, latex: &str) {
    blocks.push(h2(title));
    blocks.push(note(source));
    blocks.push(math(latex));
}

fn push_inline_case(blocks: &mut Vec<PrintBlock>, title: &str, source: &str, latex: &str) {
    blocks.push(h2(title));
    blocks.push(note(source));
    blocks.push(math_inline(latex));
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut blocks = vec![
        h1("THI-359 math sample"),
        note(
            "Each section is one layout case. Source LaTeX is in the note under the title. \
             Display ops use LM Math `.v1` glyphs; ∑/∏/⋃… under/over (\\displaylimits); \
             ∫/∮ tip-side scripts (\\nolimits). Matrices: `pmatrix` uses `[paren].style` \
             (bundled `round`); `bmatrix` is always square brackets.",
        ),
    ];

    let display_cases = [
        ("1. Simple superscript", "Source: E = mc^{2}", r"E = mc^{2}"),
        (
            "2. Multi-char scripts",
            "Source: a^{10} + b_{ij} + x^{n+1}",
            r"a^{10} + b_{ij} + x^{n+1}",
        ),
        ("3. Plain fraction", r"Source: \frac{a}{b}", r"\frac{a}{b}"),
        (
            "4. Fraction with scripts",
            r"Source: \frac{a^{10}}{b_{ij}}",
            r"\frac{a^{10}}{b_{ij}}",
        ),
        (
            "5. Quadratic formula",
            r"Source: x = \frac{-b \pm \sqrt{b^{2} - 4ac}}{2a}",
            r"x = \frac{-b \pm \sqrt{b^{2} - 4ac}}{2a}",
        ),
        (
            "6. Greek in a fraction",
            r"Source: \frac{\alpha + \beta}{\gamma}",
            r"\frac{\alpha + \beta}{\gamma}",
        ),
        (
            "7. Greek and operators (flat)",
            r"Source: \alpha + \beta = \gamma \leq \theta \rightarrow \infty",
            r"\alpha + \beta = \gamma \leq \theta \rightarrow \infty",
        ),
        (
            "8. Sum with under/over limits + fraction",
            r"Source: \sum_{i=1}^{n} i = \frac{n(n+1)}{2}",
            r"\sum_{i=1}^{n} i = \frac{n(n+1)}{2}",
        ),
        (
            "9. Product with under/over limits",
            r"Source: \prod_{k=1}^{n} k = n!",
            r"\prod_{k=1}^{n} k = n!",
        ),
        (
            "10. Integral with tip-side limits (TeX \\nolimits + display .v1)",
            r"Source: \int_{0}^{1} x^{2}\,dx",
            r"\int_{0}^{1} x^{2} dx",
        ),
        (
            "11. Contour integral",
            r"Source: \oint_{C}^{\gamma} F \cdot dr",
            r"\oint_{C}^{\gamma} F \cdot dr",
        ),
        (
            "12. Nested sum of a fraction",
            r"Source: \sum_{n=1}^{\infty} \frac{1}{n^{2}}",
            r"\sum_{n=1}^{\infty} \frac{1}{n^{2}}",
        ),
        (
            "13. Double sum",
            r"Source: \sum_{i=1}^{m} \sum_{j=1}^{n} a_{ij}",
            r"\sum_{i=1}^{m} \sum_{j=1}^{n} a_{ij}",
        ),
        (
            "14. Capitals + mathrm",
            r"Source: \Delta t = \mathrm{after} - \mathrm{before}",
            r"\Delta t = \mathrm{after} - \mathrm{before}",
        ),
        (
            "15. Sets + membership (geo glyphs)",
            r"Source: x \in A \subseteq B \cup C \neq \emptyset",
            r"x \in A \subseteq B \cup C \neq \emptyset",
        ),
        (
            "16. Partial derivative",
            r"Source: \partial_{t} \rho = \Phi(\chi)",
            r"\partial_{t} \rho = \Phi(\chi)",
        ),
        (
            "17. Square root vinculum",
            r"Source: \sqrt{b^{2} - 4ac}",
            r"\sqrt{b^{2} - 4ac}",
        ),
        (
            "18. More Greek + arrows",
            r"Source: \Phi \Leftrightarrow \Psi \to \Omega",
            r"\Phi \Leftrightarrow \Psi \to \Omega",
        ),
        (
            "19. Logic + nabla + circ",
            r"Source: \forall x \exists y\ (x \circ y) \notin \nabla",
            r"\forall x \exists y (x \circ y) \notin \nabla",
        ),
        (
            "20. Subset family + mp",
            r"Source: A \subset B \supset C \supseteq D \mp E",
            r"A \subset B \supset C \supseteq D \mp E",
        ),
        (
            "21. Big cup/cap/coprod limits",
            r"Source: \bigcup_{i=1}^{n} A_i \bigcap_{j} B_j \coprod_{k} C_k",
            r"\bigcup_{i=1}^{n} A_{i} \bigcap_{j} B_{j} \coprod_{k} C_{k}",
        ),
        (
            "22. Undelimited matrix",
            r"Source: \begin{matrix} 1 & 0 \\ 0 & 1 \end{matrix}",
            r"\begin{matrix} 1 & 0 \\ 0 & 1 \end{matrix}",
        ),
        (
            "23. pmatrix 2×2 (round fences via [paren].style)",
            r"Source: \begin{pmatrix} a & b \\ c & d \end{pmatrix}",
            r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}",
        ),
        (
            "24. bmatrix 3×3 (square fences)",
            r"Source: \begin{bmatrix} 1 & 2 & 3 \\ 4 & 5 & 6 \\ 7 & 8 & 9 \end{bmatrix}",
            r"\begin{bmatrix} 1 & 2 & 3 \\ 4 & 5 & 6 \\ 7 & 8 & 9 \end{bmatrix}",
        ),
        (
            "25. Fraction of a pmatrix",
            r"Source: M = \frac{1}{2}\begin{pmatrix} a & b \\ c & d \end{pmatrix}",
            r"M = \frac{1}{2}\begin{pmatrix} a & b \\ c & d \end{pmatrix}",
        ),
        (
            "26. Polynomial with scripts",
            "Source: f(x) = x^{2} + 2x + 1",
            r"f(x) = x^{2} + 2x + 1",
        ),
    ];
    for (title, source, latex) in display_cases {
        push_case(&mut blocks, title, source, latex);
    }

    blocks.push(h1("Inline contrast"));
    blocks.push(note(
        "Inline math keeps big-op side scripts (TeX textstyle), for contrast with display above.",
    ));
    push_inline_case(
        &mut blocks,
        "27. Inline sum (side scripts)",
        r"Source (inline): \sum_{i=1}^{n} i",
        r"\sum_{i=1}^{n} i",
    );
    push_inline_case(
        &mut blocks,
        "28. Inline integral (side scripts)",
        r"Source (inline): \int_{0}^{1} x dx",
        r"\int_{0}^{1} x dx",
    );
    push_inline_case(
        &mut blocks,
        "29. Inline geo sets",
        r"Source (inline): a \in A \cup B",
        r"a \in A \cup B",
    );

    let doc = PrintDocument {
        meta: PrintMeta {
            title: "THI-359 math sample".into(),
            doc_kind: "note".into(),
            language: Some("en".into()),
            source_doc_id: None,
        },
        profile: PrintProfileId::print_v0(),
        blocks,
    };

    let bytes = emit_pdf(&doc)?;
    write_pdf("thi359_math_sample.pdf", &bytes);
    Ok(())
}
