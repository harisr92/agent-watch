#!/usr/bin/env bash
# Regenerates macOS/AgentWatch/Resources/AppIcon.icns from assets/icon/AppIcon.svg.
# Requires inkscape (brew install --cask inkscape) and iconutil (ships with macOS).
set -euo pipefail

DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )/.." && pwd )"
SVG="$DIR/assets/icon/AppIcon.svg"
OUT="$DIR/macOS/AgentWatch/Resources/AppIcon.icns"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
SET="$TMP/AppIcon.iconset"
mkdir -p "$SET"

render() { inkscape "$SVG" -w "$1" -h "$1" -o "$2" >/dev/null 2>&1; }

for size in 16 32 128 256 512; do
    render "$size" "$SET/icon_${size}x${size}.png"
    render "$((size * 2))" "$SET/icon_${size}x${size}@2x.png"
done

iconutil -c icns "$SET" -o "$OUT"
cp "$SET/icon_256x256.png" "$DIR/assets/icon/icon-256.png"
echo "✅ Wrote $OUT"
