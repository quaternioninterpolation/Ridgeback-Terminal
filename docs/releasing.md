# Releasing a New Version

This document describes how to bump the version of Ridgeback and publish a new
GitHub release.

---

## Version Architecture

Ridgeback uses a **single source of truth** for the application version:
> [Edit Build Constants File With New Version](./../build_constants.toml)

Every other location derives the version from that file:

| Consumer | How it reads the version |
|---|---|
| `crates/ridgeback-app/build.rs` | Parses `build_constants.toml` at compile time → sets `RIDGEBACK_VERSION` env var used by `app.rs` |
| `scripts/bundle-macos.sh` | Parses `build_constants.toml` via `grep`/`sed` at bundle time |
| `Cargo.toml` `[workspace.package]` | Synced by `scripts/sync-version.sh` (and also by `build.rs` as a safety net) |
| All 6 crate `Cargo.toml` files | Inherit from `[workspace.package]` via `version.workspace = true` |

### Build code

In addition to the semantic version, every build gets a unique **build code**
of the form `RBTC:<commit-count>` (e.g. `RBTC:347`). This is generated
automatically in `build.rs` from `git rev-list --count HEAD` — no manual
action needed.

---

## Step-by-Step: Releasing a New Version

### 1. Decide the new version

Follow [Semantic Versioning](https://semver.org/):

- **Patch** (`0.1.0` → `0.1.1`): Bug fixes, minor tweaks.
- **Minor** (`0.1.1` → `0.2.0`): New features, backwards-compatible.
- **Major** (`0.2.0` → `1.0.0`): Breaking changes.

### 2. Update `build_constants.toml`

Edit the file at the repo root:

```toml
[version]
major = 0
minor = 2
patch = 0
```

### 3. Run the version sync script

```bash
./scripts/sync-version.sh
```

This updates:
- `Cargo.toml` `[workspace.package].version`
- `Cargo.lock`

### 4. Verify everything looks right

```bash
# Check the runtime version the binary will report:
cargo build -p ridgeback-app 2>&1 | tail -5

# Quick smoke test:
cargo run -p ridgeback-app -- --version 2>/dev/null || true

# Verify Cargo.toml is correct:
grep 'version' Cargo.toml | head -3
```

### 5. Commit and tag

```bash
VERSION="0.2.0"   # ← match what you set above

git add -A
git commit -m "Bump version to ${VERSION}"
git tag "v${VERSION}"
```

### 6. Push a release branch

Ridgeback's CI is triggered by pushes to `releases/**` branches:

```bash
VERSION="0.2.0"

git push origin main              # push the commit & tag to main first
git push origin "v${VERSION}"     # push the tag

# Create and push the release branch (triggers CI):
git checkout -b "releases/v${VERSION}"
git push origin "releases/v${VERSION}"
```

> **What happens next:** The GitHub Actions workflow (`.github/workflows/release.yml`)
> will:
> 1. Read the version from `build_constants.toml`
> 2. Build release binaries for all configured platforms
> 3. Bundle the macOS `.app` (via `scripts/bundle-macos.sh`)
> 4. Create a GitHub Release tagged `v<VERSION>` with the built artifacts

### 7. Write release notes

After CI finishes, go to the
[Releases page](../../releases) on GitHub and edit the draft release:

- Summarize notable changes, new features, and bug fixes.
- Mention any breaking changes prominently.
- Thank contributors if applicable.

Then click **Publish release**.

---

## Quick Reference

```bash
# Full release flow (copy-paste friendly):
# 1. Edit build_constants.toml with the new version, then:

./scripts/sync-version.sh

VERSION="X.Y.Z"
git add -A
git commit -m "Bump version to ${VERSION}"
git tag "v${VERSION}"
git push origin main
git push origin "v${VERSION}"
git checkout -b "releases/v${VERSION}"
git push origin "releases/v${VERSION}"
```

---

## Troubleshooting

### Version mismatch between binary and Cargo.toml

The `build.rs` script automatically syncs the workspace `Cargo.toml` version
from `build_constants.toml` every time you build. If they're out of sync, just
run:

```bash
cargo build -p ridgeback-app
```

Or manually:

```bash
./scripts/sync-version.sh
```

### CI didn't trigger

Make sure you pushed to a branch matching `releases/**`:

```bash
git push origin releases/v0.2.0
```

### Cargo.lock conflicts

After a version bump, `Cargo.lock` will change. If you have merge conflicts in
`Cargo.lock`, resolve them by running:

```bash
cargo update --workspace
```

