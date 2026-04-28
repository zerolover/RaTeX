use std::collections::HashMap;

use ab_glyph::{Font, FontRef};
use ratex_font::FontId;
use ratex_font_loader::FontSet;
use ratex_types::color::Color;
use ratex_types::display_item::{DisplayItem, DisplayList};
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform};

pub struct RenderOptions {
    pub font_size: f32,
    pub padding: f32,
    /// Background fill color for the output PNG. Set alpha to 0.0 for transparency.
    pub background_color: Color,
    /// Directory containing KaTeX `*.ttf` files. Required KaTeX faces are loaded lazily;
    /// rendering fails if a face referenced by the display list is missing.
    pub font_dir: String,
    /// Multiplies pixels-per-em (and padding) so the same layout renders at higher resolution
    /// (e.g. 2.0 to align RaTeX PNG pixel density with Puppeteer `deviceScaleFactor: 2` refs).
    pub device_pixel_ratio: f32,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            font_size: 40.0,
            padding: 10.0,
            background_color: Color::WHITE,
            font_dir: String::new(),
            device_pixel_ratio: 1.0,
        }
    }
}

pub fn render_to_png(
    display_list: &DisplayList,
    options: &RenderOptions,
) -> Result<Vec<u8>, String> {
    let em = options.font_size;
    let pad = options.padding;
    let dpr = options.device_pixel_ratio.clamp(0.01, 16.0);
    let em_px = em * dpr;
    let pad_px = pad * dpr;

    let total_h = display_list.height + display_list.depth;
    let img_w = (display_list.width as f32 * em_px + 2.0 * pad_px).ceil() as u32;
    let img_h = (total_h as f32 * em_px + 2.0 * pad_px).ceil() as u32;

    let img_w = img_w.max(1);
    let img_h = img_h.max(1);

    let mut pixmap = Pixmap::new(img_w, img_h)
        .ok_or_else(|| format!("Failed to create pixmap {}x{}", img_w, img_h))?;

    pixmap.fill(to_tiny_skia_color(options.background_color));

    // Lazy font loading is shared across renderers and source-aware by font_dir.
    render_with_fonts(&mut pixmap, display_list, options, em_px, pad_px, dpr)?;

    encode_png(&pixmap)
}

/// Load fonts lazily and render the DisplayList.
fn render_with_fonts(
    pixmap: &mut Pixmap,
    display_list: &DisplayList,
    options: &RenderOptions,
    em_px: f32,
    pad_px: f32,
    dpr: f32,
) -> Result<(), String> {
    let fonts = ratex_font_loader::load_fonts_for_items(&options.font_dir, &display_list.items)?;
    let font_refs = build_font_refs(&fonts)?;
    render_display_list(pixmap, display_list, &font_refs, em_px, pad_px, dpr);
    Ok(())
}

fn to_tiny_skia_color(color: Color) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba(
        color.r.clamp(0.0, 1.0),
        color.g.clamp(0.0, 1.0),
        color.b.clamp(0.0, 1.0),
        color.a.clamp(0.0, 1.0),
    )
    .unwrap_or(tiny_skia::Color::TRANSPARENT)
}

/// Build a `FontId → FontRef` map from the raw font data (borrowed from the cache lock).
fn build_font_refs(data: &FontSet) -> Result<HashMap<FontId, FontRef<'_>>, String> {
    let mut font_refs = HashMap::new();
    for (id, bytes) in data.iter() {
        let font = FontRef::try_from_slice_and_index(bytes, sfnt_collection_index(*id))
            .map_err(|e| format!("Failed to parse font {:?}: {}", id, e))?;
        font_refs.insert(*id, font);
    }

    if !font_refs.contains_key(&FontId::MainRegular) {
        return Err("Main-Regular font not found".to_string());
    }

    Ok(font_refs)
}

