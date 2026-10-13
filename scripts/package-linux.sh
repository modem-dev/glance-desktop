#!/bin/sh
# Produce a Linux archive and a checksummed Arch packaging recipe.
set -eu
cd "$(dirname "$0")/.."
[ "$(uname -s)" = Linux ] || { echo 'Run this script on Linux.' >&2; exit 1; }
MODE="${1:-release}"
case "$MODE" in release) cargo build --release --locked;; debug) cargo build --locked;; *) echo 'Expected release or debug' >&2; exit 1;; esac
VERSION="$(sed -n '/^\[package\]/,/^\[/s/^version = "\([^"]*\)".*/\1/p' Cargo.toml)"
ARCH="$(uname -m)"
NAME="glance-$VERSION-linux-$ARCH"
DIST="$(pwd)/target/dist"
mkdir -p "$DIST"
STAGE="$(mktemp -d "$(pwd)/target/.glance-linux.XXXXXX")"
trap 'rm -rf "$STAGE"' EXIT
trap 'exit 1' HUP INT TERM
ROOT="$STAGE/$NAME"
mkdir -p "$ROOT/bin" "$ROOT/share/applications" "$ROOT/share/icons/hicolor/scalable/apps" "$ROOT/share/licenses/glance"
# Keep the Cargo executable usable with MCP/video after packaging too.
install -m755 native/linux/glance-video-* "${CARGO_TARGET_DIR:-target}/$MODE/"
install -m755 "${CARGO_TARGET_DIR:-target}/$MODE/glance" "$ROOT/bin/glance"
install -m755 native/linux/glance-video-* "$ROOT/bin/"
install -m644 packaging/linux/glance.desktop "$ROOT/share/applications/"
install -m644 assets/icons/glance.svg "$ROOT/share/icons/hicolor/scalable/apps/"
install -m644 LICENSE THIRD_PARTY_NOTICES.md "$ROOT/share/licenses/glance/"
install -m644 assets/gpui/LICENSE-APACHE "$ROOT/share/licenses/glance/GPUI-LICENSE"
install -m644 assets/lucide/LICENSE "$ROOT/share/licenses/glance/Lucide-LICENSE"
install -m644 assets/fonts/LICENSE "$ROOT/share/licenses/glance/Roboto-LICENSE"
tar -czf "$DIST/$NAME.tar.gz" -C "$STAGE" "$NAME"
HASH="$(sha256sum "$DIST/$NAME.tar.gz" | cut -d ' ' -f1)"
sed -e "s/@VERSION@/$VERSION/g" -e "s/@ARCH@/$ARCH/g" \
    -e "s/@ARCHIVE@/$NAME.tar.gz/g" -e "s/@DIRECTORY@/$NAME/g" -e "s/@SHA256@/$HASH/g" \
    packaging/linux/PKGBUILD.in > "$DIST/PKGBUILD"
(cd "$DIST" && sha256sum "$NAME.tar.gz" > "$NAME.tar.gz.sha256")
printf 'Built %s/%s.tar.gz and PKGBUILD\n' "$DIST" "$NAME"
