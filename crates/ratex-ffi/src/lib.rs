//! RaTeX C ABI FFI exports for native platform integration.
//!
//! Platform-specific modules:
//! - `jni` — Android JNI bridge (compiled only on `target_os = "android"`)
//!
//! ## DisplayList JSON protocol
//!
//! The primary output of this crate is a UTF-8 JSON string representing a `DisplayList`.
//! Treat this JSON as a **public protocol**: decoders should ignore unknown fields and
//! tolerate missing optional fields for forward/backward compatibility.
//!
//! See `docs/DISPLAYLIST_JSON_PROTOCOL.md` in the repository for the full schema and
//! change policy.
//!
//! # Usage (C)
//! ```c
//! RatexColor black = {0, 0, 0, 1};
//! RatexOptions opts = { sizeof(RatexOptions), 1, &black };  // display_mode=1 (block)
//! RatexResult result = ratex_parse_and_layout("\\frac{1}{2}", &opts);
//! if (result.error_code == 0) {
//!     // consume result.data ...
//!     ratex_free_display_list(result.data);
//! } else {
//!     const char* err = ratex_get_last_error();
//!     // handle error...
//! }
//! ```

#[cfg(target_os = "android")]
pub mod jni;

use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};

use ratex_layout::{layout, to_display_list, LayoutOptions};
use ratex_parser::parse;
use ratex_pdf::{render_to_pdf, PdfOptions};
use ratex_svg::{render_to_svg, SvgOptions};
use ratex_types::display_item::DisplayList;
use ratex_types::{color::Color, math_style::MathStyle};
use serde_json::Value;

// Thread-local storage for the last error message.
thread_local! {
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

fn set_last_error(msg: &str) {
    let bytes: Vec<u8> = msg.bytes().filter(|&b| b != 0).collect();
    let stored = CString::new(bytes).unwrap_or_else(|_| {
        CString::new("(error message could not be encoded)").expect("static C string")
    });
    LAST_ERROR.with(|cell| {
        *cell.borrow_mut() = Some(stored);
    });
}

fn clear_last_error() {
    LAST_ERROR.with(|cell| {
        *cell.borrow_mut() = None;
    });
}

/// Replace non-finite floats with 0 to produce valid JSON.
fn sanitize_json_numbers(v: Value) -> Value {
    match v {
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                if f.is_finite() {
                    Value::Number(n)
                } else {
                    Value::Number(serde_json::Number::from_f64(0.0).unwrap())
                }
            } else {
                Value::Number(n)
            }
        }
        Value::Array(arr) => Value::Array(arr.into_iter().map(sanitize_json_numbers).collect()),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| (k, sanitize_json_numbers(v)))
                .collect(),
        ),
        other => other,
    }
}

fn build_display_list(
    latex_str: &str,
    style: MathStyle,
    color: Color,
) -> Result<DisplayList, String> {
    let nodes = parse(latex_str).map_err(|e| format!("parse error: {e}"))?;
    let options = LayoutOptions::default().with_style(style).with_color(color);
    let layout_box = layout(&nodes, &options);
    Ok(to_display_list(&layout_box))
}

fn do_layout(latex_str: &str, style: MathStyle, color: Color) -> Result<String, String> {
    let display_list = build_display_list(latex_str, style, color)?;
    let value =
        serde_json::to_value(&display_list).map_err(|e| format!("serialization error: {e}"))?;
    let mut sanitized = sanitize_json_numbers(value);
    // Add a protocol version at the top level for forward-compatible decoding.
    if let Value::Object(ref mut map) = sanitized {
        map.insert("version".to_string(), Value::Number(1.into()));
    }
    serde_json::to_string(&sanitized).map_err(|e| format!("JSON stringify error: {e}"))
}

fn resolve_layout_config(opts: *const RatexOptions) -> Result<(MathStyle, Color), String> {
    let style = if opts.is_null() {
        MathStyle::Display
    } else {
        let opts_ref = unsafe { &*opts };
        let min_size =
            std::mem::offset_of!(RatexOptions, display_mode) + std::mem::size_of::<c_int>();
        if opts_ref.struct_size >= min_size && opts_ref.display_mode == 0 {
            MathStyle::Text
        } else {
            MathStyle::Display
        }
    };

    let color = if opts.is_null() {
        Color::BLACK
    } else {
        let opts_ref = unsafe { &*opts };
        let color_size =
            std::mem::offset_of!(RatexOptions, color) + std::mem::size_of::<*const RatexColor>();

        if opts_ref.struct_size >= color_size && !opts_ref.color.is_null() {
            validate_color(unsafe { *opts_ref.color })?
        } else {
            Color::BLACK
        }
    };

    Ok((style, color))
}

