# AGENT.MD — Ridgeback Terminal Emulator

> **IMPORTANT**: This document is the source of truth for any AI assistant (Claude, ChatGPT, etc.) continuing work on this project. **You MUST update this file whenever you make changes** — add new modules, modify architecture, complete pending tasks, or add new ones.

---

## Project Overview

**Ridgeback** is a high-performance, GPU-accelerated, tabbed terminal emulator written in Rust.  
It targets **Windows, macOS, and Linux** with feature parity across all three.

- **Author**: Josh van den Heever  
- **License**: MIT (2025–2026)  
- **Workspace root**: `d:\Dev\RustTerm\` (may vary by machine — adjust paths accordingly)

### Core Design Principles

1. **Cross-platform first**: All code uses `#[cfg(target_os = "...")]` gates rather than hard-coding platform assumptions. Default shells, config paths, battery detection, and OS names are all platform-conditional.
2. **Separation of concerns**: 6 workspace crates with clear boundaries. The GUI crate (`ridgeback-app`) depends on all others but none of the library crates depend on GUI.
3. **egui for chrome, wgpu for terminal**: The tab bar, settings, overlays, and toasts use egui/eframe. The terminal viewport will eventually use a custom wgpu pipeline (currently using egui text rendering as a placeholder).
4. **No heavy deps for optional features**: Casting uses raw SSDP/UDP (std only). AI backends are behind config toggles. Shaders are stub-ready.

---

## Workspace Structure

```
d:\Dev\RustTerm\
├── Cargo.toml              # Workspace manifest (resolver = "2")
├── AGENT.MD                # ← THIS FILE — update on every change
├── README.md               # Public-facing readme
├── LICENSE                  # MIT
├── docs/
│   ├── casting.md          # Screen casting / Google Cast docs
│   ├── plugins.md          # Plugin API documentation
│   ├── profile-settings.md # Config file reference
│   └── shaders.md          # Shader effect documentation
├── assets/
│   └── fonts/
│       └── CascadiaMono.ttf
└── crates/
    ├── ridgeback-config/   # Configuration, profiles, WSL detection, serialization
    ├── ridgeback-core/     # Terminal engine (PTY, VT, grid, input, Sixel/image)
    ├── ridgeback-plugin/   # Plugin trait API + Lua 5.4 host + built-in exporters
    ├── ridgeback-gpu/      # wgpu multi-pass shader pipeline, glyph atlas, frame pacer
    ├── ridgeback-ai/       # AI backends (LM Studio, OpenAI, Claude, local)
    └── ridgeback-app/      # Binary crate — eframe GUI application
```

---

## Crate-by-Crate Architecture

### `ridgeback-config` — Configuration & Profiles

**Path**: `crates/ridgeback-config/src/`  
**Key deps**: `serde`, `toml`, `directories 5`

| File | Purpose |
|---|---|
| `lib.rs` | Top-level `Config` struct, `GeneralConfig`, `FontConfig`, load/save logic. Platform-conditional default profiles (including WSL on Windows) and default profile names via `#[cfg]`. Exports WSL utilities. |
| `profile.rs` | `Profile` struct (shell path, appearance, shader settings). `ShellType` enum: `Powershell`, `Cmd`, `Wsl`, `Bash`, `Zsh`, `Fish`, `Custom`. Platform-aware constructors: `default_powershell()`, `default_cmd()`, `default_wsl()`, `wsl_distro(name)`, `default_bash()`, `default_zsh()`, `default_fish()`. |
| `keybindings.rs` | `KeyBindings` (string-based combos like `"Ctrl+T"`), `ShortcutAction` enum (16 actions including split/group navigation). |
| `rendering.rs` | `RenderingConfig`: `max_shader_fps`, `battery_aware`, `update_in_background`. |
| `ai.rs` | `AiConfig` with sub-configs for LM Studio, OpenAI, Claude, and local models. Autocomplete and command query feature toggles. |
| `theme.rs` | `TabBarPosition` enum. |
| `wsl.rs` | WSL detection and profile generation. `WslDistro` struct, `detect_wsl_distros()` (parses UTF-16LE output of `wsl.exe --list --verbose`), `wsl_profiles()`, `is_wsl_available()`. Windows-only with empty stubs on other platforms. |

