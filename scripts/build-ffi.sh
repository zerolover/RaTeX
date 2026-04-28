#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIST_DIR="$REPO_ROOT/dist/ratex-ffi"
LIB_SOURCE_DIR="$REPO_ROOT/target/release"
INCLUDE_DIR="$DIST_DIR/inc/ratex"
LIB_DIR="$DIST_DIR/libs"

if ! command -v cargo >/dev/null 2>&1; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    . "$HOME/.cargo/env"
fi

mkdir -p "$INCLUDE_DIR" "$LIB_DIR"

cargo build --manifest-path "$REPO_ROOT/Cargo.toml" -p ratex-ffi --release

cp "$REPO_ROOT/include/ratex_base.h" "$INCLUDE_DIR/ratex_base.h"
cp "$REPO_ROOT/include/ratex_svg.h" "$INCLUDE_DIR/ratex_svg.h"
cp "$REPO_ROOT/include/ratex_pdf.h" "$INCLUDE_DIR/ratex_pdf.h"
cp "$REPO_ROOT/crates/ratex-ffi/include/ratex.h" "$INCLUDE_DIR/ratex.h"
if [ "$(uname -s)" = "Darwin" ]; then
    cp "$LIB_SOURCE_DIR/libratex_ffi.dylib" "$LIB_DIR/"
    install_name_tool -id "@rpath/libratex_ffi.dylib" "$LIB_DIR/libratex_ffi.dylib"
else
    cp "$LIB_SOURCE_DIR/libratex_ffi.so" "$LIB_DIR/"
fi

find "$DIST_DIR" -type f | sort
