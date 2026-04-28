/**
 * @file render.cpp
 * @brief C++ example for RaTeX PDF and SVG rendering
 *
 * This example demonstrates how to use the RaTeX C API from C++
 * to render LaTeX math formulas to PDF and SVG documents.
 */

#include "ratex_pdf.h"
#include "ratex_svg.h"
#include "ratex_base.h"
#include <chrono>
#include <fstream>
#include <iostream>
#include <string>
#include <vector>

// Helper function to save PDF to file
bool save_pdf(const char* filename, const uint8_t* data, size_t len) {
    std::ofstream file(filename, std::ios::binary);
    if (!file) {
        std::cerr << "Failed to open file: " << filename << std::endl;
        return false;
    }
    file.write(reinterpret_cast<const char*>(data), static_cast<std::streamsize>(len));
    file.close();
    return file.good();
}

// Helper function to save SVG to file
bool save_svg(const char* filename, const char* data, size_t len) {
    std::ofstream file(filename, std::ios::binary);
    if (!file) {
        std::cerr << "Failed to open file: " << filename << std::endl;
        return false;
    }
    file.write(data, static_cast<std::streamsize>(len));
    file.close();
    return file.good();
}

bool render_pdf_to_file(const char* latex,
                        const RatexOptions* layout_opts,
                        const RatexPdfOptions* pdf_opts,
                        const char* filename) {
    RatexPdfResult result = ratex_render_to_pdf(latex, layout_opts, pdf_opts);

    if (result.error_code == 0) {
        if (save_pdf(filename, result.data, result.len)) {
            std::cout << "Saved " << filename << " (" << result.len << " bytes)" << std::endl;
        }
        ratex_free_pdf_result(result);
        return true;
    }

    std::cerr << "Error: " << ratex_get_last_error() << std::endl;
    return false;
}

bool render_svg_to_file(const char* latex,
                        const RatexOptions* layout_opts,
                        const RatexSvgOptions* svg_opts,
                        const char* filename) {
    RatexSvgResult result = ratex_render_to_svg(latex, layout_opts, svg_opts);

    if (result.error_code == 0) {
        if (save_svg(filename, result.data, result.len)) {
            std::cout << "Saved " << filename << " (" << result.len << " bytes)" << std::endl;
        }
        ratex_free_svg_result(result);
        return true;
    }

    std::cerr << "Error: " << ratex_get_last_error() << std::endl;
    return false;
}

template <typename Func>
void run_timed_example(const char* name, Func&& func) {
    const auto start = std::chrono::steady_clock::now();
    func();
    const auto end = std::chrono::steady_clock::now();
    const auto elapsed_ms = std::chrono::duration_cast<std::chrono::milliseconds>(end - start).count();
    std::cout << "[Timing] " << name << ": " << elapsed_ms << " ms" << std::endl;
}

// Example 1: Simple formula with default options
void example_simple() {
    std::cout << "\n=== Example 1: Simple formula (default options) ===" << std::endl;

    // Simplest usage: just pass the LaTeX string
    render_pdf_to_file(
        "\\frac{1}{2} + \\sqrt{x}",  // LaTeX formula
        nullptr,                      // Default layout options
        nullptr,                      // Default PDF options
        "example1_simple.pdf"
    );
}

// Example 2: Display mode with custom color
void example_display_color() {
    std::cout << "\n=== Example 2: Display mode with custom color ===" << std::endl;

    // Set up custom color (dark blue)
    RatexColor blue = {0.1f, 0.3f, 0.8f, 1.0f};

    // Layout options for display mode with custom color
    RatexOptions layout_opts = {
        sizeof(RatexOptions),
        RATEX_DISPLAY_MODE_DISPLAY,  // Display mode (block style)
        &blue
    };

    render_pdf_to_file(
        "\\int_0^\\infty e^{-x^2} dx = \\frac{\\sqrt{\\pi}}{2}",
        &layout_opts,
        nullptr,  // Default PDF options
        "example2_display_color.pdf"
    );
}

// Example 3: Inline mode with custom PDF options
void example_inline_custom() {
    std::cout << "\n=== Example 3: Inline mode with custom PDF options ===" << std::endl;

    // Layout options for inline mode
    RatexOptions layout_opts = {
        sizeof(RatexOptions),
        RATEX_DISPLAY_MODE_INLINE,
        nullptr
    };

    // Custom PDF options: larger font, more padding
    RatexPdfOptions pdf_opts = {
        sizeof(RatexPdfOptions),
        64.0f,   // font_size: larger font
        30.0f,   // padding: more space around the formula
        2.5f,    // stroke_width: thicker lines
        0        // show_baseline: disabled
    };

    render_pdf_to_file(
        "\\sum_{i=1}^n x_i = \\bar{x}",
        &layout_opts,
        &pdf_opts,
        "example3_inline_custom.pdf"
    );
}