**Config file locations** (via `directories::ProjectDirs::from("", "", "Ridgeback")`):
- Windows: `%APPDATA%\Ridgeback\config\config.toml`
- macOS: `~/Library/Application Support/Ridgeback/config.toml`
- Linux: `~/.config/Ridgeback/config.toml`

**Platform defaults**:
- Windows: profiles = `powershell` + `cmd` + `wsl`, default = `powershell`
- macOS: profiles = `zsh` + `bash`, default = `zsh`
- Linux: profiles = `bash` + `zsh` + `fish`, default = `bash`

---

### `ridgeback-core` — Terminal Engine

**Path**: `crates/ridgeback-core/src/`  
**Key deps**: `vte 0.13`, `portable-pty 0.8`, `tokio` (mpsc channels)

| File | Purpose |
|---|---|
| `lib.rs` | Re-exports all public types including Sixel/image types. |
| `cell.rs` | `Cell` struct with `CellAttributes` (fg/bg `Color`, bold, italic, etc.). `Color` enum: `Default`, `Indexed(u8)`, `Rgb(u8,u8,u8)`. |
| `grid.rs` | `Grid` — 2D array of `Cell` with cursor tracking and region scrolling. |
| `buffer.rs` | `ScrollbackBuffer` — ring buffer of scrollback lines with configurable limit. |
| `vt_handler.rs` | `VtHandler` — implements `vte::Perform` trait. Processes ANSI/VT escape sequences, drives `Grid` + `ScrollbackBuffer`. Supports SGR colors, cursor movement, scroll regions, OSC title. **Sixel integration**: DCS `hook`/`put`/`unhook` for Sixel sequences, OSC 1337 for iTerm2 inline images. Fields: `sixel_decoder`, `image_layer`, `pending_images`. |
| `pty.rs` | `PtySession` — spawns shell via `portable-pty`. Background tokio task reads PTY output into `mpsc::UnboundedSender<PtyOutput>`. Cross-platform (ConPTY on Windows, Unix PTY on macOS/Linux). |
| `terminal.rs` | `Terminal` — combines `VtHandler` + `InputBuffer` + `PtySession`. Methods: `spawn()`, `process_pty_output()`, `write_to_pty()`, `submit_input()`, `send_interrupt()`, `search()`, `full_log()`. |
| `input_buffer.rs` | `InputBuffer` — fully-editable input line with cursor, selection, undo/redo (snapshot-based), history navigation, ghost text (AI suggestions), clipboard ops. Returns `InputAction` variants: `None`, `Changed`, `Submit(String)`, `Copy(String)`, `Cut(String)`, `SendToPty(Vec<u8>)`, `Interrupt`. **877 lines**, extensively unit-testable with zero GUI deps. |
| `search.rs` | `SearchOptions` + `SearchMatch` — find text in scrollback/grid with optional regex and case sensitivity. |
| `sixel.rs` | **Sixel graphics protocol + inline images.** `SixelDecoder` — full state-machine (Normal/ColorSelect/Repeat/RasterAttrs states), 256-color default palette, HLS→RGB conversion, repeat command, raster attributes, dynamic buffer growth. `SixelImage` (RGBA pixels + dimensions). `InlineImage` (image + grid placement + texture cache ID). `ImageLayer` — manages inline images with placement, viewport visibility, scroll tracking, GC. Also: `decode_iterm2_image()` for OSC 1337, `base64_decode()`. Includes unit tests for decoding, repeats, newlines, color defs, HLS, image layer ops. |

---

### `ridgeback-gpu` — GPU Pipeline

**Path**: `crates/ridgeback-gpu/src/`  
**Key deps**: `wgpu 23`, `bytemuck 1` (derive), `fontdue 0.9`, `ridgeback-config`