/// Render all items in the DisplayList using the given font cache.
fn render_display_list(
    pixmap: &mut Pixmap,
    display_list: &DisplayList,
    font_cache: &HashMap<FontId, FontRef<'_>>,
    em_px: f32,
    pad_px: f32,
    dpr: f32,
) {
    let mut font_id_cache: HashMap<&str, FontId> = HashMap::new();
    for item in &display_list.items {
        match item {
            DisplayItem::GlyphPath {
                x,
                y,
                scale,
                font,
                char_code,
                color,
            } => {
                let glyph_em = em_px * *scale as f32;
                let font_id = *font_id_cache
                    .entry(font.as_str())
                    .or_insert_with(|| FontId::parse(font).unwrap_or(FontId::MainRegular));
                render_glyph(
                    pixmap,
                    *x as f32 * em_px + pad_px,
                    *y as f32 * em_px + pad_px,
                    font_id,
                    *char_code,
                    color,
                    font_cache,
                    glyph_em,
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
                render_line(
                    pixmap,
                    *x as f32 * em_px + pad_px,
                    *y as f32 * em_px + pad_px,
                    *width as f32 * em_px,
                    *thickness as f32 * em_px,
                    color,
                    *dashed,
                );
            }
            DisplayItem::Rect {
                x,
                y,
                width,
                height,
                color,
            } => {
                render_rect(
                    pixmap,
                    *x as f32 * em_px + pad_px,
                    *y as f32 * em_px + pad_px,
                    *width as f32 * em_px,
                    *height as f32 * em_px,
                    color,
                );
            }
            DisplayItem::Path {
                x,
                y,
                commands,
                fill,
                color,
            } => {
                render_path(
                    pixmap,
                    *x as f32 * em_px + pad_px,
                    *y as f32 * em_px + pad_px,
                    commands,
                    *fill,
                    color,
                    em_px,
                    1.5 * dpr,
                );
            }
        }
    }
}

fn sfnt_collection_index(id: FontId) -> u32 {
    match id {
        FontId::CjkRegular => ratex_unicode_font::unicode_font_face_index().unwrap_or(0),
        _ => 0,
    }
}

/// When `skip_main_regular` is `true`, skips `Main-Regular` (caller already tried that face).
#[allow(clippy::too_many_arguments)]
fn try_system_unicode_fallback(
    pixmap: &mut Pixmap,
    px: f32,
    py: f32,
    ch: char,
    color: &Color,
    em: f32,
    font_cache: &HashMap<FontId, FontRef<'_>>,
    skip_main_regular: bool,
) -> bool {
    if !skip_main_regular {
        if let Some(fallback) = font_cache.get(&FontId::MainRegular) {
            let fid = fallback.glyph_id(ch);
            if fid.0 != 0
                && render_glyph_with_font(
                    pixmap,
                    px,
                    py,
                    FontGlyph {
                        font_id: FontId::MainRegular,
                        font: fallback,
                        glyph_id: fid,
                    },
                    color,
                    em,
                )
            {
                return true;
            }
        }
    }
    if let Some(cjk_font) = font_cache.get(&FontId::CjkRegular) {
        let fid = cjk_font.glyph_id(ch);
        if fid.0 != 0
            && render_glyph_with_font(
                pixmap,
                px,
                py,
                FontGlyph {
                    font_id: FontId::CjkRegular,
                    font: cjk_font,
                    glyph_id: fid,
                },
                color,
                em,
            )
        {
            return true;
        }
    }
    false
}

#[allow(clippy::too_many_arguments)]
fn render_glyph(
    pixmap: &mut Pixmap,
    px: f32,
    py: f32,
    font_id: FontId,
    char_code: u32,
    color: &Color,
    font_cache: &HashMap<FontId, FontRef<'_>>,
    em: f32,
) {
    let font = match font_cache.get(&font_id) {
        Some(f) => f,
        None => match font_cache.get(&FontId::MainRegular) {
            Some(f) => f,
            None => return,
        },
    };

    let ch = ratex_font::katex_ttf_glyph_char(font_id, char_code);
    let glyph_id = font.glyph_id(ch);

    if glyph_id.0 == 0 {
        let _ = try_system_unicode_fallback(pixmap, px, py, ch, color, em, font_cache, false);
        return;
    }

    // `RATEX_UNICODE_FONT` may map a codepoint to a non-.notdef glyph with no outlines; treat it as missing.
    if font_id == FontId::CjkRegular {
        if render_glyph_with_font(
            pixmap,
            px,
            py,
            FontGlyph {
                font_id: FontId::CjkRegular,
                font,
                glyph_id,
            },
            color,
            em,
        ) {
            return;
        }
        return;
    }

    if render_glyph_with_font(
        pixmap,
        px,
        py,
        FontGlyph {
            font_id,
            font,
            glyph_id,
        },
        color,
        em,
    ) {
        return;
    }
    // cmap had a non-zero GID but no `glyf` outline.
    let skip_main = font_id == FontId::MainRegular;
    let _ = try_system_unicode_fallback(pixmap, px, py, ch, color, em, font_cache, skip_main);
}

struct FontGlyph<'a> {
    font_id: FontId,
    font: &'a FontRef<'a>,
    glyph_id: ab_glyph::GlyphId,
}

