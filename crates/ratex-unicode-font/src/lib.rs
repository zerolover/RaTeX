//! Discover a system Unicode font for rendering glyphs not present in KaTeX fonts.
//!
//! Discovery entry points:
//! - `load_unicode_font_arc()` — respects `RATEX_UNICODE_FONT` (highest priority), then system fonts.
//! - `unicode_font_face_index` — TTC face index for
//!   `FontRef::try_from_slice_and_index` when discovery returns a font collection.
//!
//! Each result is cached in a `OnceLock` and computed at most once per process.

use std::sync::{Arc, RwLock};

/// `(full font file bytes, face index within TTC or 0 for single-font / unknown collection face)`.
static UNICODE_FONT: RwLock<Option<(Arc<Vec<u8>>, u32)>> = RwLock::new(None);

/// Raw TTF/OTF bytes of a discovered primary Unicode font, or `None` if none is found.
///
/// Checks, in order:
/// 1. `RATEX_UNICODE_FONT`
/// 2. Hard-coded system font paths
/// 3. `fontdb` system discovery
///
/// The result is cached after the first successful or failed lookup.
pub fn load_unicode_font_arc() -> Option<Arc<Vec<u8>>> {
    let read = UNICODE_FONT.read().unwrap();
    if let Some((bytes, _)) = read.as_ref() {
        return Some(Arc::clone(bytes));
    }
    drop(read);

    let font = load_unicode_fallback_font();
    *UNICODE_FONT.write().unwrap() = font.clone();
    font.as_ref().map(|(bytes, _)| Arc::clone(bytes))
}

/// Collection index for the cached primary Unicode face (`0` when not a collection).
pub fn unicode_font_face_index() -> Option<u32> {
    let read = UNICODE_FONT.read().unwrap();
    if let Some((_, index)) = read.as_ref() {
        return Some(*index);
    }
    drop(read);

    let font = load_unicode_fallback_font();
    *UNICODE_FONT.write().unwrap() = font.clone();
    font.as_ref().map(|(_, index)| *index)
}

/// Set a custom Unicode font from a spec string.
///
/// Spec format: `path`, `path#index`, or `path#FamilyName`.
///
/// Returns `true` if the font was successfully loaded and set.
///
/// Note: This only updates the cache owned by this crate. Callers that keep
/// derived font caches must invalidate them separately.
pub fn set_unicode_font(spec: &str) -> bool {
    if let Some(font) = load_font_spec(spec) {
        *UNICODE_FONT.write().unwrap() = Some(font);
        true
    } else {
        false
    }
}

/// Clear the cached Unicode font, forcing re-discovery on next access.
pub fn clear_unicode_font() {
    *UNICODE_FONT.write().unwrap() = None;
}

/// TrueType / OpenType **single** font (not `.ttc`). For collections see [`is_sfnt_container`].
fn is_sfnt_single_font(bytes: &[u8]) -> bool {
    bytes.len() >= 4
        && (bytes[..4] == [0x00, 0x01, 0x00, 0x00]
            || bytes[..4] == [0x4F, 0x54, 0x54, 0x4F]
            || bytes[..4] == [0x74, 0x72, 0x75, 0x65])
}

/// Single font or TrueType **collection** (`ttcf`).
fn is_sfnt_container(bytes: &[u8]) -> bool {
    is_sfnt_single_font(bytes) || bytes.get(0..4) == Some(b"ttcf")
}

fn load_unicode_fallback_font() -> Option<(Arc<Vec<u8>>, u32)> {
    // 1. User-specified font via RATEX_UNICODE_FONT
    if let Ok(spec) = std::env::var("RATEX_UNICODE_FONT") {
        if let Some(font) = load_font_spec(&spec) {
            eprintln!("[ratex-unicode-font] loaded from RATEX_UNICODE_FONT: {}", spec);
            return Some(font);
        }
    }

    // 2. System font discovery
    discover_system_font()
}

/// Discover a font from system paths and locale-aware system-fonts presets (does NOT check
/// `RATEX_UNICODE_FONT`).
///
/// Prioritizes fonts with broad Unicode coverage (symbols, CJK) so the discovered
/// system Unicode font remains broadly usable even when a user-selected font
/// (e.g. a narrow Korean font) lacks many glyphs.
fn discover_system_font() -> Option<(Arc<Vec<u8>>, u32)> {
    // 1. Typical system paths with broad Unicode coverage
    #[rustfmt::skip]
    let candidates: &[&str] = &[
        // Linux
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc#Noto Sans CJK SC",
        // macOS
        "/Library/Fonts/Arial Unicode.ttf",
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        // Windows
        "C:\\Windows\\Fonts\\NotoSansSC-VF.ttf",
        "C:\\Windows\\Fonts\\msyh.ttc#Microsoft YaHei",
        "C:\\Windows\\Fonts\\Deng.ttf",
    ];

    for &spec in candidates {
        if let Some(font) = load_font_spec(spec) {
            eprintln!("[ratex-unicode-font] found system font: {}", spec);
            return Some(font);
        }
    }

    // 2. fontdb — search for well-known broad-coverage families first.
    let mut db = fontdb::Database::new();
    db.load_system_fonts();

    #[cfg(target_os = "macos")]
    let fallback_families: &[&str] = &[
        "Arial Unicode MS",
        "Noto Sans CJK SC",
        "Noto Sans SC",
        "PingFang SC",
    ];
    #[cfg(target_os = "linux")]
    let fallback_families: &[&str] = &[
        "Noto Sans CJK SC",
        "Noto Sans SC",
    ];
    #[cfg(target_os = "windows")]
    let fallback_families: &[&str] = &[
        "Arial Unicode MS",
        "Noto Sans CJK SC",
        "Noto Sans SC",
        "Microsoft YaHei",
    ];
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    let fallback_families: &[&str] = &[];

    for family in fallback_families {
        let query = fontdb::Query {
            families: &[fontdb::Family::Name(family)],
            weight: fontdb::Weight::NORMAL,
            stretch: fontdb::Stretch::Normal,
            style: fontdb::Style::Normal,
        };
        if let Some(id) = db.query(&query) {
            if let Some(pair) = db
                .with_face_data(id, |data, index| {
                    is_sfnt_container(data).then(|| (data.to_vec(), index))
                })
                .flatten()
            {
                let bytes = Arc::new(pair.0);
                eprintln!("[ratex-unicode-font] found via fontdb: {} (face index {})", family, pair.1);
                return Some((bytes, pair.1));
            }
        }
    }

    eprintln!("[ratex-unicode-font] no Unicode font found");
    None
}