| File | Purpose |
|---|---|
| `lib.rs` | Module declarations + re-exports: `FramePacer`, `GlyphAtlas`, `GlyphInfo`, `ShaderPipeline`, `GlyphInstance`. |
| `frame_pacer.rs` | `FramePacer` — FPS capping with battery-aware throttling, idle detection, background rendering control. Cross-platform `is_on_battery()`: Windows FFI (`GetSystemPowerStatus`), macOS (`pmset -g batt` command), Linux (`/sys/class/power_supply/` sysfs). |
| `glyph_atlas.rs` | **CPU-side glyph rasterization + atlas packing.** `GlyphAtlas` (R8 texture, shelf-based rect packer, fontdue `Font`, `HashMap` cache, dirty flag). Methods: `new(w,h,font_data)`, `get_glyph(char,size)→GlyphInfo`, `precache_ascii(size)`, `cell_size(size)→(f32,f32)`, `clear()`. `GlyphInfo` holds UV rect + pixel metrics. |
| `shader_pipeline.rs` | **Full multi-pass wgpu pipeline (~940 lines).** `ShaderPipeline` holds `Arc<Device>`, `Arc<Queue>`, 6 offscreen textures+views, atlas texture, all render/compute pipelines, instance buffer. Uniform buffer types: `ViewportUniforms`, `CrtUniforms`, `FireUniforms`, `ShadowUniforms`, `BlurUniforms`, `CompositeUniforms` (all `#[repr(C)]` Pod). `GlyphInstance` — per-glyph instanced data (offset, size, UV, fg/bg color). 6-pass rendering: Background (fire/solid) → Text (instanced quads) → Shadow → Composite (alpha-over) → Bloom (dual Kawase blur) → CRT post-process. Optional pipelines (CRT/fire/shadow) created only when enabled. |

**WGSL Shaders** (`crates/ridgeback-gpu/shaders/`):

| File | Purpose |
|---|---|
| `text.wgsl` | Instanced glyph quad vertex + fragment shader. Unit quad × `GlyphInstance` with atlas texture sampling, alpha blending fg over bg. |
| `fire.wgsl` | Fire background with heat diffusion compute (8×8 workgroup, upward-biased convection, decay) + fire color ramp fragment shader (black→red→orange→yellow→white). |
| `crt.wgsl` | CRT post-process: barrel distortion, chromatic aberration (R/B offset), scanlines via sin(), vignette, bloom addition. |
| `shadow.wgsl` | Text shadow pass — samples text texture at offset, outputs dark shadow with modulated alpha. |
| `blur.wgsl` | Dual Kawase blur for bloom: `fs_downsample` (5-point) + `fs_upsample` (8-point weighted). |
| `composite.wgsl` | Alpha-over layer compositing: blends background + shadow + text via 3 input textures. |

---

### `ridgeback-ai` — AI Integration

**Path**: `crates/ridgeback-ai/src/`  
**Key deps**: `async-openai 0.25`, `reqwest 0.12` (json), `futures 0.3`, `tokio-stream 0.1`, `ridgeback-config`

| File | Purpose |
|---|---|
| `lib.rs` | Re-exports `AiBackend`, `AiService`, message types, all backend structs. |
| `backend.rs` | `AiBackend` trait (`name()`, `is_available()`, `complete()` → `Pin<Box<Future<Result<CompletionResponse>>>>`). Shared types: `AiRole`, `AiMessage`, `CompletionRequest`, `CompletionResponse`. **Four backend implementations**: |
| | • `LmStudioBackend` — `async-openai` client with custom `base_url`, OpenAI-compatible local API |
| | • `OpenAiBackend` — `async-openai` client with standard OpenAI endpoint + API key |
| | • `ClaudeBackend` — raw `reqwest` POST to `api.anthropic.com/v1/messages`, `x-api-key` + `anthropic-version` headers, handles system message separately per Anthropic API spec |
| | • `LocalModelBackend` — `async-openai` client pointing at `localhost:11434/v1` (Ollama/llama.cpp compatible), respects `context_length` from config |
| `service.rs` | `AiService` — high-level coordinator. `new()` wires all 4 backends via exhaustive `match` on `AiBackendType`. `suggest_completion()` for ghost text, `query_command()` for natural language → command generation. Both are shell-type-aware (all 7 variants including Wsl) and use `current_os_name()` (cfg-gated). |

---

### `ridgeback-plugin` — Plugin API & Lua Host