// ---------------------------------------------------------------------------
// Public structs
// ---------------------------------------------------------------------------

/// Options for [`ratex_parse_and_layout`].
///
/// Always set `struct_size = sizeof(RatexOptions)` before passing to the function.
/// Fields beyond `struct_size` are ignored, enabling forward compatibility.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RatexColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl RatexColor {
    pub const BLACK: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
}

impl From<RatexColor> for ratex_types::color::Color {
    fn from(value: RatexColor) -> Self {
        Self::new(value.r, value.g, value.b, value.a)
    }
}

fn validate_color(color: RatexColor) -> Result<ratex_types::color::Color, String> {
    fn validate_component(name: &str, value: f32) -> Result<(), String> {
        if !value.is_finite() {
            return Err(format!(
                "invalid color.{name}: expected a finite float in [0, 1], got {value}"
            ));
        }
        if !(0.0..=1.0).contains(&value) {
            return Err(format!(
                "invalid color.{name}: expected a float in [0, 1], got {value}"
            ));
        }
        Ok(())
    }

    validate_component("r", color.r)?;
    validate_component("g", color.g)?;
    validate_component("b", color.b)?;
    validate_component("a", color.a)?;

    Ok(color.into())
}

#[repr(C)]
pub struct RatexOptions {
    /// Must be set to `sizeof(RatexOptions)` by the caller.
    pub struct_size: usize,
    /// Rendering mode:
    /// - `0` — inline (text style, equivalent to `$...$`)
    /// - `1` — display block (display style, equivalent to `$$...$$`)
    pub display_mode: c_int,
    /// Default formula color, in normalized RGBA.
    ///
    /// Explicit LaTeX color commands like `\color{...}` / `\textcolor{...}{...}`
    /// still override this per subtree.
    pub color: *const RatexColor,
}

/// Result returned by [`ratex_parse_and_layout`].
///
/// On success: `error_code == 0` and `data` is a heap-allocated JSON string;
/// free it with [`ratex_free_display_list`].
/// On error: `error_code != 0`, `data` is NULL; call [`ratex_get_last_error`] for details.
#[repr(C)]
pub struct RatexResult {
    /// JSON display list on success, NULL on error.
    pub data: *mut c_char,
    /// `0` on success, non-zero on error.
    pub error_code: c_int,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse a LaTeX string and compute its display list with explicit rendering options.
///
/// Pass `opts = NULL` to use display-mode defaults.
///
/// # Safety
/// - `latex` must be a valid non-null null-terminated UTF-8 C string.
/// - `opts` may be NULL. If non-null it must point to a valid `RatexOptions` whose
///   `struct_size` field is set correctly.
#[no_mangle]
pub unsafe extern "C" fn ratex_parse_and_layout(
    latex: *const c_char,
    opts: *const RatexOptions,
) -> RatexResult {
    let err_result = |msg: &str| -> RatexResult {
        set_last_error(msg);
        RatexResult {
            data: std::ptr::null_mut(),
            error_code: 1,
        }
    };

    clear_last_error();

    if latex.is_null() {
        return err_result("ratex_parse_and_layout: latex pointer is null");
    }

    let latex_str = match unsafe { CStr::from_ptr(latex) }.to_str() {
        Ok(s) => s,
        Err(e) => return err_result(&format!("invalid UTF-8 in latex string: {e}")),
    };

    let (style, color) = match resolve_layout_config(opts) {
        Ok(cfg) => cfg,
        Err(msg) => return err_result(&msg),
    };

    match do_layout(latex_str, style, color) {
        Ok(json) => match CString::new(json) {
            Ok(cs) => RatexResult {
                data: cs.into_raw(),
                error_code: 0,
            },
            Err(e) => err_result(&format!("JSON contains interior null byte: {e}")),
        },
        Err(e) => err_result(&e),
    }
}

/// Free a display list JSON string returned by [`ratex_parse_and_layout`].
///
/// Passing NULL is a no-op.
///
/// # Safety
/// `ptr` must have been returned by [`ratex_parse_and_layout`] and must not be freed twice.
#[no_mangle]
pub unsafe extern "C" fn ratex_free_display_list(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe { drop(CString::from_raw(ptr)) };
    }
}