enum FaceSelector<'a> {
    Index(u32),
    Family(&'a str),
}

/// Parse and load a font spec: `path` or `path#index` or `path#FamilyName`.
fn load_font_spec(spec: &str) -> Option<(Arc<Vec<u8>>, u32)> {
    let (path, selector) = if let Some((p, suffix)) = spec.rsplit_once('#') {
        if p.is_empty() || suffix.is_empty() {
            (spec, None)
        } else if let Ok(index) = suffix.parse::<u32>() {
            (p, Some(FaceSelector::Index(index)))
        } else {
            (p, Some(FaceSelector::Family(suffix)))
        }
    } else {
        (spec, None)
    };

    let bytes = std::fs::read(std::path::Path::new(path)).ok()?;
    if !is_sfnt_container(&bytes) {
        return None;
    }

    let face_index = match selector {
        None => 0,
        Some(FaceSelector::Index(idx)) => {
            let count = ttf_parser::fonts_in_collection(&bytes).unwrap_or(1);
            if idx >= count {
                return None;
            }
            idx
        }
        Some(FaceSelector::Family(family)) => {
            if is_sfnt_single_font(&bytes) {
                return None;
            }
            find_face_index_by_family(path, family)?
        }
    };

    Some((Arc::new(bytes), face_index))
}

fn find_face_index_by_family(path: &str, family_hint: &str) -> Option<u32> {
    let mut db = fontdb::Database::new();
    db.load_font_file(path).ok()?;
    let face_index = db.faces().find_map(|face| {
        face.families
            .iter()
            .any(|(name, _)| name == family_hint)
            .then_some(face.index)
    });
    face_index
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "macos")]
    fn test_load_font_spec_macos() {
        let ttf = "/Library/Fonts/Arial Unicode.ttf";
        if std::path::Path::new(ttf).exists() {
            let result = load_font_spec(ttf);
            assert!(result.is_some(), "Should load Arial Unicode.ttf");
            if let Some((bytes, face_index)) = result {
                assert!(!bytes.is_empty());
                assert_eq!(face_index, 0);
            }

            let result = load_font_spec(&format!("{ttf}#0"));
            assert!(result.is_some(), "Should load Arial Unicode.ttf#0");
            if let Some((_, face_index)) = result {
                assert_eq!(face_index, 0);
            }

            let result = load_font_spec(&format!("{ttf}#1"));
            assert!(result.is_none(), "Should fail for TTF with index > 0");

            let result = load_font_spec(&format!("{ttf}#Arial Unicode MS"));
            assert!(result.is_none(), "Should fail for TTF with family selector");
        } else {
            eprintln!("skipping Arial Unicode.ttf checks: {ttf} not found");
        }

        let ttc = "/System/Library/Fonts/PingFang.ttc";
        if std::path::Path::new(ttc).exists() {
            let result_family = load_font_spec(&format!("{ttc}#PingFang SC"));
            assert!(result_family.is_some(), "Should load PingFang.ttc with family name");

            let result_default = load_font_spec(ttc);
            assert!(result_default.is_some(), "Should load PingFang.ttc without selector");
            if let Some((_, face_index)) = result_default {
                assert_eq!(face_index, 0, "TTC without selector should default to face 0");
            }

            if let Some((_, face_index_family)) = result_family {
                let result_index =
                    load_font_spec(&format!("{ttc}#{face_index_family}"));
                assert!(result_index.is_some(), "Should load PingFang.ttc with index");
                if let Some((_, face_index_idx)) = result_index {
                    assert_eq!(
                        face_index_family, face_index_idx,
                        "Family and index should resolve to same face"
                    );
                }
            }

            let result = load_font_spec(&format!("{ttc}#0"));
            assert!(result.is_some(), "Should load PingFang.ttc#0");

            let result = load_font_spec(&format!("{ttc}#NonExistent Font"));
            assert!(result.is_none(), "Should fail for non-existent family name");
        } else {
            eprintln!("skipping PingFang.ttc checks: {ttc} not found");
        }
    }
}
