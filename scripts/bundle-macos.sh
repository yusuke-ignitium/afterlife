#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

RELAUNCH=1
if [[ "${AFTERLIFE_NO_LAUNCH:-0}" == "1" ]]; then
    RELAUNCH=0
fi

for arg in "$@"; do
    case "$arg" in
        --no-launch)
            RELAUNCH=0
            ;;
        -h|--help)
            echo "Usage: $(basename "$0") [--no-launch]"
            exit 0
            ;;
        *)
            echo "error: unknown option '$arg' (usage: $(basename "$0") [--no-launch])" >&2
            exit 2
            ;;
    esac
done

export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT_DIR/target}"

echo "==> Building release Afterlife..."
cargo build --release -p afterlife-ui

APP_DIR="$ROOT_DIR/dist/Afterlife.app"
mkdir -p "$APP_DIR/Contents/MacOS" "$APP_DIR/Contents/Resources"

if [[ "$RELAUNCH" -eq 1 ]] && pgrep -x Afterlife >/dev/null 2>&1; then
    echo "==> Stopping running Afterlife..."
    pkill -x Afterlife >/dev/null 2>&1 || true
    for _ in $(seq 1 30); do
        if ! pgrep -x Afterlife >/dev/null 2>&1; then
            break
        fi
        sleep 0.1
    done
fi

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
    <key>NSAppleEventsUsageDescription</key>
    <string>Afterlife reads the currently playing track metadata and local artwork from Apple Music to display album art in the backdrop.</string>
</dict>
</plist>
PLIST

codesign --force --deep --sign - "$APP_DIR"
echo "==> Packaged: $APP_DIR"

if [[ "$RELAUNCH" -eq 1 ]]; then
    echo "==> Launching: $APP_DIR"
    launched=0
    for _ in 1 2 3; do
        if open -n -a "$APP_DIR"; then
            launched=1
            break
        fi
        sleep 0.3
    done
    if [[ "$launched" -ne 1 ]]; then
        echo "error: failed to launch $APP_DIR" >&2
        exit 1
    fi
fi
