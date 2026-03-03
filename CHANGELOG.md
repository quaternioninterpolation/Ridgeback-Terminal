# Changelog

All notable changes to Ridgeback Terminal will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [0.1.6] — 2026-03-04

### Fixed
- **Backspace interpreted as space/tab in macOS `.app` bundles** — when launched as a bundled `.app`, the PTY inherited `launchd`'s minimal environment with no controlling terminal, causing `openpty()` to produce wrong termios defaults (e.g. incorrect `VERASE`). Fixed by explicitly setting sane termios control characters (`VERASE=DEL`, `VINTR=^C`, `VEOF=^D`, etc.) and line discipline flags after PTY creation, and by ensuring `TERM=xterm-256color`, `COLORTERM=truecolor`, and `LANG=en_US.UTF-8` are set in the PTY environment.
- **Particle plugins and shaders missing in macOS `.app` bundles** — the bundle script now copies `assets/plugins/*.lua` into `Contents/Resources/plugins/` and `crates/ridgeback-gpu/shaders/*.wgsl` into `Contents/Resources/shaders/`. Added `Contents/Resources/shaders` as a lookup candidate in `find_shaders_dir()` (was already present for plugins).
- **Console window appearing behind the GUI on Windows** — added `#![windows_subsystem = "windows"]` to suppress the shadow cmd/terminal window that appeared when launching the app on Windows.
- **IME spurious text events on macOS** — disabled IME on the viewport (`IMEAllowed(false)`, `IMEPurpose::Terminal`) and restructured keyboard event processing into a two-pass loop to suppress spurious `Event::Text` events alongside non-printable key presses.

---

## [0.1.4] — 2026-03-03

### Added
- **Lua-driven particle system** — particle effects are now entirely defined in Lua plugins. Users can create custom particles with full control over colours, physics, opacity, and spawn logic.
  - Three trigger modes: **keypress** (emit at cursor on typing), **newline** (emit on Enter), **fullscreen** (ambient effects every frame).
  - Particles specify direct RGBA colour with transparency (0–1 alpha) and per-particle `gravity` / `drag` fields.
  - Drop `.lua` files in `<config_dir>/ridgeback/plugins/` to add custom effects; press Ctrl+Shift+P to reload.
- **Multi-effect particle system** — profiles now support a **list** of particle effects instead of a single one. Stack typing sparkles, fullscreen snow, and rain simultaneously.
  - Settings UI shows each effect as a card with ✅ enable toggle, plugin selector, trigger badges (⌨/↵/🖥), per-effect parameter editors, and 🗑 remove button.
  - **➕ Add Effect** button to append new effects; each is independently configurable.
  - Config format: `[[particle_effects]]` array-of-tables. Old `typing_particles = "fire"` configs auto-migrate.
- **Bundled particle plugins:**
  - 🔥 **Fire Particles** (`fire_particles.lua`) — embers and smoke puffs on keypress with configurable colours, counts, and speed.
  - ✨ **Sparkles** (`sparkle_particles.lua`) — colourful burst on keypress with configurable colour, opacity, lifetime, and speed.
  - ❄️ **Snow** (`snow_particles.lua`) — fullscreen snowflakes drifting down with configurable density, fall speed, sway, colour, opacity, and size. Snowflakes pile up at the bottom of the viewport, capped to the bottom padding height, then slowly fade.
  - 🌧️ **Rain** (`rain_particles.lua`) — fullscreen rain with three layers: fast rain streaks with wind, splash bursts on impact at the floor, and a translucent flood layer that builds up at the bottom. Configurable density, fall speed, wind, drop/splash/flood colours, opacity, and splash intensity.
- **Snow / rain pileup** — downward-moving particles that reach the viewport floor settle in place and stack up using 16 x-buckets for a natural uneven pile. Pile height is capped at the bottom padding so settled particles never overlap terminal text. Settled particles fade out slowly over ~5 seconds.
- **Particle opacity/transparency** — all particles support per-particle alpha (0.0–1.0) set directly in Lua, with automatic quadratic life-based fade-out.
- **Padding pixel preview** — the padding settings sliders now display both the percentage and the computed pixel value (e.g. `2.5% (15px)`), with a summary line showing all four sides.
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

### Changed
- **Particle system migrated from Rust to Lua** — the built-in `BuiltinFireParticlePlugin` (Rust) has been removed. Fire particle logic now lives entirely in `fire_particles.lua`. The `ParticleEvent` struct no longer has `heat` or `is_smoke` fields; colour is specified directly as `[r, g, b, a]`.
- **Particle renderer generalised** — `draw_particles_overlay` renders all particles uniformly using their RGBA colour with life-based alpha fade. No more fire-specific two-pass smoke/ember rendering.
- **Particle physics generalised** — `ParticleState::update_with_floor` uses per-particle `gravity` and `drag` instead of hardcoded smoke/ember physics, plus floor collision for pileup.
- **Particles respect max FPS and background update settings** — fullscreen particle emission and physics only advance on frames allowed by the `max_shader_fps` throttle. Existing particles are always rendered (no flicker). The old `request_repaint()` (which bypassed the FPS cap) has been replaced by the app's `request_repaint_after(shader_interval)` scheduler.
- **Particles render on full viewport** — `draw_particles_overlay` now receives `full_rect` (including padding) instead of `term_rect`, so fullscreen effects like snow and rain cover the entire viewport. Drawn before the shader overlay so CRT/fire post-processing applies on top.
- **Typing particle cursor alignment fixed** — particle spawn coordinates are now computed relative to `full_rect` (matching the particle render space) instead of `term_rect`, so typing particles appear exactly at the cursor position.
- **Uniform padding now produces equal pixel padding on all sides** — padding percentages reference `min(width, height)` instead of each axis independently. On a 1200×800 window at 2.5%, all four sides get 20px (the smaller dimension) instead of 30px horizontal / 20px vertical. Applied consistently in both terminal rendering and CRT post-processing.
- **Plugin loading** — the plugin host scans both bundled (`assets/plugins/`) and user (`<config_dir>/ridgeback/plugins/`) directories. Bundled plugins load first; user plugins override by ID.
- **Config format** — `typing_particles` field replaced by `particle_effects` (Vec). Old string and table formats auto-migrate via serde `alias` + custom deserializer. `TypingParticlesConfig` kept as a type alias for backward compatibility.
- **Settings UI** — "Typing Particles" section renamed to "Particle Effects" with full list management. Registered plugins grid shows trigger mode column.

### Removed
- `BuiltinFireParticlePlugin` Rust struct — replaced by `fire_particles.lua`.
- `heat`, `is_smoke` fields from `ParticleEvent`.
- `heat_to_rgb` hardcoded colour ramp function.
- Single-effect `typing_particles` config field (migrated to `particle_effects` list).

### Fixed
- Typing particles appearing offset from the cursor when terminal padding is enabled.
- Fullscreen particles (snow, rain) bypassing the `max_shader_fps` cap by calling `request_repaint()` directly.
- Fullscreen particles continuing to emit at full rate when the window is unfocused and `update_in_background` is off.
- Uniform padding producing different pixel values on horizontal vs vertical axes.
- UTF-8 BOM in `terminal_view.rs` causing a compilation error.

### Added

[0.1.3]: https://github.com/quaternioninterpolation/Ridgeback-Terminal/releases/tag/v0.1.4

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

[0.1.3]: https://github.com/quaternioninterpolation/Ridgeback-Terminal/releases/tag/v0.1.3