/// Return the last error message set by any layout function on this thread.
///
/// # Returns
/// - A pointer to a null-terminated error string, valid until the next layout call on this thread.
/// - NULL if no error has occurred on this thread.
///
/// # Safety
/// The returned pointer is only valid for the lifetime of the current thread and until the
/// next call to a layout function on this thread.
#[no_mangle]
pub extern "C" fn ratex_get_last_error() -> *const c_char {
    LAST_ERROR.with(|cell| {
        cell.borrow()
            .as_ref()
            .map(|cs| cs.as_ptr())
            .unwrap_or(std::ptr::null())
    })
}

/// Set a custom Unicode fallback font.
///
/// Spec format: "path", "path#index", or "path#FamilyName".
///
/// # Returns
/// - `1` if the font was successfully loaded and set.
/// - `0` if the font could not be loaded.
///
/// # Safety
/// `spec` must be a valid non-null null-terminated UTF-8 C string.
///
/// # Note
/// On success, this updates the Unicode font cache and clears the font loader's
/// cached `CjkRegular` entries so subsequent renders use the new font.
#[no_mangle]
pub unsafe extern "C" fn ratex_set_unicode_font(spec: *const c_char) -> c_int {
    if spec.is_null() {
        return 0;
    }
    let Ok(spec_str) = unsafe { CStr::from_ptr(spec) }.to_str() else {
        return 0;
    };
    if ratex_unicode_font::set_unicode_font(spec_str) {
        ratex_font_loader::clear_unicode_font_cache();
        1
    } else {
        0
    }
}

/// Clear the cached Unicode font, forcing re-discovery on next access.
#[no_mangle]
pub extern "C" fn ratex_clear_unicode_font() {
    ratex_unicode_font::clear_unicode_font();
    ratex_font_loader::clear_unicode_font_cache();
}

// ---------------------------------------------------------------------------
// PDF Export API
// ---------------------------------------------------------------------------

/// Options for [`ratex_render_to_pdf`].
///
/// Always set `struct_size = sizeof(RatexPdfOptions)` before passing to the function.
/// Fields beyond `struct_size` are ignored, enabling forward compatibility.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RatexPdfOptions {
    /// Must be set to `sizeof(RatexPdfOptions)` by the caller.
    pub struct_size: usize,
    /// Font size in user units. Default: 40.0
    pub font_size: f32,
    /// Padding on all sides, in user units. Default: 10.0
    pub padding: f32,
    /// Stroke width for unfilled paths, in user units. Default: 1.5
    pub stroke_width: f32,
    /// If non-zero, draw a short LaTeX-style baseline marker at the formula baseline.
    /// Default: 0 (disabled)
    pub show_baseline: c_int,
}

/// Result returned by [`ratex_render_to_pdf`].
///
/// On success: `error_code == 0` and `data` is a heap-allocated PDF byte buffer;
/// free it with [`ratex_free_pdf_result`].
/// On error: `error_code != 0`, `data` is NULL; call [`ratex_get_last_error`] for details.
#[repr(C)]
pub struct RatexPdfResult {
    /// PDF byte buffer on success, NULL on error.
    pub data: *mut u8,
    /// Length of the PDF buffer in bytes.
    pub len: usize,
    /// `0` on success, non-zero on error.
    pub error_code: c_int,
}

