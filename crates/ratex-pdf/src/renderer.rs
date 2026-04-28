//! Core rendering: convert a [`DisplayList`] into PDF bytes via pdf-writer.
//!
//! Two-pass architecture:
//! 1. Collect all glyphs used across the display list.
//! 2. Subset & embed fonts, then write the content stream.

use std::collections::HashMap;

use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect, Ref, Str};
use ratex_font::FontId;
use ratex_types::color::Color;
use ratex_types::display_item::{DisplayItem, DisplayList};
use ratex_types::path_command::PathCommand;

use crate::fonts::{self, EmbeddedFont};

/// Options controlling PDF output.
#[derive(Debug, Clone)]
pub struct PdfOptions {
    /// User units per em. Default: 40.
    pub font_size: f64,
    /// Padding on all sides, in user units. Default: 10.
    pub padding: f64,
    /// Stroke width for unfilled paths, in user units. Default: 1.5.
    pub stroke_width: f64,
    /// If true, draw a short thin baseline marker at the formula's baseline,
    /// matching the style of LaTeX `\showbaseline`.
    /// Default: false.
    pub show_baseline: bool,
}

impl Default for PdfOptions {
    /// Numeric fields match typical CLI defaults.
    fn default() -> Self {
        Self {
            font_size: 40.0,
            padding: 10.0,
            stroke_width: 1.5,
            show_baseline: false,
        }
    }
}

/// Errors that can occur during PDF rendering.
#[derive(Debug)]
pub enum PdfError {
    Font(String),
    Render(String),
}

impl std::fmt::Display for PdfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PdfError::Font(s) => write!(f, "Font error: {s}"),
            PdfError::Render(s) => write!(f, "Render error: {s}"),
        }
    }
}

impl std::error::Error for PdfError {}

const PDF_PT_PER_CM: f64 = 72.0 / 2.54;
const DEFAULT_TEX_TEXT_PT: f64 = 10.0;
const BASELINE_MARKER_LEN_EM: f64 = (0.1 * PDF_PT_PER_CM) / DEFAULT_TEX_TEXT_PT;
const BASELINE_MARKER_WIDTH_EM: f64 = 0.1 / DEFAULT_TEX_TEXT_PT;

/// Render a [`DisplayList`] to a PDF byte buffer.
pub fn render_to_pdf(
    display_list: &DisplayList,
    options: &PdfOptions,
) -> Result<Vec<u8>, PdfError> {
    let em = options.font_size;
    let pad = options.padding;
    let sw = options.stroke_width;

    let total_h = display_list.height + display_list.depth;
    let page_w = display_list.width * em + 2.0 * pad;
    let page_h = total_h * em + 2.0 * pad;

    // Load raw font data (lazy: only fonts referenced by this display list).
    let font_data =
        ratex_font_loader::load_fonts_for_items("", &display_list.items).map_err(PdfError::Font)?;

    // Pass 1: collect glyph usage across the display list.
    let font_usages = fonts::collect_glyph_usage(&display_list.items, &font_data);

    // Build the PDF.
    let mut pdf = Pdf::new();
    let mut alloc = Ref::new(1);

    let catalog_ref = alloc.bump();
    let pages_ref = alloc.bump();
    let page_ref = alloc.bump();
    let content_ref = alloc.bump();

    // Pass 2: embed the used fonts.
    let embedded = fonts::embed_fonts(&mut pdf, &mut alloc, &font_usages, &font_data)
        .map_err(PdfError::Font)?;

    // Build lookup: FontId → EmbeddedFont index.
    let font_index: HashMap<FontId, usize> = embedded
        .iter()
        .enumerate()
        .map(|(i, ef)| (ef.font_id, i))
        .collect();

    // Generate content stream.
    let content_bytes = build_content_stream(
        display_list,
        &embedded,
        &font_index,
        &font_data,
        em,
        pad,
        page_h,
        sw,
        options.show_baseline,
    );

    // Compress content stream.
    let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&content_bytes, 6);

    // Write content stream object.
    let mut stream = pdf.stream(content_ref, &compressed);
    stream.filter(Filter::FlateDecode);
    stream.pair(Name(b"Length1"), content_bytes.len() as i32);
    stream.finish();

    // Page object.
    let mut page = pdf.page(page_ref);
    page.parent(pages_ref);
    page.media_box(Rect::new(0.0, 0.0, page_w as f32, page_h as f32));
    page.contents(content_ref);

    // Page Resources: font dictionary.
    let mut resources = page.resources();
    let mut font_dict = resources.fonts();
    for ef in &embedded {
        font_dict.pair(Name(ef.res_name.as_bytes()), ef.type0_ref);
    }
    font_dict.finish();
    resources.finish();
    page.finish();

    // Pages node.
    let mut pages = pdf.pages(pages_ref);
    pages.count(1);
    pages.kids([page_ref]);
    pages.finish();

    // Catalog.
    pdf.catalog(catalog_ref).pages(pages_ref);

    Ok(pdf.finish())
}