// Example 4: Chemistry notation (mhchem)
void example_chemistry() {
    std::cout << "\n=== Example 4: Chemistry notation ===" << std::endl;

    RatexPdfOptions pdf_opts = {
        sizeof(RatexPdfOptions),
        40.0f,
        10.0f,
        1.5f,
        0
    };
    pdf_opts.font_size = 36.0f;
    pdf_opts.padding = 15.0f;

    render_pdf_to_file(
        "\\ce{H2SO4 + 2NaOH -> Na2SO4 + 2H2O}",
        nullptr,   // Default layout options
        &pdf_opts,
        "example4_chemistry.pdf"
    );
}

// Example 5: Complex expression with custom styling
void example_complex() {
    std::cout << "\n=== Example 5: Complex expression ===" << std::endl;

    // Custom color: purple
    RatexColor purple = {0.5f, 0.0f, 0.5f, 1.0f};

    RatexOptions layout_opts = {
        sizeof(RatexOptions),
        RATEX_DISPLAY_MODE_DISPLAY,
        &purple
    };

    RatexPdfOptions pdf_opts = {
        sizeof(RatexPdfOptions),
        32.0f,    // Smaller font for complex formula
        20.0f,    // Standard padding
        1.2f,     // Slightly thinner strokes
        0         // show_baseline: disabled
    };

    render_pdf_to_file(
        "\\begin{pmatrix} a & b \\\\ c & d \\end{pmatrix}^{-1} = "
        "\\frac{1}{ad-bc} \\begin{pmatrix} d & -b \\\\ -c & a \\end{pmatrix}",
        &layout_opts,
        &pdf_opts,
        "example5_complex.pdf"
    );
}

// Example 6: Error handling demonstration
void example_error_handling() {
    std::cout << "\n=== Example 6: Error handling ===" << std::endl;

    // This LaTeX has an error (undefined command)
    RatexPdfResult result = ratex_render_to_pdf(
        "\\frac{1}{\\undefinedcommand}",
        nullptr,
        nullptr
    );

    if (result.error_code == 0) {
        std::cout << "Unexpected success!" << std::endl;
        ratex_free_pdf_result(result);
    } else {
        std::cout << "Expected error caught:" << std::endl;
        std::cout << "  Error code: " << result.error_code << std::endl;
        std::cout << "  Message: " << ratex_get_last_error() << std::endl;
    }
}

// Example 7: Multiple formulas in batch
void example_batch() {
    std::cout << "\n=== Example 7: Batch rendering ===" << std::endl;

    std::vector<std::string> formulas = {
        "x^2 + y^2 = z^2",
        "\\sin^2\\theta + \\cos^2\\theta = 1",
        "e^{i\\pi} + 1 = 0",
        "\\lim_{x \\to 0} \\frac{\\sin x}{x} = 1",
        "\\nabla \\cdot \\vec{E} = \\frac{\\rho}{\\varepsilon_0}"
    };

    RatexPdfOptions pdf_opts = {
        sizeof(RatexPdfOptions),
        40.0f,
        10.0f,
        1.5f,
        0
    };
    pdf_opts.font_size = 28.0f;

    for (size_t i = 0; i < formulas.size(); ++i) {
        RatexPdfResult result = ratex_render_to_pdf(
            formulas[i].c_str(),
            nullptr,
            &pdf_opts
        );

        if (result.error_code == 0) {
            std::string filename = "batch_" + std::to_string(i + 1) + ".pdf";
            if (save_pdf(filename.c_str(), result.data, result.len)) {
                std::cout << "Saved " << filename << " (" << result.len << " bytes)" << std::endl;
            }
            ratex_free_pdf_result(result);
        } else {
            std::cerr << "Failed to render formula " << (i + 1) << ": "
                      << ratex_get_last_error() << std::endl;
        }
    }
}

// Example 8: Baseline visualization (like \showbaseline in LaTeX)
void example_baseline() {
    std::cout << "\n=== Example 8: Baseline visualization ===" << std::endl;

    RatexPdfOptions pdf_opts = {
        sizeof(RatexPdfOptions),
        48.0f,    // font_size
        20.0f,    // padding
        1.5f,     // stroke_width
        1         // show_baseline = true
    };

    // Simple formula showing baseline
    render_pdf_to_file(
        "aa",      // Simple text to show baseline
        nullptr,
        &pdf_opts,
        "example8_baseline_aa.pdf"
    );

    // Fraction with baseline
    render_pdf_to_file(
        "\\frac{1}{2} + x",
        nullptr,
        &pdf_opts,
        "example8_baseline_fraction.pdf"
    );
}

// Example 9: Self-contained SVG export
void example_svg() {
    std::cout << "\n=== Example 9: SVG rendering ===" << std::endl;

    RatexSvgOptions svg_opts = {
        sizeof(RatexSvgOptions),
        40.0f,
        10.0f,
        1.5f
    };
    svg_opts.font_size = 40.0f;
    svg_opts.padding = 10.0f;

    render_svg_to_file(
        "\\frac{1}{2}+x",
        nullptr,
        &svg_opts,
        "example9_fraction.svg"
    );
}

