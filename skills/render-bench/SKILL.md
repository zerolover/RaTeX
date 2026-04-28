---
name: render-bench
description: >-
  Run RaTeX rendering performance benchmarks (100-formula PNG/SVG/PDF, cold-vs-hot
  font cache timing, phase breakdown). Use when profiling render performance or
  comparing before/after optimization changes.
---

# Render performance benchmarks

## Cross-tool layout (Cursor / Claude Code / Codex)

- **Canonical copy**: repository root `skills/render-bench/SKILL.md` (this file).
- **Tool entry points** (symlinks to this file in-repo):
  - Cursor: `.cursor/skills/render-bench/SKILL.md`
  - Claude Code: `.claude/skills/render-bench/SKILL.md`
  - Codex: `.agents/skills/render-bench/SKILL.md`

## When to use

- After changes to rendering paths (especially `renderer.rs`, `fonts.rs`, `glyphs.rs`, outline caching, font loading).
- Before committing optimizations — run cold first so cache impact is visible.
- Comparing before/after for a specific renderer (PNG vs SVG vs PDF).

## Prerequisites

- Rust toolchain with `--release`.
- KaTeX TTFs in `fonts/` (committed in-repo; 20 `.ttf` files).
- (Optional) CJK/emoji system fonts for the full CJK/emoji categories.

## Commands

### 1. Full 100-formula benchmark

```bash
cargo test -p ratex-render --test bench_render --release -- --ignored --nocapture
```

Renders 100 formulas across 6 categories (math, complex, matrix, cjk, emoji, chem) in PNG, SVG, and PDF. Prints per-category averages and overall throughput (formulas/sec).

Warmup: 1 iteration per formula. Measurement: 3 iterations, averaged.

### 2. Font cache cold vs hot timing

```bash
cargo test -p ratex-render --test font_cache_timing -- --ignored --nocapture
```

Measures 3 formulas: first pass (cold — disk I/O for font loading), second pass (hot — `OnceLock` cache hits). Reports speedup ratio.

### 3. Render phase breakdown

```bash
cargo test -p ratex-render --test phase_breakdown --release -- --ignored --nocapture
```

Per-formula breakdown: parse+layout vs render phase timing. 6 formulas including CJK and emoji cases. Warmup: 3 iterations, Measurement: 10 iterations, averaged.

### 4. Compare before/after (quick diff)

Useful when optimizing a specific path:

```bash
# Before (on main)
cargo test -p ratex-render --test phase_breakdown --release -- --ignored --nocapture 2>&1 | tee /tmp/before.txt

# After (on branch)
cargo test -p ratex-render --test phase_breakdown --release -- --ignored --nocapture 2>&1 | tee /tmp/after.txt

# Diff the tables
diff /tmp/before.txt /tmp/after.txt
```

## Quick checklist

1. Run `cargo test -p ratex-render --test bench_render --release -- --ignored --nocapture` for the full picture.
2. For quick iteration, use `phase_breakdown` (faster, 6 formulas).
3. For font-loading specific changes, use `font_cache_timing`.
4. Always use `--release` — debug builds are 10-50× slower and meaningless for perf comparison.