**Path**: `crates/ridgeback-plugin/src/`  
**Key deps**: `mlua 0.10` (lua54, vendored, send), `serde_json`, `directories 5`, `ridgeback-core`

| File | Purpose |
|---|---|
| `lib.rs` | Re-exports plugin trait types + Lua host types: `LuaPluginHost`, `PluginScript`, `PluginResult`, `TerminalSnapshot`, `HtmlExporter`, `MarkdownExporter`, `JsonExporter`. |
| `api.rs` | Trait-based plugin API: `TerminalQuery` for querying terminal state, `SaveFormatPlugin` for export formats, `StyledLine`/`StyledSpan` for rich text. `PluginSearchMatch` for search results. |
| `lua_host.rs` | **Lua 5.4 sandboxed plugin host (~340 lines).** `LuaPluginHost` loads `.lua` scripts from a directory. `TerminalSnapshot` provides a frozen copy of terminal state (last_lines, full_log, title, shell_type). **Sandbox security**: removes `io`, `os`, `loadfile`, `dofile`, `require`, `rawset`, `rawget`, `debug`; instruction-count hook (1M instructions ≈ 5s timeout); captures `print()` output via `Arc<Mutex<String>>`. Exposes read-only `terminal` table to Lua with `last_n_lines(n)`, `full_log()`, `title()`, `shell()`, `search(pattern)`. **Built-in exporters**: `HtmlExporter` (styled HTML with inline CSS), `MarkdownExporter` (code block), `JsonExporter` (structured JSON via serde_json) — all implement `SaveFormatPlugin`. |

---

### `ridgeback-app` — GUI Application (binary crate)

**Path**: `crates/ridgeback-app/src/`  
**Key deps**: `eframe 0.29`, `egui 0.29`, `rfd 0.15`, `arboard 3`

| File | Purpose |
|---|---|
| `main.rs` | Entry point. Loads config, sets up eframe `NativeOptions`, installs CascadiaMono font, creates `RidgebackApp`. Module declarations for all app modules. |
| `app.rs` | `RidgebackApp` — main state struct (config, tabs, shortcuts, settings, AI, clipboard, cast manager, split panes, tab drag). Implements `eframe::App::update()` — processes PTY output across all groups, checks shortcuts, renders a compact toolbar at top, and split-pane central panel where each leaf renders its own tab group header + terminal. Handles deferred `TabBarAction` processing and drag-to-split drop zones. |
| `tabs.rs` | `TabGroup` and `TabManager`. `TabGroup` holds a `Vec<TabState>`, `active` index, and unique stable `id`. Each group is an independent tab container with its own tab bar (like IDE editor groups). `TabManager` manages a `Vec<TabGroup>` with `focused_group_id`, provides flat iterators across all tabs (`all_tabs_mut()`), group navigation, and tab movement between groups. |
| `group_tab_bar.rs` | Per-group tab bar rendering. `draw_group_tab_bar()` renders the tab strip with active highlight, close buttons, animations, overflow "⋯" menu for hidden tabs, right-click context menu (Close, Close Others, Close to Right, Split Right, Split Down, Tear Off, Close Group), and a `+` new tab button. Returns `Vec<TabBarAction>` for deferred processing. `draw_drop_zone_preview()` renders animated blue highlight zones during tab drag. |
| `split_pane.rs` | `SplitPane` recursive tree with `Single(group_id)` leaves (stable IDs, never renumbered). `SplitPaneManager` manages the tree, tracks `focused_group_id`. Supports horizontal/vertical splitting, group removal with auto-collapse, and drop zone computation for drag-to-split. Each leaf is rendered as a 28px tab bar header + terminal body. |
| `tab_drag.rs` | `TabDragState` with `DragSource { group_id, tab_idx }` for cross-group drag. Supports within-group reorder, cross-group movement via drop zones, and floating ghost tab rendering during drag. No multi-window tear-off (removed). |
| `terminal_view.rs` | `show_terminal()` — renders the VT grid with color support (ANSI 256-color palette), cursor highlighting, scrollback, input line with selection/ghost text, keyboard event handling. `handle_key()` dispatches to `InputBuffer` methods and routes `InputAction` results to PTY/clipboard. |
| `shortcuts.rs` | `ShortcutManager` — parses string keybindings into `KeyCombo` structs, checks egui input each frame. 16 actions including split/group navigation. |
| `settings.rs` | `SettingsWindow` — tabbed settings UI (Profiles, Shortcuts, Rendering, AI, Plugins, Cast/Share). Shortcuts tab shows all 16 keybindings. |
| `find_overlay.rs` | `FindOverlay` — Ctrl+F search panel over the terminal. |
| `command_query.rs` | `CommandQueryOverlay` — Ctrl+/ AI command query panel. |
| `casting.rs` | `CastManager` + SSDP/DIAL discovery. Background thread for device scanning. |
| `toast.rs` | `ToastManager` + `Toast` — transient notification system. |
| `particle_emit.rs` | Bridge between `TabState` and the active `ParticlePlugin`. |