/// Render a LaTeX string to PDF with explicit options.
///
/// Pass `layout_opts = NULL` to use display-mode defaults.
/// Pass `pdf_opts = NULL` to use default PDF settings.
///
/// # Safety
/// - `latex` must be a valid non-null null-terminated UTF-8 C string.
/// - `layout_opts` may be NULL. If non-null it must point to a valid `RatexOptions`.
/// - `pdf_opts` may be NULL. If non-null it must point to a valid `RatexPdfOptions`.
#[no_mangle]
pub unsafe extern "C" fn ratex_render_to_pdf(
    latex: *const c_char,
    layout_opts: *const RatexOptions,
    pdf_opts: *const RatexPdfOptions,
) -> RatexPdfResult {
    let err_result = |msg: &str| -> RatexPdfResult {
        set_last_error(msg);
        RatexPdfResult {
            data: std::ptr::null_mut(),
            len: 0,
            error_code: 1,
        }
    };

    clear_last_error();

    if latex.is_null() {
        return err_result("ratex_render_to_pdf: latex pointer is null");
    }

    let latex_str = match unsafe { CStr::from_ptr(latex) }.to_str() {
        Ok(s) => s,
        Err(e) => return err_result(&format!("invalid UTF-8 in latex string: {e}")),
    };

    let (style, color) = match resolve_layout_config(layout_opts) {
        Ok(cfg) => cfg,
        Err(msg) => return err_result(&msg),
    };

    // Parse PDF options
    let pdf_options = if pdf_opts.is_null() {
        PdfOptions::default()
    } else {
        let opts_ref = unsafe { &*pdf_opts };
        let mut pdf_opts = PdfOptions::default();

        // Check font_size field
        let font_size_offset = std::mem::offset_of!(RatexPdfOptions, font_size);
        if opts_ref.struct_size >= font_size_offset + std::mem::size_of::<f32>() {
            if opts_ref.font_size > 0.0 && opts_ref.font_size.is_finite() {
                pdf_opts.font_size = opts_ref.font_size as f64;
            }
        }

        // Check padding field
        let padding_offset = std::mem::offset_of!(RatexPdfOptions, padding);
        if opts_ref.struct_size >= padding_offset + std::mem::size_of::<f32>() {
            if opts_ref.padding >= 0.0 && opts_ref.padding.is_finite() {
                pdf_opts.padding = opts_ref.padding as f64;
            }
        }

        // Check stroke_width field
        let stroke_width_offset = std::mem::offset_of!(RatexPdfOptions, stroke_width);
        if opts_ref.struct_size >= stroke_width_offset + std::mem::size_of::<f32>() {
            if opts_ref.stroke_width > 0.0 && opts_ref.stroke_width.is_finite() {
                pdf_opts.stroke_width = opts_ref.stroke_width as f64;
            }
        }

        // Check show_baseline field
        let show_baseline_offset = std::mem::offset_of!(RatexPdfOptions, show_baseline);
        if opts_ref.struct_size >= show_baseline_offset + std::mem::size_of::<c_int>() {
            pdf_opts.show_baseline = opts_ref.show_baseline != 0;
        }

        pdf_opts
    };

    // Parse and layout
    let display_list = match build_display_list(latex_str, style, color) {
        Ok(list) => list,
        Err(msg) => return err_result(&msg),
    };

    // Render to PDF
    match render_to_pdf(&display_list, &pdf_options) {
        Ok(pdf_bytes) => {
            let len = pdf_bytes.len();
            // Convert Vec to boxed slice and leak it to give ownership to C
            let boxed = pdf_bytes.into_boxed_slice();
            let ptr = Box::into_raw(boxed) as *mut u8;
            RatexPdfResult {
                data: ptr,
                len,
                error_code: 0,
            }
        }
        Err(e) => err_result(&format!("PDF render error: {e}")),
    }
}

/// Free a PDF result returned by [`ratex_render_to_pdf`].
///
/// Passing a result with NULL data is a no-op.
///
/// # Safety
/// `result.data` must have been returned by [`ratex_render_to_pdf`] and must not be freed twice.
#[no_mangle]
pub unsafe extern "C" fn ratex_free_pdf_result(result: RatexPdfResult) {
    if !result.data.is_null() && result.len > 0 {
        // Reconstruct the Box<[u8]> and drop it
        let _ = Box::from_raw(std::slice::from_raw_parts_mut(result.data, result.len));
    }
}

// ---------------------------------------------------------------------------
// SVG Export API
// ---------------------------------------------------------------------------

/// Options for [`ratex_render_to_svg`].
///
/// Always set `struct_size = sizeof(RatexSvgOptions)` before passing to the function.
/// Fields beyond `struct_size` are ignored, enabling forward compatibility.
/// Glyphs are emitted as self-contained `<path>` outlines.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RatexSvgOptions {
    /// Must be set to `sizeof(RatexSvgOptions)` by the caller.
    pub struct_size: usize,
    /// Font size in user units. Default: 40.0
    pub font_size: f32,
    /// Padding on all sides, in user units. Default: 10.0
    pub padding: f32,
    /// Stroke width for unfilled paths, in user units. Default: 1.5
    pub stroke_width: f32,
}