fn render_glyph_with_font(
    pixmap: &mut Pixmap,
    px: f32,
    py: f32,
    g: FontGlyph<'_>,
    color: &Color,
    em: f32,
) -> bool {
    let curves = match ratex_font_loader::outline_cache::get_or_compute_outline(
        g.font_id, g.font, g.glyph_id,
    ) {
        Some(c) => c,
        None => return false,
    };
    if curves.is_empty() {
        return false;
    }

    let units_per_em = g.font.units_per_em().unwrap_or(1000.0);
    let scale = em / units_per_em;

    let mut builder = PathBuilder::new();
    let mut last_end: Option<(f32, f32)> = None;

    for curve in curves.iter() {
        use ab_glyph::OutlineCurve;
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

        // New contour if start doesn't match previous end
        let need_move = match last_end {
            None => true,
            Some((lx, ly)) => (lx - start.0).abs() > 0.01 || (ly - start.1).abs() > 0.01,
        };

        if need_move {
            if last_end.is_some() {
                builder.close();
            }
            builder.move_to(start.0, start.1);
        }

        match curve {
            OutlineCurve::Line(_, p1) => {
                builder.line_to(px + p1.x * scale, py - p1.y * scale);
            }
            OutlineCurve::Quad(_, p1, p2) => {
                builder.quad_to(
                    px + p1.x * scale,
                    py - p1.y * scale,
                    px + p2.x * scale,
                    py - p2.y * scale,
                );
            }
            OutlineCurve::Cubic(_, p1, p2, p3) => {
                builder.cubic_to(
                    px + p1.x * scale,
                    py - p1.y * scale,
                    px + p2.x * scale,
                    py - p2.y * scale,
                    px + p3.x * scale,
                    py - p3.y * scale,
                );
            }
        }

        last_end = Some(end);
    }

    if last_end.is_some() {
        builder.close();
    }

    if let Some(path) = builder.finish() {
        let mut paint = Paint::default();
        paint.set_color_rgba8(
            (color.r * 255.0) as u8,
            (color.g * 255.0) as u8,
            (color.b * 255.0) as u8,
            255,
        );
        paint.anti_alias = true;
        pixmap.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
        true
    } else {
        false
    }
}

fn render_line(
    pixmap: &mut Pixmap,
    x: f32,
    y: f32,
    width: f32,
    thickness: f32,
    color: &Color,
    dashed: bool,
) {
    let t = thickness.max(1.0);
    let mut paint = Paint::default();
    paint.set_color_rgba8(
        (color.r * 255.0) as u8,
        (color.g * 255.0) as u8,
        (color.b * 255.0) as u8,
        255,
    );

    if dashed {
        // Draw a dashed line: dash length = 4t, gap = 4t.
        let dash_len = (4.0 * t).max(2.0);
        let gap_len = (4.0 * t).max(2.0);
        let period = dash_len + gap_len;
        let top = y - t / 2.0;
        let mut cur_x = x;
        while cur_x < x + width {
            let seg_width = (dash_len).min(x + width - cur_x);
            let seg_width = seg_width.max(2.0);
            if let Some(rect) = tiny_skia::Rect::from_xywh(cur_x, top, seg_width, t) {
                pixmap.fill_rect(rect, &paint, Transform::identity(), None);
            }
            cur_x += period;
        }
    } else if let Some(rect) = tiny_skia::Rect::from_xywh(x, y - t / 2.0, width, t) {
        pixmap.fill_rect(rect, &paint, Transform::identity(), None);
    }
}

fn render_rect(pixmap: &mut Pixmap, x: f32, y: f32, width: f32, height: f32, color: &Color) {
    // Clamp to at least 2px: with width=1px at a fractional pixel position, fill_dot8's
    // dot-8 fixed-point arithmetic can produce inner_width=0 and trigger a debug_assert.
    // 2px guarantees at least 1 full interior pixel regardless of sub-pixel alignment.
    let width = width.max(2.0);
    let height = height.max(2.0);
    let rect = tiny_skia::Rect::from_xywh(x, y, width, height);
    if let Some(rect) = rect {
        let mut paint = Paint::default();
        paint.set_color_rgba8(
            (color.r * 255.0) as u8,
            (color.g * 255.0) as u8,
            (color.b * 255.0) as u8,
            255,
        );
        pixmap.fill_rect(rect, &paint, Transform::identity(), None);
    }
}