---

## Key Shortcuts (Defaults)

| Shortcut | Action |
|---|---|
| Ctrl+T | New tab in focused group |
| Ctrl+W | Close active tab |
| Ctrl+Tab | Next tab (within group) |
| Ctrl+Shift+Tab | Previous tab (within group) |
| Ctrl+Shift+D | Split right (new group) |
| Ctrl+Shift+E | Split down (new group) |
| Ctrl+Shift+W | Close group (all tabs) |
| Ctrl+Alt+Right | Focus next group |
| Ctrl+Alt+Left | Focus previous group |
| Ctrl+Alt+Shift+Right | Move tab to next group |
| Ctrl+Alt+Shift+Left | Move tab to previous group |
| Ctrl+, | Open settings |
| Ctrl+S | Save session to file |
| Ctrl+F | Find in session |
| Ctrl+/ | AI command query |
| Ctrl+Shift+P | Reload plugins |
| Ctrl+C | Copy selection / interrupt (if no selection & empty input) |
| Ctrl+V | Paste from clipboard |
| Ctrl+Z | Undo |
| Ctrl+Shift+Z / Ctrl+Y | Redo |
| Ctrl+A | Select all input |
| Tab | Accept ghost text (AI autocomplete) |

---

## Cross-Platform Implementation Details

### Platform `#[cfg]` Gates Used

- **Shell defaults**: `profile.rs` — `platform_default()`, `default_powershell()` uses `"pwsh.exe"` on Windows, `"pwsh"` elsewhere
- **Config defaults**: `lib.rs` — `Config::default()` creates platform-specific profile sets (Windows includes WSL)
- **WSL detection**: `wsl.rs` — `detect_wsl_distros()` uses `wsl.exe --list --verbose` with UTF-16LE output parsing (Windows only; empty vec on other platforms); `is_wsl_available()` checks `wsl.exe --status`
- **Battery detection**: `frame_pacer.rs` — `is_on_battery()` with Windows FFI, macOS `pmset`, Linux sysfs
- **OS name for AI**: `service.rs` — `current_os_name()` returns `"Windows"` / `"macOS"` / `"Linux"` / `"Unix"`
- **Shortcuts**: `keybindings.rs` uses `"Ctrl"` everywhere; `shortcuts.rs` parser treats `"Cmd"` ↔ `"Ctrl"` as equivalent (egui `mac_cmd`)
- **Lua plugin host**: `lua_host.rs` — uses `directories::ProjectDirs` for platform-specific plugin directory

### PTY Abstraction

`portable-pty` handles the platform split:
- **Windows**: ConPTY (Windows 10 1809+)
- **macOS/Linux**: POSIX `openpty()` / `forkpty()`

No conditional compilation needed in `pty.rs` itself — `portable-pty` abstracts it.

---

## Completed Work (Recent Sessions)

### Session: Roadmap Feature Completion

All 6 previously-unchecked roadmap items have been fully implemented:

1. **Full wgpu shader pipeline (CRT, fire, bloom)** ✅  
   - 6 WGSL shader files in `crates/ridgeback-gpu/shaders/`
   - `ShaderPipeline` module (~940 lines) with 6-pass rendering architecture
   - All uniform buffer types with `#[repr(C)]` Pod derive
   - Optional pipeline creation based on active `ShaderEffect`

