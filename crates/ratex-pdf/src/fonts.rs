//! Font loading, subsetting, and CIDFont embedding for pdf-writer.

use std::collections::{BTreeMap, HashMap, HashSet};

use ab_glyph::Font as _;
use pdf_writer::{types::*, Filter, Finish, Name, Pdf, Ref, Str};
use ratex_font::FontId;
use ratex_font_loader::FontSet;
use skrifa::instance::{Location, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::raw::FontRef as SfFontRef;
use skrifa::raw::TableProvider;
use skrifa::{GlyphId, MetadataProvider, Tag};
use subsetter::GlyphRemapper;

/// Loaded TTF bytes keyed by FontId.
pub(crate) type RawFontData = FontSet;

/// `ab_glyph` / OpenType cmap (same stack as PNG/SVG).
fn resolve_glyph_id_abglyph(raw_bytes: &[u8], font_id: FontId, char_code: u32) -> Option<u16> {
    let ch = ratex_font::katex_ttf_glyph_char(font_id, char_code);
    let idx = skrifa_collection_index(font_id);
    let font = ab_glyph::FontRef::try_from_slice_and_index(raw_bytes, idx).ok()?;
    let gid = font.glyph_id(ch);
    if gid.0 == 0 {
        None
    } else {
        Some(gid.0)
    }
}

#[inline]
fn skrifa_collection_index(face_id: FontId) -> u32 {
    match face_id {
        FontId::CjkRegular => ratex_unicode_font::unicode_font_face_index().unwrap_or(0),
        _ => 0,
    }
}

/// If the font has a `wght` variation axis, return the weight to use.
/// Prefers Regular (400) and falls back to the axis default if 400 is out of range.
///
/// Keep weight-selection logic in sync with `get_or_compute_outline` in
/// `ratex-font-loader/src/outline_cache.rs` (uses `ab_glyph` instead of `skrifa`).
fn variable_weight(font: &SfFontRef) -> Option<f32> {
    let axes = font.axes();
    let wght_axis = axes.get_by_tag(Tag::new(b"wght"))?;

    Some(if wght_axis.min_value() <= 400.0 && 400.0 <= wght_axis.max_value() {
        400.0
    } else {
        wght_axis.default_value()
    })
}

/// If the font has a `wght` variation axis, return a `Location` targeting the selected weight.
fn variable_location(font: &SfFontRef) -> Option<Location> {
    let target_weight = variable_weight(font)?;
    Some(font.axes().location([("wght", target_weight)]))
}

/// Always use `ab_glyph` / OpenType cmap so PDF glyph selection stays aligned with layout.
#[inline]
fn resolve_glyph_id_for_face(raw_bytes: &[u8], font_id: FontId, char_code: u32) -> Option<u16> {
    resolve_glyph_id_abglyph(raw_bytes, font_id, char_code)
}

/// True if the glyph has drawable outline segments (not just a lone `move_to` / empty COLR mask).
pub(crate) fn glyph_has_nonempty_outline(raw_bytes: &[u8], face_id: FontId, gid: u16) -> bool {
    let font = match SfFontRef::from_index(raw_bytes, skrifa_collection_index(face_id)) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let outlines = font.outline_glyphs();
    let Some(glyph) = outlines.get(GlyphId::new(gid as u32)) else {
        return false;
    };
    #[derive(Default)]
    struct PenStats {
        /// `line_to` / `quad_to` / `curve_to` only — excludes `move_to` and `close`.
        draws: usize,
    }
    impl OutlinePen for PenStats {
        fn move_to(&mut self, _: f32, _: f32) {}
        fn line_to(&mut self, _: f32, _: f32) {
            self.draws += 1;
        }
        fn quad_to(&mut self, _: f32, _: f32, _: f32, _: f32) {
            self.draws += 1;
        }
        fn curve_to(&mut self, _: f32, _: f32, _: f32, _: f32, _: f32, _: f32) {
            self.draws += 1;
        }
        fn close(&mut self) {}
    }
    let mut pen = PenStats::default();
    // For non-variable fonts, use default Location; for variable fonts, use the computed location.
    let location = variable_location(&font).unwrap_or_default();
    let settings = DrawSettings::unhinted(Size::new(16.0), &location);
    glyph.draw(settings, &mut pen).is_ok() && pen.draws > 0
}

/// Single source of truth for which font face and GID to subset and show.
///
/// Order: requested → `MainRegular` → `CjkRegular` (only when the display item was not already
/// `CjkRegular`). `CjkRegular` cmap hits are ignored unless
/// [`glyph_has_nonempty_outline`] is true so narrow fonts (e.g. AppleGothic missing SC hanzi)
/// are treated as missing glyphs.
pub(crate) fn resolve_pdf_glyph(
    font_data: &RawFontData,
    font_name: &str,
    char_code: u32,
) -> Option<(FontId, u16)> {
    let font_id = FontId::parse(font_name).unwrap_or(FontId::MainRegular);

    // 1. Requested font
    if let Some(bytes) = font_data.get(&font_id) {
        if let Some(gid) = resolve_glyph_id_for_face(bytes, font_id, char_code) {
            if font_id != FontId::CjkRegular || glyph_has_nonempty_outline(bytes, font_id, gid) {
                return Some((font_id, gid));
            }
        }
    }
    // 2. MainRegular
    if let Some(bytes) = font_data.get(&FontId::MainRegular) {
        if let Some(gid) = resolve_glyph_id_for_face(bytes, FontId::MainRegular, char_code) {
            return Some((FontId::MainRegular, gid));
        }
    }
    // 3. CjkRegular — skip when the item already used that face (step 1 tried it).
    if font_id != FontId::CjkRegular {
        if let Some(bytes) = font_data.get(&FontId::CjkRegular) {
            if let Some(gid) = resolve_glyph_id_for_face(bytes, FontId::CjkRegular, char_code) {
                if glyph_has_nonempty_outline(bytes, FontId::CjkRegular, gid) {
                    return Some((FontId::CjkRegular, gid));
                }
            }
        }
    }
    None
}

/// Info about a glyph we want to embed.
#[derive(Clone, Debug)]
pub(crate) struct GlyphInfo {
    /// Unicode codepoint for ToUnicode CMap.
    pub unicode: u32,
}

/// Collected usage for one font.
pub(crate) struct FontUsage {
    pub font_id: FontId,
    /// gid → GlyphInfo
    pub glyphs: BTreeMap<u16, GlyphInfo>,
}

/// Collect font subset usage.
pub(crate) fn collect_glyph_usage(
    items: &[ratex_types::display_item::DisplayItem],
    font_data: &RawFontData,
) -> Vec<FontUsage> {
    let mut usage_map: HashMap<FontId, HashSet<(u16, u32)>> = HashMap::new();

    for item in items {
        if let ratex_types::display_item::DisplayItem::GlyphPath {
            font, char_code, ..
        } = item
        {
            if let Some((face, gid)) = resolve_pdf_glyph(font_data, font, *char_code) {
                usage_map.entry(face).or_default().insert((gid, *char_code));
            }
        }
    }

    let mut font_usages: Vec<FontUsage> = usage_map
        .into_iter()
        .map(|(font_id, set)| {
            let mut glyphs = BTreeMap::new();
            for (gid, unicode) in set {
                glyphs.insert(gid, GlyphInfo { unicode });
            }
            FontUsage { font_id, glyphs }
        })
        .collect();
    font_usages.sort_by_key(|u| u.font_id.as_str().to_string());
    font_usages
}

/// Result of embedding one font into the PDF.
pub(crate) struct EmbeddedFont {
    pub font_id: FontId,
    /// PDF resource name, e.g. "F0", "F1"
    pub res_name: String,
    /// The Type0 font reference for the page Resources dict.
    pub type0_ref: Ref,
    /// Old GID → new CID mapping.
    pub remapper: GlyphRemapper,
}

/// Embed all used fonts into the PDF and return mapping info.
pub(crate) fn embed_fonts(
    pdf: &mut Pdf,
    alloc: &mut Ref,
    usages: &[FontUsage],
    font_data: &RawFontData,
) -> Result<Vec<EmbeddedFont>, String> {
    let mut embedded = Vec::new();

    for (idx, usage) in usages.iter().enumerate() {
        let raw = font_data
            .get(&usage.font_id)
            .ok_or_else(|| format!("Missing font data for {:?}", usage.font_id))?;

        // Build GlyphRemapper with all used glyph IDs.
        let mut remapper = GlyphRemapper::new();
        for &gid in usage.glyphs.keys() {
            remapper.remap(gid);
        }

        // Subset the font.
        let index = skrifa_collection_index(usage.font_id);
        let sf = SfFontRef::from_index(raw, index)
            .map_err(|e| format!("skrifa error: {e}"))?;
        let subsetted = if let Some(target_weight) = variable_weight(&sf) {
            let coords = [(subsetter::Tag::new(b"wght"), target_weight)];
            subsetter::subset_with_variations(raw, index, &coords, &remapper)
        } else {
            subsetter::subset(raw, index, &remapper)
        }.map_err(|e| format!("Subset error for {:?}: {e}", usage.font_id))?;

        // Compress the subset.
        let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&subsetted, 6);

        // Read font metrics via skrifa.
        let upem = sf.head().map_err(|_| "no head table")?.units_per_em() as f32;
        let scale = 1000.0 / upem; // PDF uses 1000 units per em for metrics

        let (ascent, descent, cap_height) = if let Ok(os2) = sf.os2() {
            let asc = os2.s_typo_ascender() as f32 * scale;
            let desc = os2.s_typo_descender() as f32 * scale;
            let cap = os2.s_cap_height().map_or(asc, |v| v as f32 * scale);
            (asc, desc, cap)
        } else {
            (800.0, -200.0, 800.0)
        };

        let bbox = {
            let head = sf.head().map_err(|_| "no head table")?;
            [
                head.x_min() as f32 * scale,
                head.y_min() as f32 * scale,
                head.x_max() as f32 * scale,
                head.y_max() as f32 * scale,
            ]
        };

        // Glyph widths (in 1000-unit space).
        let location = variable_location(&sf);
        let mut widths: Vec<(u16, f32)> = Vec::new();
        if let Some(ref loc) = location {
            // Use variation-aware glyph metrics for variable fonts.
            let glyph_metrics = sf.glyph_metrics(Size::unscaled(), loc);
            for &old_gid in usage.glyphs.keys() {
                let new_cid = remapper.get(old_gid).unwrap_or(0);
                let gid = skrifa::raw::types::GlyphId::new(old_gid as u32);
                let advance = glyph_metrics.advance_width(gid).unwrap_or(0.0) * scale;
                widths.push((new_cid, advance));
            }
        } else {
            // Static font: read directly from hmtx table.
            let hmtx = sf.hmtx().map_err(|_| "no hmtx table")?;
            for &old_gid in usage.glyphs.keys() {
                let new_cid = remapper.get(old_gid).unwrap_or(0);
                let gid = skrifa::raw::types::GlyphId::new(old_gid as u32);
                let advance = hmtx.advance(gid).unwrap_or(0) as f32 * scale;
                widths.push((new_cid, advance));
            }
        }
        widths.sort_by_key(|(cid, _)| *cid);

        // Allocate PDF object refs.
        let type0_ref = alloc.bump();
        let cid_ref = alloc.bump();
        let descriptor_ref = alloc.bump();
        let tounicode_ref = alloc.bump();
        let stream_ref = alloc.bump();

        let base_name = format!("KaTeX_{}", usage.font_id.as_str().replace('-', "_"));
        let res_name = format!("F{idx}");

        // FontDescriptor
        pdf.font_descriptor(descriptor_ref)
            .name(Name(base_name.as_bytes()))
            .flags(FontFlags::SYMBOLIC)
            .bbox(pdf_writer::Rect::new(bbox[0], bbox[1], bbox[2], bbox[3]))
            .italic_angle(0.0)
            .ascent(ascent)
            .descent(descent)
            .cap_height(cap_height)
            .stem_v(80.0)
            .font_file2(stream_ref);

        // CIDFont (Type2)
        let mut cid_font = pdf.cid_font(cid_ref);
        cid_font
            .subtype(CidFontType::Type2)
            .base_font(Name(base_name.as_bytes()))
            .default_width(0.0)
            .font_descriptor(descriptor_ref);
        cid_font.system_info(pdf_writer::types::SystemInfo {
            registry: Str(b"Adobe"),
            ordering: Str(b"Identity"),
            supplement: 0,
        });

        // W array (widths per CID).
        if !widths.is_empty() {
            let mut w = cid_font.widths();
            for &(cid, adv) in &widths {
                w.consecutive(cid, [adv]);
            }
            w.finish();
        }
        cid_font.finish();

        // Type0 (composite) font
        pdf.type0_font(type0_ref)
            .base_font(Name(base_name.as_bytes()))
            .encoding_predefined(Name(b"Identity-H"))
            .descendant_font(cid_ref)
            .to_unicode(tounicode_ref);

        // ToUnicode CMap
        let cmap = build_tounicode_cmap(&usage.glyphs, &remapper);
        pdf.stream(tounicode_ref, cmap.as_bytes())
            .pair(Name(b"Type"), Name(b"CMap"));

        // FontFile2 stream (compressed)
        let mut font_stream = pdf.stream(stream_ref, &compressed);
        font_stream.filter(Filter::FlateDecode);
        font_stream.pair(Name(b"Length1"), subsetted.len() as i32);
        font_stream.finish();

        embedded.push(EmbeddedFont {
            font_id: usage.font_id,
            res_name,
            type0_ref,
            remapper,
        });
    }

    Ok(embedded)
}

