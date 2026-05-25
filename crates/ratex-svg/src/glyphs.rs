//! Glyph outlines as SVG `<path>` via `ab_glyph`.

use std::collections::HashMap;

use ab_glyph::{Font, FontRef, OutlineCurve};
use ratex_font::FontId;
use ratex_font_loader::FontSet;

fn sfnt_collection_index(id: FontId) -> u32 {
    match id {
        FontId::CjkRegular => ratex_unicode_font::unicode_font_face_index().unwrap_or(0),
        _ => 0,
    }
}

/// Build a `FontId → FontRef` map from the cached raw data (held alive by `guard`).
pub(crate) fn build_font_refs<'a>(
    data: &'a FontSet,
) -> Result<HashMap<FontId, FontRef<'a>>, String> {
    let mut font_refs = HashMap::new();
    for (id, bytes) in data.iter() {
        let font = FontRef::try_from_slice_and_index(bytes, sfnt_collection_index(*id))
            .map_err(|e| format!("Failed to parse font {:?}: {}", id, e))?;
        font_refs.insert(*id, font);
    }
    Ok(font_refs)
}

/// Viewport-space bounding box of a glyph outline.
#[derive(Debug, Clone, Copy)]
pub(crate) struct GlyphBounds {
    pub x_min: f32,
    pub y_min: f32,
    pub x_max: f32,
    pub y_max: f32,
}

/// Vector path output matching `ratex-render::render_glyph` geometry,
/// together with its viewport-space bounding box (from font glyph_bounds).
#[derive(Debug)]
pub(crate) struct GlyphAsset {
    pub path: String,
    pub bounds: Option<GlyphBounds>,
}

/// Same geometry as `ratex-render`: SVG user space, y downward.
pub(crate) fn outline_glyph(
    px: f32,
    py: f32,
    glyph_em: f32,
    font_name: &str,
    char_code: u32,
    font_cache: &HashMap<FontId, FontRef<'_>>,
) -> Option<GlyphAsset> {
    let font_id = FontId::parse(font_name).unwrap_or(FontId::MainRegular);
    let font = match font_cache.get(&font_id) {
        Some(f) => f,
        None => font_cache.get(&FontId::MainRegular)?,
    };

    let ch = ratex_font::katex_ttf_glyph_char(font_id, char_code);
    let glyph_id = font.glyph_id(ch);

    if glyph_id.0 == 0 {
        return try_system_unicode_fallback_svg(px, py, glyph_em, ch, font_cache, false);
    }

    if font_id == FontId::CjkRegular {
        if let Some((d, bounds)) = outline_to_d(px, py, glyph_em, FontId::CjkRegular, font, glyph_id) {
            return Some(GlyphAsset { path: d, bounds });
        }
        return None;
    }

    if let Some((d, bounds)) = outline_to_d(px, py, glyph_em, font_id, font, glyph_id) {
        return Some(GlyphAsset { path: d, bounds });
    }

    let skip_main = font_id == FontId::MainRegular;
    try_system_unicode_fallback_svg(px, py, glyph_em, ch, font_cache, skip_main)
}

fn try_system_unicode_fallback_svg(
    px: f32,
    py: f32,
    em: f32,
    ch: char,
    font_cache: &HashMap<FontId, FontRef<'_>>,
    skip_main_regular: bool,
) -> Option<GlyphAsset> {
    if !skip_main_regular {
        if let Some(fallback) = font_cache.get(&FontId::MainRegular) {
            let fid = fallback.glyph_id(ch);
            if fid.0 != 0 {
                if let Some((d, bounds)) = outline_to_d(px, py, em, FontId::MainRegular, fallback, fid) {
                    return Some(GlyphAsset { path: d, bounds });
                }
            }
        }
    }
    if let Some(cjk) = font_cache.get(&FontId::CjkRegular) {
        let cid = cjk.glyph_id(ch);
        if cid.0 != 0 {
            if let Some((d, bounds)) = outline_to_d(px, py, em, FontId::CjkRegular, cjk, cid) {
                return Some(GlyphAsset { path: d, bounds });
            }
        }
    }
    None
}

