#!/bin/sh
# build-icon.sh — generate assets/aural-icon.icns from assets/aural-icon.png
# using macOS `iconutil` + `sips` (no extra deps). Run once when the source PNG
# changes; the .icns is committed so package-app.sh just copies it into the app
# bundle for the Input Monitoring / Finder icon.
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
SRC="$ROOT/assets/aural-icon.png"
OUT="$ROOT/assets/aural-icon.icns"
ICONSET="$ROOT/target/aural-icon.iconset"

[ -f "$SRC" ] || {
    echo "error: missing $SRC" >&2
    exit 1
}

rm -rf "$ICONSET"
mkdir -p "$ICONSET"

sips -z 16 16 "$SRC" --out "$ICONSET/icon_16x16.png" >/dev/null
sips -z 32 32 "$SRC" --out "$ICONSET/icon_16x16@2x.png" >/dev/null
sips -z 32 32 "$SRC" --out "$ICONSET/icon_32x32.png" >/dev/null
sips -z 64 64 "$SRC" --out "$ICONSET/icon_32x32@2x.png" >/dev/null
sips -z 128 128 "$SRC" --out "$ICONSET/icon_128x128.png" >/dev/null
sips -z 256 256 "$SRC" --out "$ICONSET/icon_128x128@2x.png" >/dev/null
sips -z 256 256 "$SRC" --out "$ICONSET/icon_256x256.png" >/dev/null
sips -z 512 512 "$SRC" --out "$ICONSET/icon_256x256@2x.png" >/dev/null
sips -z 512 512 "$SRC" --out "$ICONSET/icon_512x512.png" >/dev/null
cp "$SRC" "$ICONSET/icon_512x512@2x.png"

iconutil -c icns "$ICONSET" -o "$OUT"
echo "generated $OUT"
