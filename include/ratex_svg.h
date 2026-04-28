#ifndef RATEX_SVG_H
#define RATEX_SVG_H

#include "ratex_base.h"

#ifdef __cplusplus
extern "C" {
#endif

/**
 * @file ratex_svg.h
 * @brief C/C++ API for RaTeX SVG rendering.
 *
 * This header builds on `ratex_base.h` and adds the SVG-specific options,
 * result types, and functions needed to render LaTeX math to SVG.
 */

/** SVG-specific rendering options */
typedef struct {
    size_t struct_size;   /**< sizeof(RatexSvgOptions), for forward compatibility */
    float font_size;      /**< Font size in user units. Default: 40.0 */
    float padding;        /**< Padding on all sides, in user units. Default: 10.0 */
    float stroke_width;   /**< Stroke width for unfilled paths. Default: 1.5 */
} RatexSvgOptions;

/** Result from SVG rendering */
typedef struct {
    char* data;           /**< UTF-8 SVG document (heap allocated), NULL on error */
    size_t len;           /**< Length of SVG payload in bytes, excluding trailing NUL */
    int error_code;       /**< 0 = success, non-zero = error */
} RatexSvgResult;

/**
 * @brief Render a LaTeX string to SVG.
 *
 * This function renders a LaTeX math string to a self-contained UTF-8 SVG
 * document.
 *
 * @param latex Null-terminated UTF-8 LaTeX string. Must not be NULL.
 * @param layout_opts Layout options (display mode, color). NULL = use defaults.
 * @param svg_opts SVG-specific options. NULL = use defaults.
 * @return RatexSvgResult containing the SVG document. Check error_code.
 *
 * @note The returned SVG data is NUL-terminated for convenience and must be freed
 *       with ratex_free_svg_result().
 * @note Glyphs are emitted as self-contained `<path>` outlines.
 *
 * Example:
 * @code
 * RatexSvgOptions svg_opts = {
 *     sizeof(RatexSvgOptions), 40.0f, 10.0f, 1.5f
 * };
 * RatexSvgResult result = ratex_render_to_svg(
 *     "\\frac{1}{2} + x", NULL, &svg_opts);
 * if (result.error_code == 0) {
 *     fwrite(result.data, 1, result.len, fopen("out.svg", "wb"));
 *     ratex_free_svg_result(result);
 * }
 * @endcode
 */
RatexSvgResult ratex_render_to_svg(
    const char* latex,
    const RatexOptions* layout_opts,
    const RatexSvgOptions* svg_opts
);

/**
 * @brief Free an SVG result returned by ratex_render_to_svg.
 *
 * @param result The result struct returned by ratex_render_to_svg().
 *
 * @note Calling this with a result that has NULL data is a no-op.
 * @note Do not call free() directly on result.data.
 */
void ratex_free_svg_result(RatexSvgResult result);

#ifdef __cplusplus
}  /* extern "C" */
#endif

#endif  /* RATEX_SVG_H */
