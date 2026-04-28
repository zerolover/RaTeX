//! Smoke: LaTeX → layout → [`ratex_pdf::render_to_pdf`].

use ratex_layout::{layout, to_display_list, LayoutOptions};
use ratex_parser::parser::parse;
use ratex_pdf::{render_to_pdf, PdfOptions};
use ratex_types::math_style::MathStyle;

fn latex_to_pdf(latex: &str) -> Vec<u8> {
    let nodes = parse(latex).expect("parse LaTeX");
    let lbox = layout(
        &nodes,
        &LayoutOptions::default().with_style(MathStyle::Display),
    );
    let list = to_display_list(&lbox);
    render_to_pdf(&list, &PdfOptions::default()).expect("render_to_pdf")
}

#[test]
fn smoke_fraction_renders_valid_pdf() {
    let pdf = latex_to_pdf(r"\frac{1}{2}");
    assert!(
        pdf.starts_with(b"%PDF-"),
        "expected %PDF- header, got {:?}",
        pdf.get(..12)
            .map(|s| std::str::from_utf8(s).unwrap_or("<binary>"))
    );
    assert!(
        pdf.len() > 256,
        "PDF unexpectedly small: {} bytes",
        pdf.len()
    );
}