/// Build a ToUnicode CMap for PDF text extraction.
fn build_tounicode_cmap(glyphs: &BTreeMap<u16, GlyphInfo>, remapper: &GlyphRemapper) -> String {
    let mut entries = Vec::new();
    for (old_gid, info) in glyphs {
        if let Some(new_cid) = remapper.get(*old_gid) {
            entries.push((new_cid, info.unicode));
        }
    }
    entries.sort_by_key(|(cid, _)| *cid);

    let mut cmap = String::new();
    cmap.push_str("/CIDInit /ProcSet findresource begin\n");
    cmap.push_str("12 dict begin\n");
    cmap.push_str("begincmap\n");
    cmap.push_str("/CIDSystemInfo\n");
    cmap.push_str("<< /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n");
    cmap.push_str("/CMapName /Adobe-Identity-UCS def\n");
    cmap.push_str("/CMapType 2 def\n");
    cmap.push_str("1 begincodespacerange\n");
    cmap.push_str("<0000> <FFFF>\n");
    cmap.push_str("endcodespacerange\n");

    // Write in chunks of 100 (PDF spec limit per block).
    for chunk in entries.chunks(100) {
        cmap.push_str(&format!("{} beginbfchar\n", chunk.len()));
        for &(cid, unicode) in chunk {
            if unicode <= 0xFFFF {
                cmap.push_str(&format!("<{:04X}> <{:04X}>\n", cid, unicode));
            } else {
                // Supplementary plane → UTF-16 surrogate pair.
                let hi = ((unicode - 0x10000) >> 10) + 0xD800;
                let lo = ((unicode - 0x10000) & 0x3FF) + 0xDC00;
                cmap.push_str(&format!("<{:04X}> <{:04X}{:04X}>\n", cid, hi, lo));
            }
        }
        cmap.push_str("endbfchar\n");
    }

    cmap.push_str("endcmap\n");
    cmap.push_str("CMapName currentdict /CMap defineresource pop\n");
    cmap.push_str("end\n");
    cmap.push_str("end\n");
    cmap
}

