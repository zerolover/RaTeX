//! Global outline cache shared by PNG, SVG, and PDF renderers.
//!
//! `ab_glyph::Font::outline()` parses the TrueType `glyf` table on every call.
//! The same glyphs appear repeatedly within a formula (e.g. three `2`s in
//! `x^2 + y^2 = z^2`) and across consecutive renders — caching eliminates
//! redundant glyf parsing.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, RwLock};

use ab_glyph::{Font, FontRef, GlyphId, OutlineCurve, VariableFont};
use ratex_font::FontId;

type OutlineData = Arc<[OutlineCurve]>;

static OUTLINE_CACHE: LazyLock<RwLock<HashMap<(FontId, GlyphId), OutlineData>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));
static LOCAL_BOUNDS_CACHE: LazyLock<RwLock<HashMap<(FontId, GlyphId), LocalGlyphBounds>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

#[derive(Debug, Clone, Copy)]
pub struct LocalGlyphBounds {
    pub x_min: f32,
    pub y_min: f32,
    pub x_max: f32,
    pub y_max: f32,
}

/// Retrieve cached outline curves, or compute + cache them via `font.outline()`.
///
/// Position and scale are **not** applied — callers must transform the curves
/// with their own `px`, `py`, and `em` values before rasterising or serializing.
///
/// For variable fonts, sets `wght=400` (Regular) if the axis exists and supports it.
pub fn get_or_compute_outline(
    font_id: FontId,
    font: &FontRef<'_>,
    glyph_id: GlyphId,
) -> Option<Arc<[OutlineCurve]>> {
    let key = (font_id, glyph_id);

    // Fast path: read-lock
    {
        let cache = OUTLINE_CACHE.read().unwrap();
        if let Some(cached) = cache.get(&key) {
            return Some(Arc::clone(cached));
        }
    }

    // Slow path: compute outline + write-lock.
    // For variable fonts, clone + pin to wght=400; non-variable fonts use the original directly.
    // Keep in sync with `variable_weight` in ratex-pdf/src/fonts.rs.
    let needs_variation = font
        .variations()
        .iter()
        .any(|axis| &axis.tag == b"wght");

    let outline = if needs_variation {
        let mut instance = font.clone();
        for axis in instance.variations() {
            if &axis.tag == b"wght" {
                let w = if axis.min_value <= 400.0 && 400.0 <= axis.max_value {
                    400.0
                } else {
                    axis.default_value
                };
                instance.set_variation(b"wght", w);
                break;
            }
        }
        instance.outline(glyph_id)?
    } else {
        font.outline(glyph_id)?
    };
    let curves: Arc<[OutlineCurve]> = outline.curves.into();

    let mut cache = OUTLINE_CACHE.write().unwrap();
    // Double-check: another thread may have inserted while we computed
    if let Some(existing) = cache.get(&key) {
        return Some(Arc::clone(existing));
    }
    let result = Arc::clone(&curves);
    cache.insert(key, curves);
    Some(result)
}

/// Retrieve or compute a glyph's local outline bounds in font units.
///
/// These bounds are independent of per-instance position and scale.
pub fn get_or_compute_local_bounds(
    font_id: FontId,
    font: &FontRef<'_>,
    glyph_id: GlyphId,
) -> Option<LocalGlyphBounds> {
    let key = (font_id, glyph_id);

    {
        let cache = LOCAL_BOUNDS_CACHE.read().unwrap();
        if let Some(bounds) = cache.get(&key) {
            return Some(*bounds);
        }
    }

    let outline = get_or_compute_outline(font_id, font, glyph_id)?;
    let mut x_min = f32::INFINITY;
    let mut y_min = f32::INFINITY;
    let mut x_max = f32::NEG_INFINITY;
    let mut y_max = f32::NEG_INFINITY;

    let mut include = |x: f32, y: f32| {
        x_min = x_min.min(x);
        y_min = y_min.min(y);
        x_max = x_max.max(x);
        y_max = y_max.max(y);
    };

    for curve in outline.iter() {
        match curve {
            OutlineCurve::Line(p0, p1) => {
                include(p0.x, p0.y);
                include(p1.x, p1.y);
            }
            OutlineCurve::Quad(p0, p1, p2) => {
                include(p0.x, p0.y);
                include(p1.x, p1.y);
                include(p2.x, p2.y);
            }
            OutlineCurve::Cubic(p0, p1, p2, p3) => {
                include(p0.x, p0.y);
                include(p1.x, p1.y);
                include(p2.x, p2.y);
                include(p3.x, p3.y);
            }
        }
    }

    if !x_min.is_finite() || !y_min.is_finite() || !x_max.is_finite() || !y_max.is_finite() {
        return None;
    }

    let bounds = LocalGlyphBounds {
        x_min,
        y_min,
        x_max,
        y_max,
    };

    let mut cache = LOCAL_BOUNDS_CACHE.write().unwrap();
    if let Some(existing) = cache.get(&key) {
        return Some(*existing);
    }
    cache.insert(key, bounds);
    Some(bounds)
}

/// Clear cached outline and local-bounds data for a single logical font id.
pub fn clear_font_caches(font_id: FontId) {
    if let Ok(mut cache) = OUTLINE_CACHE.write() {
        cache.retain(|(cached_font_id, _), _| *cached_font_id != font_id);
    }
    if let Ok(mut cache) = LOCAL_BOUNDS_CACHE.write() {
        cache.retain(|(cached_font_id, _), _| *cached_font_id != font_id);
    }
}
