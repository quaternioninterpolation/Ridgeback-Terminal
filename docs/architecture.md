# Architecture & Tech Stack

Technical details about how Ridgeback is put together.

---

## Crate Structure

Ridgeback is a Rust workspace with six crates:

```
crates/
├── ridgeback-config   # Configuration types, TOML load/save, platform defaults
├── ridgeback-core     # Terminal engine: VT parser, PTY, grid, input buffer, search, Sixel
├── ridgeback-plugin   # Plugin API (Rust traits + Lua 5.4 host), export plugins
├── ridgeback-gpu      # wgpu multi-pass shader pipeline, glyph atlas, frame pacer
├── ridgeback-ai       # AI backend abstraction (LM Studio, OpenAI, Claude, local)
└── ridgeback-app      # eframe GUI application: tab groups, split panes, terminal view
```

The GUI crate (`ridgeback-app`) depends on all others. None of the library crates depend on the GUI.

---

## Tech Stack

| Area | Library | Notes |
|---|---|---|
| GUI chrome | [egui](https://github.com/emilk/egui) / [eframe](https://github.com/emilk/egui/tree/master/crates/eframe) | Tab bars, settings, overlays, toasts |
| Terminal viewport | [wgpu](https://wgpu.rs/) | Custom multi-pass shader pipeline (currently using egui text as placeholder) |
| VT parsing | [vte](https://crates.io/crates/vte) | ANSI/VT100/xterm sequences |
| PTY | [portable-pty](https://crates.io/crates/portable-pty) | ConPTY on Windows, Unix PTY elsewhere |
| Glyph rendering | [fontdue](https://crates.io/crates/fontdue) | CPU rasterization + shelf-based atlas packing |
| Shaders | WGSL | 6 shader files in `crates/ridgeback-gpu/shaders/` |
| AI | [async-openai](https://crates.io/crates/async-openai), [reqwest](https://crates.io/crates/reqwest) | LM Studio, OpenAI, Claude, local (Ollama) |
| Config | [toml](https://crates.io/crates/toml) + [serde](https://serde.rs/) | Single TOML file with profiles |
| Plugins | [mlua](https://crates.io/crates/mlua) | Lua 5.4, sandboxed, vendored |
| Icons | [egui-phosphor](https://crates.io/crates/egui-phosphor) | Phosphor icon font |

---

## Tab Groups & Split Panes

The window layout is a recursive binary tree (`SplitPane`):

- **`Single(group_id)`** — a leaf containing one `TabGroup`
- **`Horizontal(left, right, ratio)`** — side-by-side split
- **`Vertical(top, bottom, ratio)`** — top/bottom split

Each `TabGroup` has its own tab bar header, active tab index, and list of `TabState` objects. This mirrors how IDE editor groups work — each pane is independent and can host multiple tabs.

`TabManager` holds all groups and tracks the focused group. Group IDs are stable (never renumbered when groups are added or removed).

### Rendering flow

1. `TopBottomPanel::top` — thin toolbar (new tab, split buttons, settings gear)
2. `CentralPanel` → `SplitPaneManager::show()` — walks the tree recursively
3. For each `Single(group_id)` leaf:
   - 28px header: `draw_group_tab_bar()` renders the group's tab strip
   - Remaining body: `show_terminal()` renders the active tab's terminal
4. Drop-zone preview overlay (during tab drag)
5. Toast notifications

### Tab drag-to-split

Dragging a tab from one group's header activates drop-zone detection:
- Edge zones (left 25%, right 25%, top 25%, bottom 25%) create a new group via split
- Center zone (middle 50%) moves the tab into the target group
- A semi-transparent blue highlight previews where the tab would land

---

## Shader Pipeline

Six WGSL shaders in `crates/ridgeback-gpu/shaders/`:

| File | Purpose |
|---|---|
| `text.wgsl` | Instanced glyph quads with atlas sampling |
| `fire.wgsl` | Fire background with heat diffusion compute shader |
| `crt.wgsl` | CRT post-process (scanlines, curvature, chromatic aberration) |
| `shadow.wgsl` | Text shadow pass |
| `blur.wgsl` | Dual Kawase blur for bloom |
| `composite.wgsl` | Layer compositing (background + shadow + text + bloom) |

Each shader effect is configured per-profile, so different tabs within a group can have different effects.

---

## Platform Notes

| Platform | PTY Backend | GPU Backend | Shell Defaults |
|---|---|---|---|
| Windows | ConPTY | DX12 / Vulkan | PowerShell, CMD, WSL |
| macOS | POSIX PTY | Metal | Zsh, Bash |
| Linux | POSIX PTY | Vulkan | Bash, Zsh, Fish |

Platform-specific code uses `#[cfg(target_os = "...")]` with fallback arms.

---

## Key Dependencies

```toml
# Workspace
serde = "1"
toml = "0.8"
anyhow = "1"
tracing = "0.1"
tokio = { version = "1", features = ["full"] }

# ridgeback-app
eframe = "0.29"
egui = "0.29"
rfd = "0.15"
arboard = "3"

# ridgeback-core
vte = "0.13"
portable-pty = "0.8"

# ridgeback-gpu
wgpu = "23"
fontdue = "0.9"

# ridgeback-ai
async-openai = "0.25"
reqwest = "0.12"

# ridgeback-plugin
mlua = "0.10"
```

