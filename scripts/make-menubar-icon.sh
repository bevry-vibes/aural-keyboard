#!/bin/sh
# make-menubar-icon.sh — derive the small menubar status icon from the app icon.
# sips (macOS, built-in) downscales assets/aural-icon.png to a compact PNG that
# src/menubar.rs embeds via include_bytes! and hands to NSImage/NSData at
# runtime (Foundation decodes it, so no PNG parser is needed). Run once when the
# source icon changes; the small PNG is committed so normal builds need no sips.
#
# The menubar icon is the *colored* app icon, cropped to its content bounds and
# scaled up to fill the full status-bar height (matching the size of other
# status items). ImageMagick (`magick`) does the crop.
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
SRC="$ROOT/assets/aural-icon.png"
OUT="$ROOT/assets/aural-menubar.png"
TMP="$ROOT/target/aural-menubar-src.png"

[ -f "$SRC" ] || {
    echo "error: missing $SRC" >&2
    exit 1
}

mkdir -p "$ROOT/target"
# Trim the opaque content FIRST (on the full-res source), then downscale to the
# final size. Downscaling is sharp; upscaling a cropped glyph is what blurred it.
# 44px = 2x of the ~22pt status bar on a retina display.
magick "$SRC" -trim +repage -resize 44x44\! -background none -gravity center -extent 44x44 "$TMP"
# Force true RGBA (color type 6): magick's default may emit GrayscaleAlpha
# (2 bytes/pixel), which the runtime decoder can't hand to `Icon::from_rgba`.
magick "$TMP" -define png:color-type=6 "$OUT"
rm -f "$TMP"
echo "generated $OUT"
ls -l "$OUT"