/// Result returned by [`ratex_render_to_svg`].
///
/// On success: `error_code == 0` and `data` is a heap-allocated UTF-8 SVG document;
/// free it with [`ratex_free_svg_result`].
/// On error: `error_code != 0`, `data` is NULL; call [`ratex_get_last_error`] for details.
#[repr(C)]
pub struct RatexSvgResult {
    /// UTF-8 SVG document on success, NULL on error. Guaranteed to be NUL-terminated.
    pub data: *mut c_char,
    /// Length of the SVG payload in bytes, excluding the trailing NUL.
    pub len: usize,
    /// `0` on success, non-zero on error.
    pub error_code: c_int,
}

/// Render a LaTeX string to SVG with explicit options.
///
/// Pass `layout_opts = NULL` to use display-mode defaults.
/// Pass `svg_opts = NULL` to use default SVG settings.
///
/// # Safety
/// - `latex` must be a valid non-null null-terminated UTF-8 C string.
/// - `layout_opts` may be NULL. If non-null it must point to a valid `RatexOptions`.
/// - `svg_opts` may be NULL. If non-null it must point to a valid `RatexSvgOptions`.
#[no_mangle]
pub unsafe extern "C" fn ratex_render_to_svg(
    latex: *const c_char,
    layout_opts: *const RatexOptions,
    svg_opts: *const RatexSvgOptions,
) -> RatexSvgResult {
    let err_result = |msg: &str| -> RatexSvgResult {
        set_last_error(msg);
        RatexSvgResult {
            data: std::ptr::null_mut(),
            len: 0,
            error_code: 1,
        }
    };

    clear_last_error();

    if latex.is_null() {
        return err_result("ratex_render_to_svg: latex pointer is null");
    }

    let latex_str = match unsafe { CStr::from_ptr(latex) }.to_str() {
        Ok(s) => s,
        Err(e) => return err_result(&format!("invalid UTF-8 in latex string: {e}")),
    };

    let (style, color) = match resolve_layout_config(layout_opts) {
        Ok(cfg) => cfg,
        Err(msg) => return err_result(&msg),
    };

    let svg_options = if svg_opts.is_null() {
        SvgOptions::default()
    } else {
        let opts_ref = unsafe { &*svg_opts };
        let mut svg_opts = SvgOptions::default();

        let font_size_offset = std::mem::offset_of!(RatexSvgOptions, font_size);
        if opts_ref.struct_size >= font_size_offset + std::mem::size_of::<f32>() {
            if opts_ref.font_size > 0.0 && opts_ref.font_size.is_finite() {
                svg_opts.font_size = opts_ref.font_size as f64;
            }
        }

        let padding_offset = std::mem::offset_of!(RatexSvgOptions, padding);
        if opts_ref.struct_size >= padding_offset + std::mem::size_of::<f32>() {
            if opts_ref.padding >= 0.0 && opts_ref.padding.is_finite() {
                svg_opts.padding = opts_ref.padding as f64;
            }
        }

        let stroke_width_offset = std::mem::offset_of!(RatexSvgOptions, stroke_width);
        if opts_ref.struct_size >= stroke_width_offset + std::mem::size_of::<f32>() {
            if opts_ref.stroke_width > 0.0 && opts_ref.stroke_width.is_finite() {
                svg_opts.stroke_width = opts_ref.stroke_width as f64;
            }
        }

        svg_opts
    };

    let display_list = match build_display_list(latex_str, style, color) {
        Ok(list) => list,
        Err(msg) => return err_result(&msg),
    };

    let svg = render_to_svg(&display_list, &svg_options);
    let len = svg.len();
    let mut bytes = svg.into_bytes();
    bytes.push(0);
    let ptr = Box::into_raw(bytes.into_boxed_slice()) as *mut c_char;

    RatexSvgResult {
        data: ptr,
        len,
        error_code: 0,
    }
}