#[cfg(all(test, target_os = "macos"))]
mod macos_cjk_pdf_tests {
    use super::*;
    use ratex_font::FontId;
    use std::collections::HashMap;
    use std::path::Path;

    const APPLE_GOTHIC: &str = "/System/Library/Fonts/Supplemental/AppleGothic.ttf";
    #[test]
    fn applegothic_missing_sc_hanzi_abglyph_sees_unmapped() {
        let bytes = std::fs::read(APPLE_GOTHIC).expect("AppleGothic");
        for cp in [0x6C27u32, 0x78B3u32] {
            assert!(
                resolve_glyph_id_abglyph(&bytes, FontId::CjkRegular, cp).is_none(),
                "U+{cp:04X} must be unmapped in AppleGothic for PNG/PDF parity"
            );
        }
    }

    #[test]
    fn resolve_pdf_glyph_rejects_missing_sc_in_applegothic() {
        let ag = std::fs::read(APPLE_GOTHIC).expect("AppleGothic");
        let main_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fonts/KaTeX_Main-Regular.ttf");
        let main = std::fs::read(main_path).expect("KaTeX_Main-Regular");
        let mut data = HashMap::new();
        data.insert(FontId::MainRegular, main);
        data.insert(FontId::CjkRegular, ag);
        let data: RawFontData = data.into();
        for cp in [0x6C27u32, 0x78B3u32] {
            let r = resolve_pdf_glyph(&data, "CJK-Regular", cp);
            assert!(r.is_none(), "U+{cp:04X}: expected missing glyph, got {r:?}");
        }
    }
}
