#!/usr/bin/env bash
#
# bundle-macos.sh — Build Ridgeback as a macOS .app bundle.
#
# Usage:
#   ./scripts/bundle-macos.sh [--release]
#
# This script is macOS-only and is a no-op on other platforms.
# It does NOT require cargo-bundle — it builds the .app structure directly.

set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "⚠️  This script is macOS-only. Skipping."
    exit 0
fi

# ── Parse arguments ────────────────────────────────────────────────────
PROFILE="debug"
TARGET=""
SKIP_BUILD=false
CARGO_FLAGS=()
for arg in "$@"; do
    case "$arg" in
        --release)
            PROFILE="release"
            CARGO_FLAGS+=("--release")
            ;;
        --target=*)
            TARGET="${arg#--target=}"
            CARGO_FLAGS+=("--target" "$TARGET")
            ;;
        --skip-build)
            SKIP_BUILD=true
            ;;
    esac
done

# ── Paths ──────────────────────────────────────────────────────────────
cd "$(dirname "$0")/.."

APP_NAME="Ridgeback"
APP_DIR="target/${PROFILE}/${APP_NAME}.app"
CONTENTS_DIR="${APP_DIR}/Contents"
MACOS_DIR="${CONTENTS_DIR}/MacOS"
RESOURCES_DIR="${CONTENTS_DIR}/Resources"
SRC_PNG="assets/images/icon.png"
ICNS_OUT="assets/macos/icon.icns"

BUNDLE_ID="com.ridgeback.terminal"
VERSION="0.1.0"
MIN_MACOS="10.15"
COPYRIGHT="Copyright © 2026 Ridgeback contributors"

echo "🔨 Building Ridgeback .app bundle (${PROFILE})..."

# ── 1. Compile the binary (skip if already built, e.g. in CI) ─────────
if [[ "$SKIP_BUILD" == false ]]; then
    cargo build ${CARGO_FLAGS[@]+"${CARGO_FLAGS[@]}"} --bin ridgeback -p ridgeback-app
fi

# Locate the binary — with --target it's under target/<target>/<profile>/
if [[ -n "$TARGET" ]]; then
    BINARY="target/${TARGET}/${PROFILE}/ridgeback"
else
    BINARY="target/${PROFILE}/ridgeback"
fi

if [[ ! -f "$BINARY" ]]; then
    echo "❌ Binary not found at ${BINARY}"
    exit 1
fi

# ── 2. Generate .icns from source PNG ──────────────────────────────────
if [[ "$SRC_PNG" -nt "$ICNS_OUT" ]] || [[ ! -f "$ICNS_OUT" ]]; then
    echo "🎨 Generating .icns from ${SRC_PNG}..."
    mkdir -p assets/macos
    ICONSET_DIR=$(mktemp -d)/Ridgeback.iconset
    mkdir -p "$ICONSET_DIR"
    for SIZE in 16 32 128 256 512; do
        sips -z $SIZE $SIZE "$SRC_PNG" --out "$ICONSET_DIR/icon_${SIZE}x${SIZE}.png" > /dev/null 2>&1
        DOUBLE=$((SIZE * 2))
        sips -z $DOUBLE $DOUBLE "$SRC_PNG" --out "$ICONSET_DIR/icon_${SIZE}x${SIZE}@2x.png" > /dev/null 2>&1
    done
    iconutil -c icns "$ICONSET_DIR" -o "$ICNS_OUT"
    rm -rf "$ICONSET_DIR"
    echo "   ✅ ${ICNS_OUT}"
fi

# ── 3. Assemble the .app bundle ───────────────────────────────────────
echo "📦 Assembling ${APP_NAME}.app..."
rm -rf "$APP_DIR"
mkdir -p "$MACOS_DIR" "$RESOURCES_DIR"

# Copy binary
cp "$BINARY" "$MACOS_DIR/ridgeback"
chmod +x "$MACOS_DIR/ridgeback"

# Copy icon
cp "$ICNS_OUT" "$RESOURCES_DIR/icon.icns"

# ── 4. Write Info.plist ────────────────────────────────────────────────
cat > "${CONTENTS_DIR}/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleDisplayName</key>
  <string>${APP_NAME}</string>
  <key>CFBundleExecutable</key>
  <string>ridgeback</string>
  <key>CFBundleIconFile</key>
  <string>icon</string>
  <key>CFBundleIdentifier</key>
  <string>${BUNDLE_ID}</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>${APP_NAME}</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>${VERSION}</string>
  <key>CFBundleVersion</key>
  <string>${VERSION}</string>
  <key>LSMinimumSystemVersion</key>
  <string>${MIN_MACOS}</string>
  <key>NSHighResolutionCapable</key>
  <true/>
  <key>NSHumanReadableCopyright</key>
  <string>${COPYRIGHT}</string>
  <key>CFBundleDocumentTypes</key>
  <array/>
</dict>
</plist>
PLIST

# ── 5. Done ────────────────────────────────────────────────────────────
echo ""
echo "✅ ${APP_NAME}.app created at: ${APP_DIR}"
echo ""
echo "   To install, copy to /Applications:"
echo "     cp -r \"${APP_DIR}\" /Applications/"
echo ""
echo "   Contents:"
ls -R "$APP_DIR"


