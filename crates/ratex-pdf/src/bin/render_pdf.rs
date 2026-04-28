//! Batch-export golden cases to PDF using pdf-writer.

use std::io::{self, BufRead};
use std::path::PathBuf;

use ratex_layout::{layout, to_display_list, LayoutOptions};
use ratex_parser::parser::parse;
use ratex_pdf::{render_to_pdf, PdfOptions};
use ratex_types::math_style::MathStyle;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let output_dir = args
        .iter()
        .position(|a| a == "--output-dir")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| "output_pdf".to_string());

    let device_pixel_ratio = args
        .iter()
        .position(|a| a == "--dpr")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(1.0);

    let font_size = args
        .iter()
        .position(|a| a == "--font-size")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(40.0);
    let show_baseline = args.iter().any(|a| a == "--show-baseline");

    std::fs::create_dir_all(&output_dir).expect("Failed to create output dir");

    let dpr = device_pixel_ratio.clamp(0.01, 16.0);
    let pdf_opts = PdfOptions {
        font_size: font_size * dpr,
        padding: 10.0 * dpr,
        stroke_width: 1.5 * dpr,
        show_baseline,
    };

    let inline = args.iter().any(|a| a == "--inline");
    let style = if inline {
        MathStyle::Text
    } else {
        MathStyle::Display
    };
    let layout_opts = LayoutOptions::default().with_style(style);

    let stdin = io::stdin();
    let mut idx = 0;
    let mut ok_count = 0;
    for line in stdin.lock().lines() {
        let line = line.expect("Failed to read line");
        let expr = line.trim();
        if expr.is_empty() || expr.starts_with('#') {
            continue;
        }

        idx += 1;
        match pdf_formula(expr, &layout_opts, &pdf_opts) {
            Ok(bytes) => {
                let path = PathBuf::from(&output_dir).join(format!("{:04}.pdf", idx));
                std::fs::write(&path, &bytes).expect("Failed to write PDF");
                ok_count += 1;
                println!("OK  {:4} {}", idx, expr);
            }
            Err(e) => {
                eprintln!("ERR {:4} {} — {}", idx, expr, e);
            }
        }
    }

    println!(
        "\nProcessed {} formula(s), wrote {} PDF(s) to {}/",
        idx, ok_count, output_dir
    );
}

fn pdf_formula(
    expr: &str,
    layout_opts: &LayoutOptions,
    pdf_opts: &PdfOptions,
) -> Result<Vec<u8>, String> {
    let ast = parse(expr).map_err(|e| format!("Parse error: {e}"))?;
    let lbox = layout(&ast, layout_opts);
    let display_list = to_display_list(&lbox);
    render_to_pdf(&display_list, pdf_opts).map_err(|e| format!("{e}"))
}