// ---------------------------------------------------------------------------
// Content stream generation
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn build_content_stream(
    display_list: &DisplayList,
    embedded: &[EmbeddedFont],
    font_index: &HashMap<FontId, usize>,
    font_data: &fonts::RawFontData,
    em: f64,
    pad: f64,
    page_h: f64,
    stroke_width: f64,
    show_baseline: bool,
) -> Vec<u8> {
    let items = &display_list.items;
    let mut content = Content::new();

    // Draw a short baseline marker aligned with the formula baseline.
    // The marker scales with the formula font size so the visual proportion matches
    // LaTeX's `\rule{0.1cm}{0.1pt}` at the default 10pt text size.
    if show_baseline {
        let baseline_y = display_list.height * em + pad;
        let line_y = flip_y(baseline_y, page_h);
        let marker_width = (em * BASELINE_MARKER_WIDTH_EM) as f32;
        let marker_len = em * BASELINE_MARKER_LEN_EM;

        content.set_line_width(marker_width);
        content.set_stroke_rgb(0.0, 0.0, 0.0);

        let start_x = pad as f32;
        let end_x = (pad + marker_len) as f32;

        content.move_to(start_x, line_y);
        content.line_to(end_x, line_y);
        content.stroke();
    }

    for item in items {
        match item {
            DisplayItem::GlyphPath {
                x,
                y,
                scale,
                font,
                char_code,
                color,
                ..
            } => {
                emit_glyph(
                    &mut content,
                    *x * em + pad,
                    *y * em + pad,
                    font,
                    *char_code,
                    *scale,
                    color,
                    em,
                    page_h,
                    embedded,
                    font_index,
                    font_data,
                );
            }
            DisplayItem::Line {
                x,
                y,
                width,
                thickness,
                color,
                dashed,
            } => {
                emit_line(
                    &mut content,
                    &LineParams {
                        x: *x * em + pad,
                        y: *y * em + pad,
                        width: *width * em,
                        thickness: *thickness * em,
                        color: *color,
                        dashed: *dashed,
                        page_h,
                    },
                );
            }
            DisplayItem::Rect {
                x,
                y,
                width,
                height,
                color,
            } => {
                emit_rect(
                    &mut content,
                    *x * em + pad,
                    *y * em + pad,
                    *width * em,
                    *height * em,
                    color,
                    page_h,
                );
            }
            DisplayItem::Path {
                x,
                y,
                commands,
                fill,
                color,
            } => {
                emit_path(
                    &mut content,
                    *x * em + pad,
                    *y * em + pad,
                    commands,
                    *fill,
                    color,
                    em,
                    stroke_width,
                    page_h,
                );
            }
        }
    }

    content.finish().into_vec()
}

/// Flip Y: PDF origin is bottom-left, DisplayList origin is top-left.
#[inline]
fn flip_y(y: f64, page_h: f64) -> f32 {
    (page_h - y) as f32
}

