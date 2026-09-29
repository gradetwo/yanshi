#!/usr/bin/env bash
# Render the Yanshi brand assets (assets/brand/svg/*.svg) into the PNG sizes the
# product actually ships: desktop app icon, favicons, and README logos.
#
# Usage: assets/brand/render.sh
# Requires: rsvg-convert (librsvg), optional: magick/convert for the .ico

set -euo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SVG="$DIR/svg"
OUT="$DIR/png"
mkdir -p "$OUT"

render() { # <source> <width> <output>
  rsvg-convert -w "$2" "$SVG/$1" -o "$OUT/$3"
  printf '  %-34s %s\n' "$3" "$(file -b "$OUT/$3" | cut -d, -f1-2)"
}

echo "brand assets -> $OUT"
# Desktop app icon (freedesktop hicolor 256×256) + light/dark variants.
render icon-light.svg 256 yanshi-icon-256.png
render icon-dark.svg 256 yanshi-icon-dark-256.png
render icon-light.svg 512 yanshi-icon-512.png
# Favicons (also served by the HTTP layer at /favicon.svg + /favicon.png).
render icon-light.svg 32 favicon-32.png
render icon-light.svg 180 favicon-180.png
render favicon.svg 16 favicon-16.png
# README / docs logos (SVG is preferred in Markdown; PNG is a fallback for
# renderers that do not scale SVG).
render logo-horizontal.svg 560 logo-horizontal-560.png
render logo-horizontal-cn.svg 420 logo-horizontal-cn-420.png
render logo-primary.svg 260 logo-primary-260.png
render logo-vertical.svg 260 logo-vertical-260.png
render logo-ultra-mini.svg 260 logo-ultra-mini-260.png

# Multi-resolution .ico for browsers that still request /favicon.ico.
if command -v magick >/dev/null 2>&1; then
  magick "$OUT/favicon-16.png" "$OUT/favicon-32.png" "$OUT/favicon-180.png" "$OUT/favicon.ico"
  printf '  %-34s %s\n' "favicon.ico" "$(file -b "$OUT/favicon.ico" | cut -d, -f1-2)"
fi
