#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT_DIR/target}"

echo "==> Building release Afterlife..."
cargo build --release -p afterlife-ui

APP_DIR="$ROOT_DIR/dist/Afterlife.app"
mkdir -p "$APP_DIR/Contents/MacOS" "$APP_DIR/Contents/Resources"

cp "$CARGO_TARGET_DIR/release/afterlife-desktop" "$APP_DIR/Contents/MacOS/Afterlife"
chmod +x "$APP_DIR/Contents/MacOS/Afterlife"
cp "$ROOT_DIR/crates/afterlife-ui/assets/Afterlife.icns" "$APP_DIR/Contents/Resources/Afterlife.icns"

cat > "$APP_DIR/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>Afterlife</string>
    <key>CFBundleDisplayName</key>
    <string>Afterlife</string>
    <key>CFBundleIdentifier</key>
    <string>dev.afterlife.app</string>
    <key>CFBundleVersion</key>
    <string>0.1.0</string>
    <key>CFBundleShortVersionString</key>
    <string>0.1.0</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleExecutable</key>
    <string>Afterlife</string>
    <key>CFBundleIconFile</key>
    <string>Afterlife</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
PLIST

codesign --force --deep --sign - "$APP_DIR"
echo "==> Packaged: $APP_DIR"