// ---------------------------------------------------------------------------
// Glyph
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn emit_glyph(
    content: &mut Content,
    px: f64,
    py: f64,
    font_name: &str,
    char_code: u32,
    scale: f64,
    color: &Color,
    em: f64,
    page_h: f64,
    embedded: &[EmbeddedFont],
    font_index: &HashMap<FontId, usize>,
    font_data: &fonts::RawFontData,
) {
    let (actual_fid, gid) = match fonts::resolve_pdf_glyph(font_data, font_name, char_code) {
        Some(p) => p,
        None => return,
    };

    let ef_idx = match font_index.get(&actual_fid) {
        Some(&i) => i,
        None => return,
    };
    let ef = &embedded[ef_idx];

    let new_cid = match ef.remapper.get(gid) {
        Some(c) => c,
        None => return,
    };

    let glyph_em = (scale * em) as f32;
    let pdf_x = px as f32;
    let pdf_y = flip_y(py, page_h);

    // CID as 2-byte big-endian.
    let cid_bytes = [(new_cid >> 8) as u8, (new_cid & 0xFF) as u8];

    set_fill_rgb(content, color);
    content.begin_text();
    content.set_font(Name(ef.res_name.as_bytes()), glyph_em);
    content.set_text_matrix([1.0, 0.0, 0.0, 1.0, pdf_x, pdf_y]);
    content.show(Str(&cid_bytes));
    content.end_text();
}

// ---------------------------------------------------------------------------
// Line
// ---------------------------------------------------------------------------

struct LineParams {
    x: f64,
    y: f64,
    width: f64,
    thickness: f64,
    color: Color,
    dashed: bool,
    page_h: f64,
}

fn emit_line(content: &mut Content, line: &LineParams) {
    let t = line.thickness.max(0.5);

    set_fill_rgb(content, &line.color);

    if line.dashed {
        let dash_len = (4.0 * t).max(1.0);
        let gap_len = (4.0 * t).max(1.0);
        let period = dash_len + gap_len;
        let top = line.y - t / 2.0;
        let mut cur_x = line.x;
        while cur_x < line.x + line.width {
            let seg_w = dash_len.min(line.x + line.width - cur_x).max(0.5);
            let pdf_x = cur_x as f32;
            let pdf_y = flip_y(top + t, line.page_h); // bottom edge in PDF coords
            content.rect(pdf_x, pdf_y, seg_w as f32, t as f32);
            cur_x += period;
        }
        content.fill_nonzero();
    } else {
        let top = line.y - t / 2.0;
        let pdf_x = line.x as f32;
        let pdf_y = flip_y(top + t, line.page_h);
        content.rect(pdf_x, pdf_y, line.width as f32, t as f32);
        content.fill_nonzero();
    }
}

// ---------------------------------------------------------------------------
// Rect
// ---------------------------------------------------------------------------

fn emit_rect(
    content: &mut Content,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    color: &Color,
    page_h: f64,
) {
    let w = width.max(0.5);
    let h = height.max(0.5);

    set_fill_rgb(content, color);
    let pdf_x = x as f32;
    let pdf_y = flip_y(y + h, page_h); // bottom-left corner in PDF coords
    content.rect(pdf_x, pdf_y, w as f32, h as f32);
    content.fill_nonzero();
}