2. **Glyph atlas with instanced text rendering** ✅  
   - `GlyphAtlas` with fontdue rasterization + shelf-based rect packing
   - `GlyphInstance` for per-glyph instanced data
   - ASCII pre-caching, cell size calculation

3. **Lua plugin host runtime** ✅  
   - `LuaPluginHost` with sandboxed Lua 5.4 (mlua, vendored)
   - Security: dangerous globals removed, instruction-count timeout
   - Read-only `terminal` API table for scripts
   - Built-in exporters: HTML, Markdown, JSON

4. **LM Studio / OpenAI / Claude backend integration** ✅  
   - `OpenAiBackend` via async-openai (standard endpoint)
   - `ClaudeBackend` via reqwest (Anthropic Messages API)
   - `LocalModelBackend` via async-openai (Ollama/llama.cpp at localhost:11434)
   - All 4 backends wired in `AiService::new()` via exhaustive match

5. **WSL profile support** ✅  
   - `Profile::default_wsl()` and `Profile::wsl_distro(name)` constructors
   - WSL auto-included in Windows default profiles
   - `wsl.rs` module: distro detection via `wsl.exe --list --verbose` (UTF-16LE parsing)
   - `wsl_profiles()` generates profiles for each detected distro

6. **Sixel / image protocol support** ✅  
   - `SixelDecoder` — full DCS state machine with color palette, HLS→RGB, repeat, raster attributes
   - `ImageLayer` — manages inline images with grid placement, viewport visibility, scroll tracking, GC
   - iTerm2 inline images via OSC 1337 (`decode_iterm2_image()`)
   - Integrated into `VtHandler`: DCS `hook`/`put`/`unhook` + OSC dispatch
   - Unit tests for decoding, repeats, newlines, color definitions, HLS conversion, image layer ops

### Session: IDE-Style Tab Groups, Drag-to-Split, Docs Overhaul

Complete redesign of the split-pane/tab system and documentation:

1. **Tab Groups** ✅
   - `TabGroup` struct with own `Vec<TabState>`, `active` index, stable `id`
   - `TabManager` refactored to hold `Vec<TabGroup>`, `focused_group_id`, group lifecycle methods
   - Flat iterators (`all_tabs_mut()`) for cross-group PTY/settings processing

2. **Split Pane Redesign** ✅
   - `SplitPane::Single(group_id)` — leaves reference groups by stable ID (never renumbered)
   - Removed `remap_after_remove` — group IDs are stable
   - Drop zone computation for drag-to-split (left/right/top/bottom/center zones per leaf)
   - Auto-collapse when a group becomes empty

3. **Per-Group Tab Bar (`group_tab_bar.rs`)** ✅
   - Tab strip rendering with animations, close buttons, active highlight
   - Right-click context menu: Close, Close Others, Close to Right, Split Right, Split Down, Tear Off, Close Group
   - Overflow "⋯" menu for hidden tabs (scrollable list)
   - `+` button for new tab in the group
   - `TabBarAction` enum for deferred processing

4. **Cross-Group Tab Drag** ✅
   - `DragSource { group_id, tab_idx }` replaces old `Option<usize>`
   - Edge drop zones create new groups via split; center zone moves tab into existing group
   - Animated blue highlight preview during drag
   - Ghost tab rendering under cursor
   - Removed multi-window tear-off infrastructure (`lazy_static`, `DetachedTabInfo`, `IN_FLIGHT_TAB`)

5. **New Keybindings** ✅
   - `FocusNextGroup` (Ctrl+Alt+Right), `FocusPrevGroup` (Ctrl+Alt+Left)
   - `MoveTabNextGroup` (Ctrl+Alt+Shift+Right), `MoveTabPrevGroup` (Ctrl+Alt+Shift+Left)
   - 16 total `ShortcutAction` variants (was 12)

6. **Toolbar Redesign** ✅
   - Global top tab bar replaced with compact toolbar: + (profile picker), split buttons, settings gear
   - Per-group tab bars live inside each split pane leaf (28px header)

