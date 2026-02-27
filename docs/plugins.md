# Plugin System

Ridgeback's plugin system lets you extend the terminal with custom functionality — from querying the buffer to exporting sessions in custom formats. Plugins can be written in **Rust** (compiled into the binary) or **Lua 5.4** (loaded at runtime from script files).

---

## Concepts

| Concept | Description |
|---|---|
| **TerminalQuery** | Read-only access to scrollback, visible buffer, and search |
| **SaveFormatPlugin** | Export terminal sessions in custom file formats |
| **Lua Host** | Runtime Lua 5.4 interpreter with access to the TerminalQuery API |

Plugins never have write access to the PTY or terminal state — they operate on a read-only snapshot of the buffer.

---

## Rust Plugin API

### TerminalQuery Trait

The core trait for querying terminal buffer content, defined in the `ridgeback-plugin` crate:

```rust
pub trait TerminalQuery {
    /// Get the last N lines from scrollback (most recent first).
    fn last_n_lines(&self, n: usize) -> Vec<StyledLine>;

    /// Get the full terminal log (all scrollback + visible lines).
    fn full_log(&self) -> Vec<StyledLine>;

    /// Search the buffer with the given pattern.
    fn search(&self, pattern: &str, use_regex: bool, ignore_case: bool) -> Vec<SearchMatch>;
}
```

Each `StyledLine` contains a vector of `StyledSpan` structs:

```rust
pub struct StyledSpan {
    pub text: String,
    pub fg: Option<[u8; 3]>,
    pub bg: Option<[u8; 3]>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}
```

### SaveFormatPlugin Trait

Implement this to add custom export formats to the Save Session dialog:

```rust
pub trait SaveFormatPlugin {
    /// Display name shown in the file-type dropdown (e.g. "Rich Text").
    fn name(&self) -> &str;

    /// File extension without the dot (e.g. "rtf").
    fn extension(&self) -> &str;

    /// Convert styled terminal lines into the target format as bytes.
    fn export(&self, lines: &[StyledLine]) -> anyhow::Result<Vec<u8>>;
}
```

### Registering a Rust Plugin

Rust plugins are registered at compile time in `ridgeback-app`:

```rust
use ridgeback_plugin::{SaveFormatPlugin, StyledLine};

struct HtmlExporter;

impl SaveFormatPlugin for HtmlExporter {
    fn name(&self) -> &str { "HTML" }
    fn extension(&self) -> &str { "html" }
    fn export(&self, lines: &[StyledLine]) -> anyhow::Result<Vec<u8>> {
        let mut html = String::from("<pre style='background:#1e1e2e;color:#cdd6f4;'>\n");
        for line in lines {
            for span in &line.spans {
                html.push_str(&html_escape(&span.text));
            }
            html.push('\n');
        }
        html.push_str("</pre>");
        Ok(html.into_bytes())
    }
}
```

---

## Lua Plugin API

Lua plugins are `.lua` files placed in the plugins directory:

```
%APPDATA%\Ridgeback\plugins\
```

Ridgeback scans this directory on startup and loads all `.lua` files. Each script receives a global `terminal` table with the query API.

### Available Functions

| Lua Function | Description |
|---|---|
| `terminal.last_n_lines(n)` | Returns an array of the last `n` lines as strings |
| `terminal.full_log()` | Returns all scrollback + visible lines as one string |
| `terminal.search(pattern, use_regex, ignore_case)` | Returns an array of match tables `{line, col, len, text}` |
| `terminal.title()` | Returns the current terminal title |
| `terminal.cwd()` | Returns the current working directory (if available) |
| `terminal.shell()` | Returns the shell type (`"powershell"`, `"cmd"`, `"wsl"`) |

### Example: Log Watcher

```lua
-- plugins/log_watcher.lua
-- Highlights ERROR lines in the last 100 lines of output

local lines = terminal.last_n_lines(100)
local errors = {}

for i, line in ipairs(lines) do
    if line:find("ERROR") or line:find("FATAL") then
        table.insert(errors, { line_number = i, text = line })
    end
end

if #errors > 0 then
    print("Found " .. #errors .. " error(s):")
    for _, err in ipairs(errors) do
        print("  Line " .. err.line_number .. ": " .. err.text)
    end
end
```

### Example: Export to JSON

```lua
-- plugins/json_export.lua
-- Exports the full session log as a JSON array

local log = terminal.full_log()
local lines = {}
for line in log:gmatch("[^\n]+") do
    -- Escape quotes for JSON
    local escaped = line:gsub('"', '\\"')
    table.insert(lines, '"' .. escaped .. '"')
end

local json = "[\n  " .. table.concat(lines, ",\n  ") .. "\n]"
return json
```

---

## Plugin Security

- Plugins run in a **sandboxed Lua environment** — they cannot access the filesystem, network, or OS APIs unless explicitly granted by the user.
- The `terminal` table is read-only — plugins cannot inject input or modify the buffer.
- Plugins are executed asynchronously on a background thread to avoid blocking the UI.
- Long-running plugins are killed after a configurable timeout (default: 5 seconds).

---

## Creating Your Own Plugin

1. Create a `.lua` file in `%APPDATA%\Ridgeback\plugins\`
2. Use the `terminal.*` API to query buffer content
3. Return a result string or print output to the plugin console
4. Restart Ridgeback (or reload plugins via Settings > Plugins)

For Rust plugins, add your implementation to the `ridgeback-plugin` crate and register it in `ridgeback-app`.
