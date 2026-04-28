#ifndef RATEX_BASE_H
#define RATEX_BASE_H

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/**
 * @file ratex_base.h
 * @brief Shared C/C++ ABI definitions for RaTeX.
 *
 * This header defines the common constants, structs, and utility functions
 * shared by the format-specific RaTeX APIs.
 */

/* ==========================================================================
 * Constants
 * ========================================================================== */

/** Display mode: inline style (equivalent to $...$) */
#define RATEX_DISPLAY_MODE_INLINE 0
/** Display mode: block style (equivalent to $$...$$) */
#define RATEX_DISPLAY_MODE_DISPLAY 1

/* ==========================================================================
 * Shared Types
 * ========================================================================== */

/** RGBA color with normalized components [0, 1] */
typedef struct {
    float r;  /**< Red component [0, 1] */
    float g;  /**< Green component [0, 1] */
    float b;  /**< Blue component [0, 1] */
    float a;  /**< Alpha component [0, 1] */
} RatexColor;

/** Shared layout options for formula rendering */
typedef struct {
    size_t struct_size;   /**< sizeof(RatexOptions), for forward compatibility */
    int display_mode;     /**< 0 = inline, 1 = display (see RATEX_DISPLAY_MODE_*) */
    const RatexColor* color;  /**< Default formula color, NULL = black */
} RatexOptions;

/** Result from DisplayList JSON export */
typedef struct {
    char* data;           /**< JSON display list string, NULL on error */
    int error_code;       /**< 0 = success, non-zero = error */
} RatexResult;

/* ==========================================================================
 * Shared API Functions
 * ========================================================================== */

/**
 * @brief Parse LaTeX and return DisplayList as JSON.
 *
 * This function parses a LaTeX math string and returns the intermediate
 * DisplayList representation encoded as UTF-8 JSON.
 *
 * @param latex Null-terminated UTF-8 LaTeX string. Must not be NULL.
 * @param opts Layout options. NULL = use defaults.
 * @return RatexResult containing JSON display list. Check error_code.
 *
 * @note The returned JSON string must be freed with ratex_free_display_list().
 */
RatexResult ratex_parse_and_layout(
    const char* latex,
    const RatexOptions* opts
);

/**
 * @brief Free a display list JSON string.
 *
 * @param ptr The JSON string returned by ratex_parse_and_layout().
 *
 * @note Passing NULL is a no-op.
 * @note Do not call free() directly on this pointer.
 */
void ratex_free_display_list(char* ptr);

/**
 * @brief Get the last error message.
 *
 * Returns the last error produced by a RaTeX API call on the current thread.
 *
 * @return Null-terminated error string, or NULL if no error occurred.
 *         The pointer is only valid until the next RaTeX API call.
 */
const char* ratex_get_last_error(void);

/**
 * @brief Set a custom Unicode fallback font.
 *
 * Spec format: "path", "path#index", or "path#FamilyName".
 *
 * @param spec Font specification string. Must not be NULL.
 * @return 1 if successful, 0 if the font could not be loaded.
 *
 * @note This overrides RATEX_UNICODE_FONT environment variable.
 * @note On success, cached CJK fallback font data is invalidated so subsequent
 *       renders use the new font.
 */
int ratex_set_unicode_font(const char* spec);

/**
 * @brief Clear the cached Unicode font.
 *
 * Forces re-discovery on next access and invalidates cached CJK fallback font data.
 */
void ratex_clear_unicode_font(void);

#ifdef __cplusplus
}  /* extern "C" */
#endif

#endif  /* RATEX_BASE_H */