7. **Documentation Overhaul** ✅
   - `README.md` simplified — humble tone, removed architecture/tech stack/roadmap
   - `docs/architecture.md` created — tech stack, crate structure, tab group model, shader pipeline
   - `docs/shaders.md` — added per-tab note, Lua plugin registration path
   - `docs/plugins.md` — added Shader Plugins and Particle Plugins sections
   - `docs/profile-settings.md` — all 16 keybindings documented
   - `AGENT.MD` — updated module tables, shortcuts, completed work

8. **Focus Bug Fix** ✅
   - Old bug: clicking a tab in the global bar set `tabs.active` but the split tree leaf still held `Single(0)`, so new tabs never rendered and switching tabs broke
   - Fixed by design: each leaf is now a group with its own `active` index; tab bar clicks call `group.set_active(idx)` directly

9. **Widget ID Collision Fix** ✅
   - Bug: all terminals shared `id_salt("terminal_output")` for ScrollArea and `ui.id().with("inline_input")` for keyboard focus — egui reported "Second use of ScrollArea ID" and "Second use of widget ID" when split
   - Bug: every terminal called `m.request_focus(input_id)` every frame when nothing was focused, so all terminals received keyboard input simultaneously
   - Fix: added `terminal_id: u64` to `TabState` (auto-incrementing `AtomicU64` counter, never reused)
   - ScrollArea: `id_salt(("terminal_output", terminal_id))` — unique per terminal
   - Input focus: `egui::Id::new(("inline_input", terminal_id))` — unique per terminal
   - Auto-focus: only the focused group's active tab calls `request_focus` when nothing is focused
   - `show_terminal()` now takes `terminal_id: u64` and `is_focused_terminal: bool` parameters
   - `handle_keyboard_input()` early-returns if `!is_focused_terminal`

---

## Pending / In-Progress Work

### MUST DO (incomplete from earlier sessions)

1. **Drag & Drop Module** (`ridgeback-app/src/drag_drop.rs`)  
   - **NOT YET CREATED** — the `toast.rs` module was created but the drag_drop module itself has not been written yet.
   - Requirements:
     - Support drag & drop for **selected text** and **files**
     - **Default file drop**: inserts the file path into the input buffer
     - **Alt+drop**: when Alt is held while dropping a file, show a context popup with options:
       - "Insert file path" (default behavior)
       - "Insert file contents"
     - **Alt+hover indicator**: when Alt is held and a file is hovering over the window, change the cursor to indicate alternative options are available
     - **Truncation warning**: if file contents exceed a max input size, insert truncated content and fire a toast warning via `ToastManager`
     - **Text drag**: dragging selected text from the terminal should work as a standard drag source
   - Implementation approach:
     - Use egui's `RawInput::dropped_files` and `RawInput::hovered_files` for drag detection
     - Track `Alt` modifier state during hover via `egui::Modifiers`
     - Read file contents with `std::fs::read_to_string()`, truncate to configurable max (suggest 64KB default)
     - Wire into `app.rs` update loop — process drops before rendering central panel
     - Use `ToastManager` for truncation notifications
   - Add `mod drag_drop;` to `main.rs`
   - Add `DragDropManager` (or equivalent state) to `RidgebackApp`
   - Wire into `terminal_view.rs` or `app.rs` update loop

2. **Wire toast.rs into app.rs**  
   - `toast.rs` exists but is **not yet imported or used** in `main.rs` / `app.rs`
   - Add `mod toast;` to `main.rs`
   - Add `ToastManager` field to `RidgebackApp`
   - Call `self.toast_manager.show(ctx)` at the end of `update()`
   - Pass `&mut self.toast_manager` to drag_drop handling and anywhere else that needs it

3. **Wire wgpu pipeline into terminal_view.rs**  
   - The shader pipeline and glyph atlas are fully implemented but the terminal viewport in `terminal_view.rs` still uses egui's `LayoutJob` text renderer as a placeholder.
   - Future work: create wgpu device/queue, instantiate `ShaderPipeline`, upload glyph instances each frame, and display the final texture as an egui image.

4. **Wire Sixel images into terminal rendering**  
   - `VtHandler` now populates `pending_images` and `image_layer`, but `terminal_view.rs` does not yet read or render them.
   - Need to drain `pending_images`, call `image_layer.place_image()` with cell dimensions, and render image textures in the terminal viewport.

