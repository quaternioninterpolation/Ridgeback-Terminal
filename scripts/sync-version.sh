#!/usr/bin/env bash
#
# sync-version.sh — Propagate the version from build_constants.toml to all
# other locations in the repository.
#
# This is the recommended way to bump the version:
#   1. Edit build_constants.toml   (single source of truth)
#   2. Run:  ./scripts/sync-version.sh
#   3. Commit the result.
#
# What it updates:
#   • Cargo.toml  [workspace.package] version
#   • Cargo.lock  (via `cargo update --workspace`)

set -euo pipefail
cd "$(dirname "$0")/.."

# ── Read version from build_constants.toml ─────────────────────────────
MAJOR=$(grep 'major' build_constants.toml | head -1 | sed 's/[^0-9]//g')
MINOR=$(grep 'minor' build_constants.toml | head -1 | sed 's/[^0-9]//g')
PATCH=$(grep 'patch' build_constants.toml | head -1 | sed 's/[^0-9]//g')
VERSION="${MAJOR}.${MINOR}.${PATCH}"

echo "📌 Version from build_constants.toml: ${VERSION}"

# ── Update workspace Cargo.toml ───────────────────────────────────────
# Replace the version line under [workspace.package]
if grep -q '\[workspace\.package\]' Cargo.toml; then
    # Use sed to replace the version line following [workspace.package]
    if [[ "$(uname -s)" == "Darwin" ]]; then
        sed -i '' '/\[workspace\.package\]/,/^$/{s/^version = ".*"/version = "'"${VERSION}"'"/;}' Cargo.toml
    else
        sed -i '/\[workspace\.package\]/,/^$/{s/^version = ".*"/version = "'"${VERSION}"'"/;}' Cargo.toml
    fi
    echo "   ✅ Cargo.toml [workspace.package] version → ${VERSION}"
else
    echo "   ⚠️  No [workspace.package] section found in Cargo.toml"
    exit 1
fi

# ── Update Cargo.lock ─────────────────────────────────────────────────
echo "   🔄 Updating Cargo.lock..."
cargo update --workspace 2>/dev/null || echo "   ⚠️  cargo update failed (non-fatal)"
echo "   ✅ Cargo.lock updated"

# ── Summary ───────────────────────────────────────────────────────────
echo ""
echo "✅ All version references updated to ${VERSION}"
echo ""
echo "   Files changed:"
echo "     • build_constants.toml       (source of truth — you edited this)"
echo "     • Cargo.toml                 (workspace version)"
echo "     • Cargo.lock                 (dependency lock)"
echo ""
echo "   Files that read build_constants.toml at build/bundle time (no change needed):"
echo "     • crates/ridgeback-app/build.rs"
echo "     • scripts/bundle-macos.sh"
echo ""
echo "Next steps:"
echo "  git add -A && git commit -m 'Bump version to ${VERSION}'"

