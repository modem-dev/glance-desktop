#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
MODE="${1:-release}"
export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-12.0}"
VERSION="$(sed -n '/^\[package\]/,/^\[/s/^version = "\([^"]*\)".*/\1/p' Cargo.toml)"
if [ "$MODE" = "release" ]; then cargo build --release --locked; else cargo build --locked; fi
APP_DEST="${GLANCE_BUNDLE_DEST:-$(pwd)/target/Glance.app}"
mkdir -p "$(pwd)/target" "$(dirname "$APP_DEST")"
BUILD_DIR="$(mktemp -d "$(pwd)/target/.glance-bundle.XXXXXX")"
APP="$BUILD_DIR/Glance.app"
cleanup() {
    if [ ! -e "$APP_DEST" ] && [ -e "$BUILD_DIR/previous.app" ]; then
        mv "$BUILD_DIR/previous.app" "$APP_DEST"
    fi
    rm -rf "$BUILD_DIR"
}
trap cleanup EXIT
trap 'exit 1' HUP INT TERM
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cargo run --release --locked --example icon -- target/Glance.iconset
# A new resource name prevents macOS from reusing an earlier design's icon cache.
ICON_HASH="$(shasum -a 256 target/Glance.iconset/icon_512x512@2x.png | cut -c 1-12)"
ICON_NAME="Glance-$ICON_HASH.icns"
iconutil -c icns target/Glance.iconset -o "$APP/Contents/Resources/$ICON_NAME"
/usr/bin/swiftc -target "$(uname -m)-apple-macosx12.0" -O native/video_encoder.swift -o target/glance-video-encoder
/usr/bin/swiftc -target "$(uname -m)-apple-macosx12.0" -O native/video_frame.swift -o target/glance-video-frame
/usr/bin/swiftc -target "$(uname -m)-apple-macosx12.0" -O native/ocr.swift -o target/glance-ocr
cp target/glance-ocr "$APP/Contents/MacOS/glance-ocr"
cp target/glance-video-frame "$APP/Contents/MacOS/glance-video-frame"
cp target/glance-video-encoder "$APP/Contents/MacOS/glance-video-encoder"
cp assets/gpui/LICENSE-APACHE "$APP/Contents/Resources/GPUI-LICENSE"
cp assets/lucide/LICENSE "$APP/Contents/Resources/Lucide-LICENSE"
cp assets/fonts/LICENSE "$APP/Contents/Resources/Roboto-LICENSE"
cp LICENSE "$APP/Contents/Resources/LICENSE"
cp THIRD_PARTY_NOTICES.md "$APP/Contents/Resources/THIRD_PARTY_NOTICES.md"
cp "${CARGO_TARGET_DIR:-target}/$MODE/glance" "$APP/Contents/MacOS/Glance"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>Glance</string>
<key>CFBundleIdentifier</key><string>sh.glance.desktop</string>
<key>CFBundleIconFile</key><string>Glance.icns</string>
<key>CFBundleName</key><string>Glance</string>
<key>CFBundleDisplayName</key><string>Glance</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>$VERSION</string>
<key>CFBundleVersion</key><string>$VERSION</string>
<key>LSMinimumSystemVersion</key><string>12.0</string>
<key>NSHighResolutionCapable</key><true/>
<key>NSScreenCaptureUsageDescription</key><string>Glance captures your selected screen area for annotation.</string>
</dict></plist>
PLIST
/usr/libexec/PlistBuddy -c "Set :CFBundleIconFile $ICON_NAME" "$APP/Contents/Info.plist"
./scripts/sign-app.sh "$APP"
# Do not replace the installed bundle until every build/signing step succeeds.
if [ -e "$APP_DEST" ]; then mv "$APP_DEST" "$BUILD_DIR/previous.app"; fi
mv "$APP" "$APP_DEST"
printf 'Built %s\n' "$APP_DEST"