// ---------------------------------------------------------------------------
// Path
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn emit_path(
    content: &mut Content,
    ox: f64,
    oy: f64,
    commands: &[PathCommand],
    fill: bool,
    color: &Color,
    em: f64,
    stroke_width: f64,
    page_h: f64,
) {
    if fill {
        // Split by MoveTo to avoid cross-contour winding issues (same as ratex-render).
        let mut start = 0;
        for i in 1..commands.len() {
            if matches!(commands[i], PathCommand::MoveTo { .. }) {
                emit_path_segment(
                    content,
                    ox,
                    oy,
                    &commands[start..i],
                    true,
                    color,
                    em,
                    stroke_width,
                    page_h,
                );
                start = i;
            }
        }
        emit_path_segment(
            content,
            ox,
            oy,
            &commands[start..],
            true,
            color,
            em,
            stroke_width,
            page_h,
        );
    } else {
        emit_path_segment(
            content,
            ox,
            oy,
            commands,
            false,
            color,
            em,
            stroke_width,
            page_h,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_path_segment(
    content: &mut Content,
    ox: f64,
    oy: f64,
    commands: &[PathCommand],
    fill: bool,
    color: &Color,
    em: f64,
    stroke_width: f64,
    page_h: f64,
) {
    if commands.is_empty() {
        return;
    }

    // Track current point for quad-to-cubic promotion.
    let mut cur = (0.0f32, 0.0f32);

    for cmd in commands {
        match cmd {
            PathCommand::MoveTo { x, y } => {
                let px = (ox + x * em) as f32;
                let py = flip_y(oy + y * em, page_h);
                content.move_to(px, py);
                cur = (px, py);
            }
            PathCommand::LineTo { x, y } => {
                let px = (ox + x * em) as f32;
                let py = flip_y(oy + y * em, page_h);
                content.line_to(px, py);
                cur = (px, py);
            }
            PathCommand::CubicTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                let end_x = (ox + x * em) as f32;
                let end_y = flip_y(oy + y * em, page_h);
                content.cubic_to(
                    (ox + x1 * em) as f32,
                    flip_y(oy + y1 * em, page_h),
                    (ox + x2 * em) as f32,
                    flip_y(oy + y2 * em, page_h),
                    end_x,
                    end_y,
                );
                cur = (end_x, end_y);
            }
            PathCommand::QuadTo { x1, y1, x, y } => {
                // PDF has no native quadratic Bezier; promote to cubic.
                // Q(P0,P1,P2) → C(P0, P0+2/3*(P1-P0), P2+2/3*(P1-P2), P2)
                let qx = (ox + x1 * em) as f32;
                let qy = flip_y(oy + y1 * em, page_h);
                let end_x = (ox + x * em) as f32;
                let end_y = flip_y(oy + y * em, page_h);
                let cp1_x = cur.0 + 2.0 / 3.0 * (qx - cur.0);
                let cp1_y = cur.1 + 2.0 / 3.0 * (qy - cur.1);
                let cp2_x = end_x + 2.0 / 3.0 * (qx - end_x);
                let cp2_y = end_y + 2.0 / 3.0 * (qy - end_y);
                content.cubic_to(cp1_x, cp1_y, cp2_x, cp2_y, end_x, end_y);
                cur = (end_x, end_y);
            }
            PathCommand::Close => {
                content.close_path();
            }
        }
    }

    if fill {
        set_fill_rgb(content, color);
        content.fill_even_odd();
    } else {
        set_stroke_rgb(content, color);
        content.set_line_width(stroke_width as f32);
        content.stroke();
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn set_fill_rgb(content: &mut Content, color: &Color) {
    content.set_fill_rgb(color.r, color.g, color.b);
}

fn set_stroke_rgb(content: &mut Content, color: &Color) {
    content.set_stroke_rgb(color.r, color.g, color.b);
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{build_content_stream, BASELINE_MARKER_LEN_EM, BASELINE_MARKER_WIDTH_EM};
    use ratex_font::FontId;
    use ratex_types::display_item::DisplayList;

    #[test]
    fn show_baseline_draws_short_marker_at_formula_baseline() {
        let display_list = DisplayList {
            items: vec![],
            width: 4.0,
            height: 2.5,
            depth: 1.0,
        };
        let font_data: crate::fonts::RawFontData = HashMap::<FontId, Vec<u8>>::new().into();

        let content = build_content_stream(
            &display_list,
            &[],
            &HashMap::new(),
            &font_data,
            20.0,
            10.0,
            (display_list.height + display_list.depth) * 20.0 + 20.0,
            1.5,
            true,
        );

        let content = String::from_utf8(content).expect("PDF content stream should be UTF-8");
        let baseline_y = display_list.depth * 20.0 + 10.0;
        let baseline_width = 20.0 * BASELINE_MARKER_WIDTH_EM;
        let baseline_x2 = (10.0 + 20.0 * BASELINE_MARKER_LEN_EM) as f32;

        assert!(
            content.contains(&format!("{baseline_width:.1} w")),
            "missing baseline width in content stream: {content}"
        );
        assert!(
            content.contains(&format!("10 {:.0} m", baseline_y)),
            "missing baseline start in content stream: {content}"
        );
        assert!(
            content.contains(&format!("{baseline_x2:.7} {:.0} l", baseline_y)),
            "missing baseline end in content stream: {content}"
        );
    }
}
