#!/bin/sh
# Rebuild native icon sizes from the approved transparent PNG on macOS.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT HUP INT TERM
iconset="$work/DiffExcel.iconset"
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
  double=$((size * 2))
  sips -z "$size" "$size" assets/diff-excel.png --out "$iconset/icon_${size}x${size}.png" >/dev/null
  sips -z "$double" "$double" assets/diff-excel.png --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
cp "$iconset/icon_256x256.png" assets/icon-256.png
iconutil -c icns "$iconset" -o assets/DiffExcel.icns
