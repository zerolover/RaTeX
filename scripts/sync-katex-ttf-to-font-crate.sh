#!/usr/bin/env bash
# Copy KaTeX *.ttf into crates/ratex-katex-fonts/fonts/ for publishing / bundled-font builds.
# Source: first existing of repo fonts/, or tools/lexer_compare/node_modules/katex/dist/fonts/.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC=""
if [[ -d "$ROOT/fonts" ]] && compgen -G "$ROOT/fonts/KaTeX_*.ttf" > /dev/null; then
  SRC="$ROOT/fonts"
elif [[ -d "$ROOT/tools/lexer_compare/node_modules/katex/dist/fonts" ]]; then
  SRC="$ROOT/tools/lexer_compare/node_modules/katex/dist/fonts"
else
  echo "No KaTeX TTF source found (expected $ROOT/fonts or katex npm under tools/lexer_compare)." >&2
  exit 1
fi
DEST="$ROOT/crates/ratex-katex-fonts/fonts"
mkdir -p "$DEST"
cp "$SRC"/KaTeX_*.ttf "$DEST/"
echo "Synced $(ls -1 "$DEST"/*.ttf 2>/dev/null | wc -l | tr -d ' ') files from $SRC -> $DEST"