#[allow(clippy::too_many_arguments)]
fn render_path(
    pixmap: &mut Pixmap,
    x: f32,
    y: f32,
    commands: &[ratex_types::path_command::PathCommand],
    fill: bool,
    color: &Color,
    em: f32,
    stroke_width_px: f32,
) {
    // For filled paths, render each subpath (delimited by MoveTo) as a separate
    // fill_path call.  KaTeX stretchy arrows are assembled from multiple path
    // components (e.g. "lefthook" + "rightarrow") whose winding directions can
    // be opposite.  Combining them into a single fill_path with FillRule::Winding
    // causes the shaft region to cancel out (net winding = 0 → unfilled).
    // Drawing each subpath independently avoids cross-component winding interactions.
    if fill {
        let mut start = 0;
        for i in 1..commands.len() {
            if matches!(
                commands[i],
                ratex_types::path_command::PathCommand::MoveTo { .. }
            ) {
                render_path_segment(
                    pixmap,
                    x,
                    y,
                    &commands[start..i],
                    fill,
                    color,
                    em,
                    stroke_width_px,
                );
                start = i;
            }
        }
        render_path_segment(
            pixmap,
            x,
            y,
            &commands[start..],
            fill,
            color,
            em,
            stroke_width_px,
        );
        return;
    }
    render_path_segment(pixmap, x, y, commands, fill, color, em, stroke_width_px);
}

#[allow(clippy::too_many_arguments)]
fn render_path_segment(
    pixmap: &mut Pixmap,
    x: f32,
    y: f32,
    commands: &[ratex_types::path_command::PathCommand],
    fill: bool,
    color: &Color,
    em: f32,
    stroke_width_px: f32,
) {
    let mut builder = PathBuilder::new();
    for cmd in commands {
        match cmd {
            ratex_types::path_command::PathCommand::MoveTo { x: cx, y: cy } => {
                builder.move_to(x + *cx as f32 * em, y + *cy as f32 * em);
            }
            ratex_types::path_command::PathCommand::LineTo { x: cx, y: cy } => {
                builder.line_to(x + *cx as f32 * em, y + *cy as f32 * em);
            }
            ratex_types::path_command::PathCommand::CubicTo {
                x1,
                y1,
                x2,
                y2,
                x: cx,
                y: cy,
            } => {
                builder.cubic_to(
                    x + *x1 as f32 * em,
                    y + *y1 as f32 * em,
                    x + *x2 as f32 * em,
                    y + *y2 as f32 * em,
                    x + *cx as f32 * em,
                    y + *cy as f32 * em,
                );
            }
            ratex_types::path_command::PathCommand::QuadTo {
                x1,
                y1,
                x: cx,
                y: cy,
            } => {
                builder.quad_to(
                    x + *x1 as f32 * em,
                    y + *y1 as f32 * em,
                    x + *cx as f32 * em,
                    y + *cy as f32 * em,
                );
            }
            ratex_types::path_command::PathCommand::Close => {
                builder.close();
            }
        }
    }

    if let Some(path) = builder.finish() {
        let mut paint = Paint::default();
        paint.set_color_rgba8(
            (color.r * 255.0) as u8,
            (color.g * 255.0) as u8,
            (color.b * 255.0) as u8,
            255,
        );
        if fill {
            paint.anti_alias = true;
            // Even-odd: KaTeX `tallDelim` vert uses two subpaths (outline + stem); nonzero winding
            // double-fills the stem and inflates ink vs reference PNGs.
            pixmap.fill_path(
                &path,
                &paint,
                FillRule::EvenOdd,
                Transform::identity(),
                None,
            );
        } else {
            let stroke = Stroke {
                width: stroke_width_px,
                ..Default::default()
            };
            pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }
    }
}

fn encode_png(pixmap: &Pixmap) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut buf, pixmap.width(), pixmap.height());
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|e| format!("PNG header error: {}", e))?;
        writer
            .write_image_data(pixmap.data())
            .map_err(|e| format!("PNG write error: {}", e))?;
    }
    Ok(buf)
}
