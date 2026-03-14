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
# Read version from the single source of truth: build_constants.toml
_MAJOR=$(grep 'major' build_constants.toml | head -1 | sed 's/[^0-9]//g')
_MINOR=$(grep 'minor' build_constants.toml | head -1 | sed 's/[^0-9]//g')
_PATCH=$(grep 'patch' build_constants.toml | head -1 | sed 's/[^0-9]//g')
VERSION="${_MAJOR}.${_MINOR}.${_PATCH}"
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

# Copy bundled plugins (Lua particle/shader definitions)
if [[ -d "assets/plugins" ]]; then
    echo "🔌 Bundling plugins..."
    mkdir -p "$RESOURCES_DIR/plugins"
    cp assets/plugins/*.lua "$RESOURCES_DIR/plugins/"
fi

# Copy shader files (.wgsl)
if [[ -d "crates/ridgeback-gpu/shaders" ]]; then
    echo "🎨 Bundling shaders..."
    mkdir -p "$RESOURCES_DIR/shaders"
    cp crates/ridgeback-gpu/shaders/*.wgsl "$RESOURCES_DIR/shaders/"
fi

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

  <!-- Appear in the Utilities category in Launchpad / App Store -->
  <key>LSApplicationCategoryType</key>
  <string>public.app-category.utilities</string>

  <!-- Shell-script document types — enables "Open With > Ridgeback" in Finder -->
  <key>CFBundleDocumentTypes</key>
  <array>
    <dict>
      <key>CFBundleTypeName</key>
      <string>Shell Script</string>
      <key>CFBundleTypeRole</key>
      <string>Shell</string>
      <key>LSHandlerRank</key>
      <string>Alternate</string>
      <key>LSItemContentTypes</key>
      <array>
        <string>public.shell-script</string>
        <string>public.bash-script</string>
        <string>public.zsh-script</string>
        <string>com.apple.terminal.shell-script</string>
      </array>
      <key>CFBundleTypeExtensions</key>
      <array>
        <string>sh</string>
        <string>bash</string>
        <string>zsh</string>
        <string>fish</string>
        <string>command</string>
        <string>ps1</string>
      </array>
    </dict>
    <dict>
      <key>CFBundleTypeName</key>
      <string>Folder</string>
      <key>CFBundleTypeRole</key>
      <string>Shell</string>
      <key>LSHandlerRank</key>
      <string>Alternate</string>
      <key>LSItemContentTypes</key>
      <array>
        <string>public.folder</string>
      </array>
    </dict>
  </array>

  <!-- ridgeback:// URL scheme for programmatic terminal launching -->
  <key>CFBundleURLTypes</key>
  <array>
    <dict>
      <key>CFBundleURLName</key>
      <string>Ridgeback Terminal URL</string>
      <key>CFBundleURLSchemes</key>
      <array>
        <string>ridgeback</string>
      </array>
    </dict>
  </array>
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
echo "   To register context menus and Quick Actions after installing:"
echo "     /Applications/${APP_NAME}.app/Contents/MacOS/ridgeback --register"
echo ""
echo "   Contents:"
ls -R "$APP_DIR"


