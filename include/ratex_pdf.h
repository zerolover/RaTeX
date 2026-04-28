#ifndef RATEX_PDF_H
#define RATEX_PDF_H

#include "ratex_base.h"

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/**
 * @file ratex_pdf.h
 * @brief C/C++ API for RaTeX PDF rendering.
 *
 * This header builds on `ratex_base.h` and adds the PDF-specific options,
 * result types, and functions needed to render LaTeX math to PDF.
 */

/** PDF-specific rendering options */
typedef struct {
    size_t struct_size;   /**< sizeof(RatexPdfOptions), for forward compatibility */
    float font_size;      /**< Font size in user units. Default: 40.0 */
    float padding;        /**< Padding on all sides, in user units. Default: 10.0 */
    float stroke_width;   /**< Stroke width for unfilled paths. Default: 1.5 */
    int show_baseline;    /**< If non-zero, draw a short LaTeX-style baseline marker. Default: 0 */
} RatexPdfOptions;

/** Result from PDF rendering */
typedef struct {
    uint8_t* data;        /**< PDF byte buffer (heap allocated), NULL on error */
    size_t len;           /**< Length of PDF buffer in bytes */
    int error_code;       /**< 0 = success, non-zero = error */
} RatexPdfResult;

/* ==========================================================================
 * API Functions
 * ========================================================================== */

/**
 * @brief Render a LaTeX string to PDF.
 *
 * This function renders a LaTeX math string to a PDF document containing the
 * formatted formula.
 *
 * @param latex Null-terminated UTF-8 LaTeX string. Must not be NULL.
 * @param layout_opts Layout options (display mode, color). NULL = use defaults.
 * @param pdf_opts PDF-specific options (font size, padding). NULL = use defaults.
 * @return RatexPdfResult containing the PDF data. Check error_code.
 *
 * @note The returned PDF data must be freed with ratex_free_pdf_result().
 * @note PDF export always uses bundled KaTeX fonts.
 *
 * Example:
 * @code
 * RatexPdfOptions pdf_opts = {
 *     sizeof(RatexPdfOptions),
 *     48.0f,    // font_size
 *     20.0f,    // padding
 *     2.0f,     // stroke_width
 *     0         // show_baseline
 * };
 * RatexPdfResult result = ratex_render_to_pdf(
 *     "\\frac{1}{2} + \\sqrt{x}", NULL, &pdf_opts);
 * if (result.error_code == 0) {
 *     fwrite(result.data, 1, result.len, fopen("out.pdf", "wb"));
 *     ratex_free_pdf_result(result);
 * }
 * @endcode
 */
RatexPdfResult ratex_render_to_pdf(
    const char* latex,
    const RatexOptions* layout_opts,
    const RatexPdfOptions* pdf_opts
);

/**
 * @brief Free a PDF result returned by ratex_render_to_pdf.
 *
 * @param result The result struct returned by ratex_render_to_pdf().
 *
 * @note Calling this with a result that has NULL data is a no-op.
 * @note Do not call free() directly on result.data.
 */
void ratex_free_pdf_result(RatexPdfResult result);

#ifdef __cplusplus
}  /* extern "C" */
#endif

#endif  /* RATEX_PDF_H */
