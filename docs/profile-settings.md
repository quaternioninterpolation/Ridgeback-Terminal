# Profile & Settings Reference

Ridgeback is configured through a single TOML file. The location is platform-dependent:

| Platform | Config Path |
|---|---|
| Windows | `%APPDATA%\Ridgeback\config.toml` |
| macOS | `~/Library/Application Support/Ridgeback/config.toml` |
| Linux | `~/.config/ridgeback/config.toml` |

You can also edit settings visually via **Cmd+,** (macOS) or **Ctrl+,** (Windows/Linux) inside the app.

---

## Table of Contents

- [General](#general)
- [Profiles](#profiles)
  - [Shell Configuration](#shell-configuration)
  - [Cursor](#cursor)
  - [Color Scheme](#color-scheme)
  - [Shader Effect](#shader-effect)
- [Keybindings](#keybindings)
- [Rendering](#rendering)
- [AI](#ai)
  - [Autocomplete](#autocomplete)
  - [Command Query](#command-query)
  - [Backends](#backends)

---

## General

```toml
[general]
default_profile = "powershell"        # Key of the profile to open on launch
tab_bar_position = "top"              # "top" or "bottom"
confirm_close_with_multiple_tabs = true

[general.font]
family = "Cascadia Mono"              # Font family for the terminal viewport
size = 14.0                           # Font size in points
bold_is_bright = true                 # Render bold text with bright ANSI colors
```

---

## Profiles

Profiles define the shell, appearance, and behavior of each terminal tab. You can have multiple profiles and switch between them when opening a new tab.

Default profiles are created based on your platform:

| Platform | Default Profiles |
|---|---|
| Windows | PowerShell, Command Prompt |
| macOS | Zsh, Bash |
| Linux | Bash, Zsh, Fish |

```toml
[profiles.powershell]
name = "PowerShell"
shell = "pwsh.exe"                     # Windows; use "pwsh" on macOS/Linux
args = ["-NoLogo"]
working_directory = "~"
shell_type = "powershell"             # powershell | cmd | wsl | bash | zsh | fish | custom
scrollback_limit = 10000
```

### Shell Configuration

| Field | Type | Default | Description |
|---|---|---|---|
| `name` | string | *(platform)* | Display name shown in the tab and profile picker |
| `shell` | string | *(platform)* | Path or name of the shell executable |
| `args` | string[] | *(varies)* | Arguments passed to the shell on launch |
| `working_directory` | path | `"~"` | Starting directory (~ expands to the user home) |
| `shell_type` | enum | *(platform)* | Shell type hint: `powershell`, `cmd`, `wsl`, `bash`, `zsh`, `fish`, `custom` |
| `scrollback_limit` | integer | `10000` | Maximum number of lines kept in the scrollback buffer |

### Cursor

```toml
[profiles.powershell]
cursor_style = "bar"                  # block | bar | underline
cursor_blink = true
cursor_blink_ms = 530                # Blink interval in milliseconds
```

| Field | Type | Default | Description |
|---|---|---|---|
| `cursor_style` | enum | `"bar"` | Visual shape of the cursor |
| `cursor_blink` | bool | `true` | Whether the cursor blinks |
| `cursor_blink_ms` | integer | `530` | Blink period in milliseconds |

### Color Scheme

The color scheme uses CSS-style hex strings. The default theme is **Catppuccin Mocha**.

```toml
[profiles.powershell.colors]
background = "#1E1E2E"
foreground = "#CDD6F4"
cursor = "#F5E0DC"
selection_bg = "#585B70"
selection_fg = "#CDD6F4"

# ANSI 16-color palette
black = "#45475A"
red = "#F38BA8"
green = "#A6E3A1"
yellow = "#F9E2AF"
blue = "#89B4FA"
magenta = "#F5C2E7"
cyan = "#94E2D5"
white = "#BAC2DE"
bright_black = "#585B70"
bright_red = "#F38BA8"
bright_green = "#A6E3A1"
bright_yellow = "#F9E2AF"
bright_blue = "#89B4FA"
bright_magenta = "#F5C2E7"
bright_cyan = "#94E2D5"
bright_white = "#A6ADC8"
```

All 16 ANSI colors are individually configurable. The 256-color and true-color palettes are handled automatically by the renderer.

### Shader Effect

Each profile can have an independent shader effect applied to its terminal viewport.

```toml
[profiles.powershell]
shader_effect = "none"                # none | crt | fire
```

Shader-specific parameters are set in the `shader_params` sub-table:

```toml
[profiles.powershell.shader_params]
# CRT parameters
scanline_intensity = 0.15
curvature = 0.03
bloom_strength = 0.3
chromatic_aberration = 0.002

# Fire parameters
fire_intensity = 0.6
fire_decay_rate = 0.95
fire_spread = 1.0
```

Only the parameters matching the active `shader_effect` have any effect. See [docs/shaders.md](shaders.md) for full details.

---

## Keybindings

All keyboard shortcuts are customizable. Use modifier names (`Ctrl`, `Cmd`, `Shift`, `Alt`) joined with `+` and ending with the key name. Both `Ctrl` and `Cmd` map to the platform command key — **Cmd (⌘)** on macOS, **Ctrl** on Windows/Linux.

```toml
[keybindings]
# macOS defaults use Cmd; Windows/Linux defaults use Ctrl
new_tab = "Cmd+T"              # or "Ctrl+T" on Windows/Linux
close_tab = "Cmd+W"
next_tab = "Cmd+Tab"
prev_tab = "Cmd+Shift+Tab"
open_settings = "Cmd+,"
save_session = "Cmd+S"
find_in_session = "Cmd+F"
ai_command_query = "Cmd+/"
split_horizontal = "Cmd+Shift+D"
split_vertical = "Cmd+Shift+E"
close_pane = "Cmd+Shift+W"
reload_plugins = "Cmd+Shift+P"
focus_next_group = "Cmd+Alt+Right"
focus_prev_group = "Cmd+Alt+Left"
move_tab_to_next_group = "Cmd+Alt+Shift+Right"
move_tab_to_prev_group = "Cmd+Alt+Shift+Left"
```

| Action | Default (macOS / Other) | Description |
|---|---|---|
| `new_tab` | `Cmd+T` / `Ctrl+T` | Open a new tab in the focused group |
| `close_tab` | `Cmd+W` / `Ctrl+W` | Close the active tab |
| `next_tab` | `Cmd+Tab` / `Ctrl+Tab` | Switch to the next tab within the group |
| `prev_tab` | `Cmd+Shift+Tab` / `Ctrl+Shift+Tab` | Switch to the previous tab within the group |
| `open_settings` | `Cmd+,` / `Ctrl+,` | Toggle the settings window |
| `save_session` | `Cmd+S` / `Ctrl+S` | Save the current session's output to a file |
| `find_in_session` | `Cmd+F` / `Ctrl+F` | Open the find-in-session overlay |
| `ai_command_query` | `Cmd+/` / `Ctrl+/` | Open the AI command query overlay |
| `split_horizontal` | `Cmd+Shift+D` / `Ctrl+Shift+D` | Split right — create a new tab group beside the current one |
| `split_vertical` | `Cmd+Shift+E` / `Ctrl+Shift+E` | Split down — create a new tab group below the current one |
| `close_pane` | `Cmd+Shift+W` / `Ctrl+Shift+W` | Close the entire focused tab group |
| `reload_plugins` | `Cmd+Shift+P` / `Ctrl+Shift+P` | Reload all Lua plugins without restarting |
| `focus_next_group` | `Cmd+Alt+Right` / `Ctrl+Alt+Right` | Move focus to the next tab group |
| `focus_prev_group` | `Cmd+Alt+Left` / `Ctrl+Alt+Left` | Move focus to the previous tab group |
| `move_tab_to_next_group` | `Cmd+Alt+Shift+Right` / `Ctrl+Alt+Shift+Right` | Move the active tab to the next group |
| `move_tab_to_prev_group` | `Cmd+Alt+Shift+Left` / `Ctrl+Alt+Shift+Left` | Move the active tab to the previous group |

### Key Names

Supported key names: `A`–`Z`, `0`–`9`, `Tab`, `Enter`, `Escape`, `Backspace`, `Delete`, `Home`, `End`, `PageUp`, `PageDown`, `Up`, `Down`, `Left`, `Right`, `Space`, `/`, `,`, `.`

Modifier names: `Ctrl`, `Cmd`, `Shift`, `Alt` (`Ctrl` and `Cmd` are interchangeable — both map to the platform command key)

Built-in shortcuts (not rebindable): `Cmd+C` / `Ctrl+C` (copy/interrupt), `Cmd+V` / `Ctrl+V` (paste), `Cmd+Z` / `Ctrl+Z` (undo), `Cmd+Shift+Z` / `Ctrl+Shift+Z` (redo), `Cmd+A` / `Ctrl+A` (select all in input), `Tab` (accept ghost text).

---

## Rendering

```toml
[rendering]
update_in_background = true           # Continue rendering when the window is unfocused
max_shader_fps = 144                   # Shader/animation frame rate cap (1–240)
battery_aware = true                   # Throttle on battery power
```

| Field | Type | Default | Range | Description |
|---|---|---|---|---|
| `update_in_background` | bool | `true` | — | If `false`, the terminal freezes rendering when the window loses focus |
| `max_shader_fps` | integer | `144` | 1–240 | Maximum frames per second for shader animation. Text/scroll updates are not capped. |
| `battery_aware` | bool | `true` | — | When on battery, reduces shader FPS to 30 and simplifies effects |

---

## AI

```toml
[ai]
enabled = true
default_backend = "lm_studio"         # lm_studio | openai | claude | local
```

### Autocomplete

Ghost-text suggestions appear as dimmed text after the cursor. Press **Tab** to accept, or keep typing to dismiss.

```toml
[ai.autocomplete]
enabled = true
debounce_ms = 250                     # Wait before sending a request after typing stops
max_tokens = 64                       # Maximum tokens in the completion response
temperature = 0.2                     # Sampling temperature (lower = more deterministic)
context_lines = 10                    # Number of recent terminal output lines sent as context
```

### Command Query

Press **Cmd+/** (macOS) or **Ctrl+/** (Windows/Linux) to open the command query overlay. Type a natural-language description of what you want to do, and the AI returns one or more suggested commands.

```toml
[ai.command_query]
enabled = true
max_suggestions = 3                   # Number of command suggestions returned
max_tokens = 256                      # Maximum tokens in the response
temperature = 0.4                     # Sampling temperature
```

### Backends

#### LM Studio (default)

```toml
[ai.backends.lm_studio]
base_url = "http://localhost:1234/v1"
api_key = "lm-studio"
model = "default"
timeout_secs = 30
```

Point `base_url` at your running LM Studio server. The `model` field can be `"default"` to use whatever model is loaded, or a specific model name.

#### OpenAI

```toml
[ai.backends.openai]
api_key = ""                          # Your OpenAI API key
model = "gpt-4o-mini"
timeout_secs = 30
```

#### Claude (Anthropic)

```toml
[ai.backends.claude]
api_key = ""                          # Your Anthropic API key
model = "claude-sonnet-4-20250514"
max_tokens = 1024
```

#### Local Model

```toml
[ai.backends.local]
model_repo = "microsoft/Phi-3-mini-4k-instruct"
quantization = "Q4_K_M"
device = "auto"                       # auto | cpu | cuda
context_length = 4096
```

Local models are loaded via the [mistral.rs](https://github.com/EricLBuehler/mistral.rs) engine (future feature).

---

## Full Example Config

```toml
[general]
default_profile = "powershell"
tab_bar_position = "top"
confirm_close_with_multiple_tabs = true

[general.font]
family = "Cascadia Mono"
size = 14.0
bold_is_bright = true

[rendering]
update_in_background = true
max_shader_fps = 144
battery_aware = true

[keybindings]
# Use "Cmd" on macOS, "Ctrl" on Windows/Linux (both are accepted)
new_tab = "Cmd+T"
close_tab = "Cmd+W"
next_tab = "Cmd+Tab"
prev_tab = "Cmd+Shift+Tab"
open_settings = "Cmd+,"
save_session = "Cmd+S"
find_in_session = "Cmd+F"
ai_command_query = "Cmd+/"
split_horizontal = "Cmd+Shift+D"
split_vertical = "Cmd+Shift+E"
close_pane = "Cmd+Shift+W"
reload_plugins = "Cmd+Shift+P"
focus_next_group = "Cmd+Alt+Right"
focus_prev_group = "Cmd+Alt+Left"
move_tab_to_next_group = "Cmd+Alt+Shift+Right"
move_tab_to_prev_group = "Cmd+Alt+Shift+Left"

[ai]
enabled = true
default_backend = "lm_studio"

[ai.autocomplete]
enabled = true
debounce_ms = 250
temperature = 0.2

[ai.command_query]
enabled = true
max_suggestions = 3
temperature = 0.4

[ai.backends.lm_studio]
base_url = "http://localhost:1234/v1"
model = "default"

[profiles.powershell]
name = "PowerShell"
shell = "pwsh.exe"
args = ["-NoLogo"]
shell_type = "powershell"
scrollback_limit = 10000
cursor_style = "bar"
cursor_blink = true
shader_effect = "crt"

[profiles.powershell.shader_params]
scanline_intensity = 0.15
curvature = 0.03

[profiles.powershell.colors]
background = "#1E1E2E"
foreground = "#CDD6F4"
cursor = "#F5E0DC"

[profiles.cmd]
name = "Command Prompt"
shell = "cmd.exe"
shell_type = "cmd"
shader_effect = "none"
```
