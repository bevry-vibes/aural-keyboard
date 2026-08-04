#!/bin/sh
# package-app.sh — wrap the release binary as Aural.app so macOS attributes
# TCC permissions (Input Monitoring) to "Aural" instead of the terminal that
# launched it (TCC grants go to the *responsible process*: for a naked binary
# run from a terminal that is the terminal; for a binary inside an .app bundle
# it is the bundle — regardless of who launched it). No install, login item,
# or launchctl involved: run the bundled binary from any terminal:
#
#   Aural.app/Contents/MacOS/aural run
#
# Identity: ad-hoc signed by default (free, automatic on arm64); each rebuild
# then re-prompts TCC. For a stable local identity, create a self-signed
# code-signing certificate (Keychain Access → Certificate Assistant → Create
# a Certificate → Code Signing) and pass:
#
#   AURAL_SIGN_IDENTITY="MyCertName" ./scripts/package-app.sh
#
# Env overrides: AURAL_APP (output path), AURAL_SIGN_IDENTITY (signing identity).
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
TARGET_DIR=${CARGO_TARGET_DIR:-"$ROOT/target"}
BIN=${1:-"$TARGET_DIR/release/aural"}
APP=${AURAL_APP:-"$TARGET_DIR/release/Aural.app"}
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)

[ -x "$BIN" ] || {
    echo "error: binary not found: $BIN (run: cargo build --release)" >&2
    exit 1
}

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
cp "$BIN" "$APP/Contents/MacOS/aural"

cat > "$APP/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleDisplayName</key>
    <string>Aural</string>
    <key>CFBundleExecutable</key>
    <string>aural</string>
    <key>CFBundleIdentifier</key>
    <string>com.bevry.aural</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>Aural</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>$VERSION</string>
    <key>CFBundleVersion</key>
    <string>$VERSION</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>LSUIElement</key>
    <true/>
</dict>
</plist>
EOF

IDENTITY=${AURAL_SIGN_IDENTITY:--} # "-" = ad-hoc
codesign --force --sign "$IDENTITY" "$APP"

echo "packaged $APP (identity: $IDENTITY, version $VERSION)"
echo "run from any terminal:  \"$APP/Contents/MacOS/aural\" run"
echo "the Input Monitoring prompt will name 'Aural' instead of your terminal."
