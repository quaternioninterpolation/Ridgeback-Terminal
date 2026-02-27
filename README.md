# Ridgeback

A modern, GPU-accelerated terminal emulator built in Rust — fast, extensible, and beautiful.

![Rust](https://img.shields.io/badge/Rust-1.75%2B-orange?logo=rust)
![License](https://img.shields.io/badge/license-MIT-blue)
![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey)

---

![icon](./assets/images/icon.png)

## Overview

Ridgeback is a high-performance tabbed terminal emulator for **Windows, macOS, and Linux** that combines the power of a native Rust application with a GPU-rendered viewport powered by `wgpu`. It ships with customizable shader effects (CRT scanlines, fire backgrounds), an AI-powered command assistant, a Warp-style editable input line, a Lua plugin system, and built-in screen casting support — all wrapped in an egui-based interface.

### Key Features

| Feature | Description |
|---|---|
| **Tabbed interface** | Multiple terminal sessions in a single window with profile-based tabs |
| **GPU shader effects** | CRT post-process, fire background with character heat map, bloom & blur |
| **AI integration** | Ghost-text autocomplete and Ctrl+/ natural-language command query via LM Studio, OpenAI, or Claude |
| **Editable input line** | Warp-style local input buffer with cursor movement, selection, undo/redo, clipboard |
| **Plugin system** | Extend via Rust traits or Lua 5.4 scripts |
| **Find in session** | Regex-capable search across scrollback and visible buffer (Ctrl+F) |
| **Save session** | Export terminal output to a text file (Ctrl+S) |
| **Battery-aware rendering** | Automatically throttles shaders and frame rate on battery power |
| **Screen casting** | Cast to Google Cast / Chromecast devices; native AirPlay, Miracast, and PipeWire support |
| **TOML configuration** | Human-readable config with profiles, keybindings, rendering, and AI settings |

---

## Architecture

Ridgeback is structured as a Rust workspace with six crates:

```
crates/
├── ridgeback-config   # Configuration types, TOML load/save, defaults
├── ridgeback-core     # Terminal engine: VT parser, PTY, grid, input buffer, search
├── ridgeback-plugin   # Plugin API traits (TerminalQuery, SaveFormatPlugin)
├── ridgeback-gpu      # wgpu multi-pass shader pipeline, glyph atlas, frame pacer
├── ridgeback-ai       # AI backend abstraction (LM Studio, OpenAI, Claude, local)
└── ridgeback-app      # eframe application: tabs, terminal view, settings, overlays
```

### Technology Stack

- **GUI chrome**: [egui](https://github.com/emilk/egui) / [eframe](https://github.com/emilk/egui/tree/master/crates/eframe) — tab bar, settings window, overlays
- **Terminal viewport**: Custom [wgpu](https://wgpu.rs/) multi-pass pipeline rendering to an offscreen texture displayed as an egui image
- **VT parsing**: [vte](https://crates.io/crates/vte) — full ANSI/VT100/xterm sequence handling
- **PTY**: [portable-pty](https://crates.io/crates/portable-pty) — Windows ConPTY, macOS/Linux PTY
- **Glyph rendering**: [fontdue](https://crates.io/crates/fontdue) rasterization + [etagere](https://crates.io/crates/etagere) atlas packing
- **Shaders**: WGSL (WebGPU Shading Language)
- **AI**: [async-openai](https://crates.io/crates/async-openai) for LM Studio / OpenAI, [reqwest](https://crates.io/crates/reqwest) for Claude
- **Config**: [toml](https://crates.io/crates/toml) + [serde](https://serde.rs/)
- **Plugins**: [mlua](https://crates.io/crates/mlua) for Lua 5.4 scripting

---

## Getting Started

### Prerequisites

- **Rust 1.75+** (install via [rustup](https://rustup.rs/))
- **Windows 10 1903+**, **macOS 12+**, or **Linux** (with a recent kernel)
- A GPU with Vulkan, DX12, or Metal support (for shader effects)

### Platform Notes

| Platform | PTY Backend | GPU Backend | Native Casting |
|---|---|---|---|
| Windows | ConPTY | DX12 / Vulkan | Miracast (Win+K) |
| macOS | POSIX PTY | Metal | AirPlay / Screen Sharing |
| Linux | POSIX PTY | Vulkan | PipeWire / OBS |

Google Cast (Chromecast) streaming is available on all platforms via the built-in Cast panel in Settings.

### Build & Run

```bash
git clone https://github.com/your-username/ridgeback.git
cd ridgeback
cargo run --release
```

### Configuration

On first launch Ridgeback creates a default config at:

| Platform | Path |
|---|---|
| Windows | `%APPDATA%\Ridgeback\config.toml` |
| macOS | `~/Library/Application Support/Ridgeback/config.toml` |
| Linux | `~/.config/ridgeback/config.toml` |

You can also open settings from within the app with **Ctrl+,**.

See [docs/profile-settings.md](docs/profile-settings.md) for a full reference of all configuration options.

---

## Keyboard Shortcuts

| Shortcut | Action |
|---|---|
| `Ctrl+T` | New terminal tab |
| `Ctrl+W` | Close current tab |
| `Ctrl+Tab` | Next tab |
| `Ctrl+Shift+Tab` | Previous tab |
| `Ctrl+S` | Save session to file |
| `Ctrl+F` | Find in session |
| `Ctrl+/` | AI command query |
| `Ctrl+,` | Open settings |
| `Ctrl+C` | Copy selection (or send SIGINT if no selection) |
| `Ctrl+V` | Paste from clipboard |
| `Ctrl+Z` | Undo (input line) |
| `Ctrl+Shift+Z` | Redo (input line) |
| `Tab` | Accept ghost-text suggestion |

All shortcuts are re-bindable in `config.toml` — see [docs/profile-settings.md](docs/profile-settings.md#keybindings).

---

## Documentation

- [Profile & Settings Reference](docs/profile-settings.md) — profiles, colors, keybindings, rendering, AI config
- [Shader Effects](docs/shaders.md) — CRT, fire, custom shaders, the multi-pass pipeline
- [Plugin System](docs/plugins.md) — Rust trait API, Lua scripting, buffer queries, export plugins
- [Casting & Screen Sharing](docs/casting.md) — Google Cast, AirPlay, Miracast, PipeWire

---

## Roadmap

- [x] Terminal engine (VT parser, PTY, grid, scrollback)
- [x] Editable input line with undo/redo, selection, clipboard
- [x] Tab management with profile support
- [x] Find in session (regex + plain text)
- [x] Save session to file
- [x] AI command query overlay (Ctrl+/)
- [x] Ghost-text autocomplete
- [x] Settings UI
- [x] Full wgpu shader pipeline (CRT, fire, bloom)
- [x] Glyph atlas with instanced text rendering
- [x] Lua plugin host runtime
- [x] LM Studio / OpenAI / Claude backend integration
- [x] WSL profile support
- [x] Sixel / image protocol support
- [x] Cross-platform (Windows, macOS, Linux)
- [x] Screen casting (Google Cast + native OS sharing)

---

## Contributing

Contributions are welcome! Please open an issue first to discuss larger changes. For code contributions:

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/my-feature`)
3. Make your changes and ensure `cargo clippy` and `cargo test` pass
4. Submit a pull request

---

## License

This project is licensed under the MIT License — see [LICENSE](LICENSE) for details.

Copyright (c) 2026 Josh van den Heever