5. **Wire Lua plugin host into app**  
   - `LuaPluginHost` exists in the plugin crate but is not yet instantiated in `RidgebackApp` or exposed in the Settings UI.
   - Need to load plugins on startup, provide a "Plugins" settings tab, and wire export plugins into the Save Session flow.

### SHOULD DO (improvements)

6. **Full example config in `docs/profile-settings.md`**  
   - The "Full Example Config" section at the bottom currently only shows `[profiles.powershell]` and `[profiles.cmd]`. Add examples for `[profiles.bash]`, `[profiles.zsh]`, `[profiles.wsl]`, etc.

7. **macOS Cmd key support in keybindings**  
   - Currently defaults are `"Ctrl+T"` etc. On macOS, users expect `Cmd+T`. The parser already maps `"Cmd"` → `ctrl = true` which works with egui's `mac_cmd`, but the default config should offer Cmd variants on macOS.

8. **Full CASTV2 protocol**  
   - `casting.rs` implements SSDP discovery but not the full Google Cast V2 protobuf-based streaming protocol. Current implementation discovers devices but actual streaming is stubbed.

9. **PNG/JPEG decoding for iTerm2 images**  
   - `decode_iterm2_image()` handles base64 decoding and the iTerm2 protocol, but image format decoding (PNG, JPEG, GIF) is stubbed. Adding the `image` crate would enable full format support.

---

## Coding Conventions

- **Rust edition**: 2021
- **Error handling**: `anyhow::Result` for fallible operations, `tracing::error!()` for logging
- **Serialization**: `serde` with `#[serde(default)]` on all config structs for forward-compatible TOML
- **Naming**: Crates prefixed `ridgeback-`, modules are snake_case, types are PascalCase
- **Platform code**: Always use `#[cfg(target_os = "...")]` with a fallback `#[cfg(not(any(...)))]` arm
- **GUI pattern**: Free functions (`show_terminal()`, `show_cast_panel()`) that take `&mut egui::Ui` plus state refs — not methods on widget structs
- **Input handling**: All input editing logic lives in `InputBuffer` (core crate) returning `InputAction` enums — the GUI layer dispatches actions, never mutates state directly

---

## Key Workspace Dependencies

```toml
# Workspace-level (Cargo.toml)
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
anyhow = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tokio = { version = "1", features = ["full"] }

# ridgeback-app
eframe = "0.29"
egui = "0.29"
rfd = "0.15"
arboard = "3"

# ridgeback-config
directories = "5"

# ridgeback-core
vte = "0.13"
portable-pty = "0.8"

# ridgeback-gpu
wgpu = "23"
bytemuck = { version = "1", features = ["derive"] }
fontdue = "0.9"

# ridgeback-ai
async-openai = "0.25"
reqwest = { version = "0.12", features = ["json"] }
futures = "0.3"
tokio-stream = "0.1"

# ridgeback-plugin
mlua = { version = "0.10", features = ["lua54", "vendored", "send"] }
directories = "5"
```

---

## How to Build & Run

```bash
# From workspace root
cargo build                    # Debug build
cargo build --release          # Release build
cargo run -p ridgeback-app     # Run the terminal
cargo test --workspace         # Run all tests

# Environment variable for verbose logging
RUST_LOG=debug cargo run -p ridgeback-app
```

---

## Update Instructions for Future AI Agents

**When you make ANY changes to this project, you MUST update this file:**

1. If you **add a new module/file**: Add it to the appropriate crate table in the "Crate-by-Crate Architecture" section.
2. If you **complete a pending task**: Move it from "Pending / In-Progress Work" to a "Completed" section (or remove it and note what was done).
3. If you **add new pending work**: Add it to the "Pending / In-Progress Work" section with clear requirements.
4. If you **change architecture** (new crate, new dependency, changed trait): Update the relevant sections.
5. If you **add new shortcuts/keybindings**: Update the "Key Shortcuts" table.
6. If you **change platform behavior**: Update the "Cross-Platform Implementation Details" section.

**Keep this file accurate. It is the handoff document between sessions.**