// Example 10: Multilingual text in PDF and SVG
void example_multilingual() {
    std::cout << "\n=== Example 10: Multilingual text ===" << std::endl;

    const char* latex = u8"\\text{你好，世界！ 日本語テキスト 한국어 예시}";

    RatexPdfOptions pdf_opts = {
        sizeof(RatexPdfOptions),
        32.0f,
        16.0f,
        1.5f,
        0
    };

    RatexSvgOptions svg_opts = {
        sizeof(RatexSvgOptions),
        32.0f,
        16.0f,
        1.5f
    };

    render_pdf_to_file(
        latex,
        nullptr,
        &pdf_opts,
        "example10_multilingual.pdf"
    );

    render_svg_to_file(
        latex,
        nullptr,
        &svg_opts,
        "example10_multilingual.svg"
    );
}

// Example 11: Emoji text in PDF and SVG
void example_emoji() {
    std::cout << "\n=== Example 11: Emoji text ===" << std::endl;

    const char* latex = u8"\\text{😀🎉🚀💡✅❌}";

    RatexPdfOptions pdf_opts = {
        sizeof(RatexPdfOptions),
        36.0f,
        16.0f,
        1.5f,
        0
    };

    RatexSvgOptions svg_opts = {
        sizeof(RatexSvgOptions),
        36.0f,
        16.0f,
        1.5f
    };

    render_pdf_to_file(
        latex,
        nullptr,
        &pdf_opts,
        "example11_emoji.pdf"
    );

    render_svg_to_file(
        latex,
        nullptr,
        &svg_opts,
        "example11_emoji.svg"
    );
}

// Example 12: Direct Unicode Greek letters in PDF and SVG
void example_unicode_greek() {
    std::cout << "\n=== Example 12: Unicode Greek letters ===" << std::endl;

    const char* latex = u8"α + β = γ";

    RatexOptions layout_opts = {
        sizeof(RatexOptions),
        RATEX_DISPLAY_MODE_DISPLAY,
        nullptr
    };

    RatexPdfOptions pdf_opts = {
        sizeof(RatexPdfOptions),
        40.0f,
        18.0f,
        1.5f,
        0
    };

    RatexSvgOptions svg_opts = {
        sizeof(RatexSvgOptions),
        40.0f,
        18.0f,
        1.5f
    };

    render_pdf_to_file(
        latex,
        &layout_opts,
        &pdf_opts,
        "example12_unicode_greek.pdf"
    );

    render_svg_to_file(
        latex,
        &layout_opts,
        &svg_opts,
        "example12_unicode_greek.svg"
    );
}

// Example 13: Custom Unicode font configuration
void example_custom_unicode_font() {
    std::cout << "\n=== Example 13: Custom Unicode font ===" << std::endl;

    // Set a custom Unicode font (example paths, adjust for your system)
#ifdef __APPLE__
    const char* font_spec = "/System/Library/Fonts/PingFang.ttc#PingFang SC";
#elif _WIN32
    const char* font_spec = "C:\\Windows\\Fonts\\msyh.ttc#Microsoft YaHei";
#else
    const char* font_spec = "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc#Noto Sans CJK SC";
#endif

    if (ratex_set_unicode_font(font_spec)) {
        std::cout << "Successfully set Unicode font: " << font_spec << std::endl;

        const char* latex = u8"\\text{Hello World! 你好，世界！日本語テキスト 한국어 예시}";

        RatexPdfOptions pdf_opts = {
            sizeof(RatexPdfOptions),
            36.0f,
            16.0f,
            1.5f,
            0
        };

        render_pdf_to_file(
            latex,
            nullptr,
            &pdf_opts,
            "example13_custom_font.pdf"
        );

        // Clear the custom font to restore auto-discovery
        ratex_clear_unicode_font();
        std::cout << "Unicode font cache cleared" << std::endl;
    } else {
        std::cout << "Failed to set Unicode font (font may not exist on this system)" << std::endl;
    }
}

int main() {
    std::cout << "RaTeX C++ Rendering Examples" << std::endl;
    std::cout << "============================" << std::endl;

    run_timed_example("example_simple", example_simple);
    run_timed_example("example_display_color", example_display_color);
    run_timed_example("example_inline_custom", example_inline_custom);
    run_timed_example("example_chemistry", example_chemistry);
    run_timed_example("example_complex", example_complex);
    run_timed_example("example_error_handling", example_error_handling);
    run_timed_example("example_batch", example_batch);
    run_timed_example("example_baseline", example_baseline);
    run_timed_example("example_svg", example_svg);
    run_timed_example("example_multilingual", example_multilingual);
    run_timed_example("example_emoji", example_emoji);
    run_timed_example("example_unicode_greek", example_unicode_greek);
    run_timed_example("example_custom_unicode_font", example_custom_unicode_font);

    std::cout << "\nAll examples completed!" << std::endl;
    return 0;
}
