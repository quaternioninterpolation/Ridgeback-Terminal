# Changelog

All notable changes to Ridgeback Terminal will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Added
- **Lua-driven particle system** — particle effects are now entirely defined in Lua plugins. Users can create custom particles with full control over colours, physics, opacity, and spawn logic.
  - Three trigger modes: **keypress** (emit at cursor on typing), **newline** (emit on Enter), **fullscreen** (ambient effects every frame, e.g. snow, bubbles).
  - Particles specify direct RGBA colour with transparency (0–1 alpha).
  - Per-particle `gravity` and `drag` fields for custom physics.
  - Bundled plugins: 🔥 Fire Particles, ✨ Sparkles, ❄️ Snow.
  - Drop `.lua` files in `<config_dir>/ridgeback/plugins/` to add custom effects.
- **Snow particle plugin** — new bundled fullscreen particle effect (`snow_particles.lua`) with configurable density, fall speed, sway, colour, opacity, and size.
- **Particle opacity/transparency** — all particles now support per-particle alpha (0.0–1.0) set directly in Lua, with automatic quadratic fade-out over lifetime.

### Changed
- **Particle system migrated from Rust to Lua** — the built-in `BuiltinFireParticlePlugin` (Rust) has been removed. Fire particle logic now lives in `fire_particles.lua` with full emit code. The generic `ParticleEvent` struct no longer has `heat` or `is_smoke` fields; colour is specified directly as `[r, g, b, a]`.
- **Particle renderer generalised** — `draw_particles_overlay` no longer uses fire-specific two-pass smoke/ember rendering. All particles are rendered uniformly using their RGBA colour with life-based alpha fade.
- **Particle physics generalised** — `ParticleState::update` uses per-particle `gravity` and `drag` instead of hardcoded smoke/ember physics.
- **Plugin loading** — the plugin host now scans both bundled (`assets/plugins/`) and user (`<config_dir>/ridgeback/plugins/`) directories. Bundled plugins load first; user plugins override by ID.
- **Settings UI** — particle plugin list now shows trigger mode badges (⌨ keypress, ↵ newline, 🖥 fullscreen).

### Removed
- `BuiltinFireParticlePlugin` Rust struct — replaced by `fire_particles.lua`.
- `heat`, `is_smoke` fields from `ParticleEvent`.
- `heat_to_rgb` hardcoded colour ramp function.

### Added (continued from previous)
- **CRT barrel distortion** — the CRT shader now applies real per-pixel barrel distortion to terminal text via CPU rasterization with `fontdue` and a barrel-distorted egui mesh, replacing the old flat overlay that only drew grey rectangles.
- **Terminal padding** — configurable per-profile padding (percentage of screen width/height) with three editing modes in Settings:
  - **Uniform** — single slider for all sides.
  - **W × H** — separate horizontal and vertical sliders.
  - **Individual** — separate top/bottom/left/right sliders.
  - Default: 2.5% on all sides. Padding is applied to both normal rendering and CRT post-process.
- **FPS counter overlay** — toggleable from Settings → Rendering. Displays real GPU frame rate in a pill at the bottom-right corner.
- **Max shader FPS enforcement** — the configured max FPS limit is now actually enforced; all repaint requests use `request_repaint_after` instead of immediate repaints.
- **Background FPS limiting** — when "Update terminals in background" is off and the window loses focus, rendering is capped at 1 FPS instead of stopping entirely.
- Right-click context menu on profile list in Settings with **Duplicate** and **Remove** options.
  - Duplicate appends a count suffix to the name (e.g. "Zsh (1)", "Zsh (2)").
  - Remove prompts a confirmation dialog before deleting; the last profile cannot be removed.
- macOS code signing and notarization support in CI (`release.yml`).
  - Ad-hoc codesigning added to `bundle-macos.sh` for local builds.
  - Full Developer ID signing + Apple notarization when secrets are configured.
  - Hardened runtime entitlements (`scripts/entitlements.plist`) for wgpu/GPU compatibility.
- macOS Gatekeeper workaround instructions in GitHub Release notes.
- `CHANGELOG.md` — this file.

---

## [0.1.3] — 2026-03-03

### Added
- Initial public release.
- GPU-accelerated terminal emulator with egui/eframe and wgpu.
- Multi-tab support with IDE-style tab groups and split panes.
- Cross-group tab drag-and-drop with drop zone previews.
- Per-profile shader effects (CRT, fire) via Lua plugin system.
- Per-profile typing particle effects via Lua plugin system.
- Lua 5.4 sandboxed plugin host with built-in exporters (HTML, Markdown, JSON).
- Full wgpu 6-pass shader pipeline (background → text → shadow → composite → bloom → CRT).
- Glyph atlas with fontdue rasterization and instanced text rendering.
- AI integration: LM Studio, OpenAI, Claude, and local model (Qwen2.5-Coder) backends.
- AI autocomplete (ghost text) and natural-language command query (Ctrl+/).
- Local model management: download from HuggingFace, device selection, inference server.
- Sixel graphics protocol and iTerm2 inline image support.
- WSL profile auto-detection on Windows.
- ANSI 256-color and true-color support.
- Scrollback buffer with configurable limit.
- Input buffer with undo/redo, selection, history navigation, and clipboard support.
- Find overlay (Ctrl+F) with regex and case-sensitive search.
- Toast notification system with progress bars for downloads.
- Configurable keybindings (16 shortcut actions).
- TOML-based configuration with platform-specific defaults.
- macOS .app bundle via `scripts/bundle-macos.sh`.
- Cross-platform builds: Windows, macOS (x86_64 + aarch64), Linux.
- GitHub Actions release workflow with build matrix.
- Battery-aware frame pacing for shader effects.
- Google Cast / SSDP device discovery (streaming protocol stubbed).

---

[Unreleased]: https://github.com/quaternioninterpolation/Ridgeback-Terminal/compare/v0.1.3...HEAD
[0.1.3]: https://github.com/quaternioninterpolation/Ridgeback-Terminal/releases/tag/v0.1.3