/// Free an SVG result returned by [`ratex_render_to_svg`].
///
/// Passing a result with NULL data is a no-op.
///
/// # Safety
/// `result.data` must have been returned by [`ratex_render_to_svg`] and must not be freed twice.
#[no_mangle]
pub unsafe extern "C" fn ratex_free_svg_result(result: RatexSvgResult) {
    if !result.data.is_null() {
        let _ = Box::from_raw(std::slice::from_raw_parts_mut(
            result.data as *mut u8,
            result.len.saturating_add(1),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::ffi::CString;

    /// Assert the default formula color applied to the first `GlyphPath` in the protocol JSON is black.
    ///
    /// We key off `type == "GlyphPath"` (see `docs/DISPLAYLIST_JSON_PROTOCOL.md`) instead of “first
    /// item with any `color`”, so fraction bars or paths cannot satisfy the assertion by accident.
    fn assert_default_glyph_path_color_is_black(json: &str) {
        let v: Value = serde_json::from_str(json).expect("valid display list JSON");
        let items = v
            .get("items")
            .and_then(|i| i.as_array())
            .expect("display list must have items array");
        let glyph = items
            .iter()
            .find(|item| {
                item.get("type")
                    .and_then(|t| t.as_str())
                    .is_some_and(|ty| ty == "GlyphPath")
            })
            .expect("expected at least one GlyphPath item");
        let color = glyph
            .get("color")
            .expect("GlyphPath must include color per DISPLAYLIST_JSON_PROTOCOL");
        let r = color.get("r").and_then(|x| x.as_f64());
        let g = color.get("g").and_then(|x| x.as_f64());
        let b = color.get("b").and_then(|x| x.as_f64());
        let a = color.get("a").and_then(|x| x.as_f64());
        assert_eq!((r, g, b, a), (Some(0.0), Some(0.0), Some(0.0), Some(1.0)));
    }

    fn call(latex: &str, display_mode: c_int) -> Option<String> {
        let input = CString::new(latex).unwrap();
        let black = RatexColor::BLACK;
        let opts = RatexOptions {
            struct_size: std::mem::size_of::<RatexOptions>(),
            display_mode,
            color: &black,
        };
        let result = unsafe { ratex_parse_and_layout(input.as_ptr(), &opts) };
        if result.error_code != 0 || result.data.is_null() {
            return None;
        }
        let json = unsafe { CStr::from_ptr(result.data) }
            .to_str()
            .unwrap()
            .to_owned();
        unsafe { ratex_free_display_list(result.data) };
        Some(json)
    }

    fn svg_call(latex: &str) -> Option<String> {
        let input = CString::new(latex).unwrap();
        let opts = RatexSvgOptions {
            struct_size: std::mem::size_of::<RatexSvgOptions>(),
            font_size: 24.0,
            padding: 8.0,
            stroke_width: 1.5,
        };
        let result = unsafe { ratex_render_to_svg(input.as_ptr(), std::ptr::null(), &opts) };
        if result.error_code != 0 || result.data.is_null() {
            return None;
        }
        let svg = unsafe { std::slice::from_raw_parts(result.data as *const u8, result.len) };
        let svg = std::str::from_utf8(svg).unwrap().to_owned();
        unsafe { ratex_free_svg_result(result) };
        Some(svg)
    }

    #[test]
    fn svg_output_uses_embedded_paths() {
        let svg = svg_call("x^2").expect("svg render should succeed");
        assert!(svg.contains("<path"));
        assert!(svg.contains("fill-rule=\"nonzero\""));
        assert!(!svg.contains("<text"));
    }

    #[test]
    fn display_fraction() {
        let json = call(r"\frac{1}{2}", 1).expect("should not fail");
        assert!(json.starts_with('{'));
        assert!(json.contains("items"));
    }

    #[test]
    fn inline_fraction() {
        let json = call(r"\frac{1}{2}", 0).expect("should not fail");
        assert!(json.contains("items"));
    }

    #[test]
    fn display_expression() {
        let json = call("x^2 + y^2 = z^2", 1).expect("should not fail");
        assert!(json.contains("items"));
    }

    #[test]
    fn null_latex_returns_error() {
        let black = RatexColor::BLACK;
        let opts = RatexOptions {
            struct_size: std::mem::size_of::<RatexOptions>(),
            display_mode: 1,
            color: &black,
        };
        let result = unsafe { ratex_parse_and_layout(std::ptr::null(), &opts) };
        assert_ne!(result.error_code, 0);
        assert!(result.data.is_null());
        let err = ratex_get_last_error();
        assert!(!err.is_null());
        let msg = unsafe { CStr::from_ptr(err) }.to_str().unwrap();
        assert!(msg.contains("null"));
    }

    #[test]
    fn null_opts_defaults_to_display() {
        let input = CString::new(r"x^2").unwrap();
        let result = unsafe { ratex_parse_and_layout(input.as_ptr(), std::ptr::null()) };
        assert_eq!(result.error_code, 0);
        assert!(!result.data.is_null());
        unsafe { ratex_free_display_list(result.data) };
    }

    #[test]
    fn free_null_is_noop() {
        unsafe { ratex_free_display_list(std::ptr::null_mut()) };
    }

    #[test]
    fn error_on_bad_latex() {
        let result = call(r"\undefined{x}", 1);
        if result.is_none() {
            let err = ratex_get_last_error();
            assert!(!err.is_null());
        }
    }

    #[test]
    fn custom_color_applies_without_overriding_explicit_latex_color() {
        let input = CString::new(r"x + \color{red}{y}").unwrap();
        let blue = RatexColor {
            r: 0.0,
            g: 0.0,
            b: 1.0,
            a: 1.0,
        };
        let opts = RatexOptions {
            struct_size: std::mem::size_of::<RatexOptions>(),
            display_mode: 1,
            color: &blue,
        };
        let result = unsafe { ratex_parse_and_layout(input.as_ptr(), &opts) };
        assert_eq!(result.error_code, 0);
        let json = unsafe { CStr::from_ptr(result.data) }
            .to_str()
            .unwrap()
            .to_owned();
        unsafe { ratex_free_display_list(result.data) };

        assert!(json.contains("\"b\":1.0"));
        assert!(json.contains("\"r\":1.0"));
    }

    #[repr(C)]
    struct LegacyRatexOptions {
        struct_size: usize,
        display_mode: c_int,
    }

    #[test]
    fn short_legacy_options_remain_binary_compatible() {
        let input = CString::new("x").unwrap();
        let legacy_opts = LegacyRatexOptions {
            struct_size: std::mem::size_of::<LegacyRatexOptions>(),
            display_mode: 1,
        };

        let result = unsafe {
            ratex_parse_and_layout(
                input.as_ptr(),
                &legacy_opts as *const LegacyRatexOptions as *const RatexOptions,
            )
        };
        assert_eq!(result.error_code, 0);
        assert!(!result.data.is_null());

        let json = unsafe { CStr::from_ptr(result.data) }
            .to_str()
            .unwrap()
            .to_owned();
        unsafe { ratex_free_display_list(result.data) };

        // Old callers do not provide the color tail, so layout must fall back to black.
        assert_default_glyph_path_color_is_black(&json);
    }

    #[test]
    fn invalid_color_returns_error() {
        let input = CString::new("x").unwrap();
        let invalid = RatexColor {
            r: f32::NAN,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        };
        let opts = RatexOptions {
            struct_size: std::mem::size_of::<RatexOptions>(),
            display_mode: 1,
            color: &invalid,
        };

        let result = unsafe { ratex_parse_and_layout(input.as_ptr(), &opts) };
        assert_ne!(result.error_code, 0);
        assert!(result.data.is_null());

        let err = ratex_get_last_error();
        assert!(!err.is_null());
        let msg = unsafe { CStr::from_ptr(err) }.to_str().unwrap();
        assert!(msg.contains("invalid color.r"));
    }

    #[test]
    fn null_color_pointer_defaults_to_black() {
        let input = CString::new("x").unwrap();
        let opts = RatexOptions {
            struct_size: std::mem::size_of::<RatexOptions>(),
            display_mode: 1,
            color: std::ptr::null(),
        };

        let result = unsafe { ratex_parse_and_layout(input.as_ptr(), &opts) };
        assert_eq!(result.error_code, 0);
        assert!(!result.data.is_null());

        let json = unsafe { CStr::from_ptr(result.data) }
            .to_str()
            .unwrap()
            .to_owned();
        unsafe { ratex_free_display_list(result.data) };

        assert_default_glyph_path_color_is_black(&json);
    }
}