fn outline_to_d(
    px: f32,
    py: f32,
    em: f32,
    font_id: FontId,
    font: &FontRef<'_>,
    glyph_id: ab_glyph::GlyphId,
) -> Option<(String, Option<GlyphBounds>)> {
    let curves = ratex_font_loader::outline_cache::get_or_compute_outline(
        font_id, font, glyph_id,
    )?;
    let units_per_em = font.units_per_em().unwrap_or(1000.0);
    let scale = em / units_per_em;
    let local_bounds =
        ratex_font_loader::outline_cache::get_or_compute_local_bounds(font_id, font, glyph_id);

    let mut d = String::new();
    let mut last_end: Option<(f32, f32)> = None;

    for curve in curves.iter() {
        let (start, end) = match curve {
            OutlineCurve::Line(p0, p1) => {
                let sx = px + p0.x * scale;
                let sy = py - p0.y * scale;
                let ex = px + p1.x * scale;
                let ey = py - p1.y * scale;
                ((sx, sy), (ex, ey))
            }
            OutlineCurve::Quad(p0, _, p2) => {
                let sx = px + p0.x * scale;
                let sy = py - p0.y * scale;
                let ex = px + p2.x * scale;
                let ey = py - p2.y * scale;
                ((sx, sy), (ex, ey))
            }
            OutlineCurve::Cubic(p0, _, _, p3) => {
                let sx = px + p0.x * scale;
                let sy = py - p0.y * scale;
                let ex = px + p3.x * scale;
                let ey = py - p3.y * scale;
                ((sx, sy), (ex, ey))
            }
        };

        let need_move = match last_end {
            None => true,
            Some((lx, ly)) => (lx - start.0).abs() > 0.01 || (ly - start.1).abs() > 0.01,
        };

        if need_move {
            if last_end.is_some() {
                d.push('Z');
                d.push(' ');
            }
            use std::fmt::Write;
            let _ = write!(
                &mut d,
                "M{} {}",
                super::fmt_num(start.0 as f64),
                super::fmt_num(start.1 as f64)
            );
            d.push(' ');
        }

        match curve {
            OutlineCurve::Line(_, p1) => {
                use std::fmt::Write;
                let _ = write!(
                    &mut d,
                    "L{} {}",
                    super::fmt_num((px + p1.x * scale) as f64),
                    super::fmt_num((py - p1.y * scale) as f64)
                );
                d.push(' ');
            }
            OutlineCurve::Quad(_, p1, p2) => {
                use std::fmt::Write;
                let _ = write!(
                    &mut d,
                    "Q{} {} {} {}",
                    super::fmt_num((px + p1.x * scale) as f64),
                    super::fmt_num((py - p1.y * scale) as f64),
                    super::fmt_num((px + p2.x * scale) as f64),
                    super::fmt_num((py - p2.y * scale) as f64)
                );
                d.push(' ');
            }
            OutlineCurve::Cubic(_, p1, p2, p3) => {
                use std::fmt::Write;
                let _ = write!(
                    &mut d,
                    "C{} {} {} {} {} {}",
                    super::fmt_num((px + p1.x * scale) as f64),
                    super::fmt_num((py - p1.y * scale) as f64),
                    super::fmt_num((px + p2.x * scale) as f64),
                    super::fmt_num((py - p2.y * scale) as f64),
                    super::fmt_num((px + p3.x * scale) as f64),
                    super::fmt_num((py - p3.y * scale) as f64)
                );
                d.push(' ');
            }
        }

        last_end = Some(end);
    }

    if last_end.is_some() {
        d.push('Z');
    }

    let d = d.trim().to_string();
    if d.is_empty() {
        None
    } else {
        let bounds = local_bounds.map(|bounds| GlyphBounds {
            x_min: px + bounds.x_min * scale,
            y_min: py - bounds.y_max * scale,
            x_max: px + bounds.x_max * scale,
            y_max: py - bounds.y_min * scale,
        });
        Some((d, bounds))
    }
}
