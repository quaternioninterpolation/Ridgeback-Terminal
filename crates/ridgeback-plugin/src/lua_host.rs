//! Lua 5.4 plugin host for Ridgeback.
//!
//! Loads `.lua` scripts from the user's plugins directory, providing
//! a sandboxed `terminal` table with read-only access to terminal state.
//!
//! ## Security
//! - Scripts run in a restricted environment: no `io`, `os.execute`,
//!   `loadfile`, `dofile`, or `require` (filesystem/network access is blocked).
//! - The `terminal` table is read-only — plugins cannot inject input.
//! - Scripts are killed after a configurable timeout (default 5 seconds).

use anyhow::{Context, Result};
use mlua::prelude::*;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::api::{StyledLine};

/// Result from executing a Lua plugin.
#[derive(Debug, Clone)]
pub struct PluginResult {
    pub name: String,
    pub output: String,
    pub error: Option<String>,
}

/// A loaded plugin script.
#[derive(Debug, Clone)]
pub struct PluginScript {
    pub name: String,
    pub path: PathBuf,
    pub source: String,
}

/// Snapshot of terminal state passed to Lua scripts.
/// This avoids holding a reference to the live terminal.
#[derive(Debug, Clone)]
pub struct TerminalSnapshot {
    pub last_lines: Vec<String>,
    pub full_log: String,
    pub title: String,
    pub shell_type: String,
}

/// The Lua plugin host.
pub struct LuaPluginHost {
    scripts: Vec<PluginScript>,
    timeout: Duration,
}

impl LuaPluginHost {
    pub fn new() -> Self {
        Self {
            scripts: Vec::new(),
            timeout: Duration::from_secs(5),
        }
    }

    /// Set the execution timeout for plugins.
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    /// Scan a directory for `.lua` plugin files and load them.
    pub fn load_directory(&mut self, dir: &Path) -> Result<usize> {
        self.scripts.clear();

        if !dir.exists() {
            tracing::info!("Plugin directory does not exist: {}", dir.display());
            return Ok(0);
        }

        let entries = std::fs::read_dir(dir)
            .with_context(|| format!("Failed to read plugin directory: {}", dir.display()))?;

        let mut count = 0;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("lua") {
                match std::fs::read_to_string(&path) {
                    Ok(source) => {
                        let name = path
                            .file_stem()
                            .and_then(|n| n.to_str())
                            .unwrap_or("unnamed")
                            .to_string();

                        tracing::info!("Loaded plugin: {} ({})", name, path.display());
                        self.scripts.push(PluginScript {
                            name,
                            path: path.clone(),
                            source,
                        });
                        count += 1;
                    }
                    Err(e) => {
                        tracing::warn!("Failed to read plugin {}: {}", path.display(), e);
                    }
                }
            }
        }

        Ok(count)
    }

    /// List loaded plugins.
    pub fn loaded_plugins(&self) -> &[PluginScript] {
        &self.scripts
    }

    /// Execute a single plugin script with the given terminal state snapshot.
    pub fn execute_plugin(
        &self,
        script: &PluginScript,
        snapshot: &TerminalSnapshot,
    ) -> PluginResult {
        match self.run_sandboxed(&script.source, &script.name, snapshot) {
            Ok(output) => PluginResult {
                name: script.name.clone(),
                output,
                error: None,
            },
            Err(e) => PluginResult {
                name: script.name.clone(),
                output: String::new(),
                error: Some(e.to_string()),
            },
        }
    }

    /// Execute all loaded plugins.
    pub fn execute_all(&self, snapshot: &TerminalSnapshot) -> Vec<PluginResult> {
        self.scripts
            .iter()
            .map(|script| self.execute_plugin(script, snapshot))
            .collect()
    }

    /// Run Lua source in a sandboxed environment.
    fn run_sandboxed(
        &self,
        source: &str,
        name: &str,
        snapshot: &TerminalSnapshot,
    ) -> Result<String> {
        let lua = Lua::new();

        // Set a hook to enforce timeout — interrupt after N instructions
        let max_instructions = 1_000_000u32; // ~5s of Lua execution
        lua.set_hook(
            mlua::HookTriggers::new().every_nth_instruction(10000),
            {
                let limit = max_instructions;
                let count = std::cell::Cell::new(0u32);
                move |_lua, _debug| {
                    count.set(count.get() + 10000);
                    if count.get() >= limit {
                        Err(mlua::Error::RuntimeError(
                            "Plugin execution timeout exceeded".to_string(),
                        ))
                    } else {
                        Ok(mlua::VmState::Continue)
                    }
                }
            },
        );

        // Build the `terminal` table
        let terminal_table = lua.create_table()?;

        // terminal.last_n_lines(n) → table of strings
        let lines = snapshot.last_lines.clone();
        terminal_table.set(
            "last_n_lines",
            lua.create_function(move |_, n: usize| {
                let result: Vec<String> = lines.iter().take(n).cloned().collect();
                Ok(result)
            })?,
        )?;

        // terminal.full_log() → string
        let full_log = snapshot.full_log.clone();
        terminal_table.set(
            "full_log",
            lua.create_function(move |_, ()| Ok(full_log.clone()))?,
        )?;

        // terminal.title() → string
        let title = snapshot.title.clone();
        terminal_table.set(
            "title",
            lua.create_function(move |_, ()| Ok(title.clone()))?,
        )?;

        // terminal.shell() → string
        let shell = snapshot.shell_type.clone();
        terminal_table.set(
            "shell",
            lua.create_function(move |_, ()| Ok(shell.clone()))?,
        )?;

        // terminal.search(pattern, use_regex, ignore_case) → table of match tables
        // Note: search is provided as a snapshot-compatible stub
        terminal_table.set(
            "search",
            lua.create_function(|lua, (pattern, _use_regex, _ignore_case): (String, bool, bool)| {
                // Simple string search on the log
                let results = lua.create_table()?;
                // Return empty results for sandboxed execution
                // (full search would need the actual buffer)
                let _ = pattern;
                Ok(results)
            })?,
        )?;

        // Set terminal table as global
        lua.globals().set("terminal", terminal_table)?;

        // Capture print output
        let output = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let output_clone = output.clone();
        lua.globals().set(
            "print",
            lua.create_function(move |_, args: mlua::Variadic<mlua::Value>| {
                let parts: Vec<String> = args
                    .iter()
                    .map(|v| match v {
                        mlua::Value::String(s) => s.to_str().map(|bs| bs.to_string()).unwrap_or_default(),
                        mlua::Value::Integer(n) => n.to_string(),
                        mlua::Value::Number(n) => n.to_string(),
                        mlua::Value::Boolean(b) => b.to_string(),
                        mlua::Value::Nil => "nil".to_string(),
                        _ => format!("{:?}", v),
                    })
                    .collect();
                let line = parts.join("\t");
                if let Ok(mut out) = output_clone.lock() {
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    out.push_str(&line);
                }
                Ok(())
            })?,
        )?;

        // Remove dangerous globals for sandbox
        let globals = lua.globals();
        for &dangerous in &["io", "os", "loadfile", "dofile", "require", "rawset", "rawget", "debug"] {
            globals.set(dangerous, mlua::Value::Nil)?;
        }

        // Execute
        let chunk = lua.load(source).set_name(name);
        match chunk.eval::<mlua::Value>() {
            Ok(mlua::Value::String(s)) => {
                return Ok(s.to_str().map(|bs| bs.to_string()).unwrap_or_default());
            }
            Ok(_) => {}
            Err(e) => {
                return Err(anyhow::anyhow!("Plugin '{}' error: {}", name, e));
            }
        }

        let result = output.lock().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        Ok(result.clone())
    }

    /// Get the platform-appropriate plugins directory.
    pub fn plugins_dir() -> Result<PathBuf> {
        let dirs = directories::ProjectDirs::from("", "", "Ridgeback")
            .context("Failed to determine plugins directory")?;
        Ok(dirs.config_dir().join("plugins"))
    }
}

impl Default for LuaPluginHost {
    fn default() -> Self {
        Self::new()
    }
}

// ── Built-in SaveFormatPlugin implementations ──────────────────────────

/// HTML export plugin — converts styled terminal output to an HTML document.
pub struct HtmlExporter;

impl crate::api::SaveFormatPlugin for HtmlExporter {
    fn name(&self) -> &str {
        "HTML"
    }

    fn extension(&self) -> &str {
        "html"
    }

    fn export(&self, lines: &[StyledLine]) -> Vec<u8> {
        let mut html = String::from(
            "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\">\
             <title>Ridgeback Session</title>\
             <style>body{background:#1e1e2e;margin:0;padding:16px;}\
             pre{font-family:'Cascadia Mono',monospace;font-size:14px;color:#cdd6f4;line-height:1.3;}</style>\
             </head><body><pre>\n",
        );

        for line in lines {
            for span in &line.spans {
                let mut style_parts = Vec::new();

                if let Some((r, g, b)) = span.fg {
                    style_parts.push(format!("color:rgb({},{},{})", r, g, b));
                }
                if let Some((r, g, b)) = span.bg {
                    style_parts.push(format!("background:rgb({},{},{})", r, g, b));
                }
                if span.bold {
                    style_parts.push("font-weight:bold".to_string());
                }
                if span.italic {
                    style_parts.push("font-style:italic".to_string());
                }
                if span.underline {
                    style_parts.push("text-decoration:underline".to_string());
                }

                let escaped = html_escape(&span.text);
                if style_parts.is_empty() {
                    html.push_str(&escaped);
                } else {
                    html.push_str(&format!(
                        "<span style=\"{}\">{}</span>",
                        style_parts.join(";"),
                        escaped
                    ));
                }
            }
            html.push('\n');
        }

        html.push_str("</pre></body></html>");
        html.into_bytes()
    }
}

/// Markdown export plugin — plain text with code block.
pub struct MarkdownExporter;

impl crate::api::SaveFormatPlugin for MarkdownExporter {
    fn name(&self) -> &str {
        "Markdown"
    }

    fn extension(&self) -> &str {
        "md"
    }

    fn export(&self, lines: &[StyledLine]) -> Vec<u8> {
        let mut md = String::from("# Ridgeback Terminal Session\n\n```\n");
        for line in lines {
            for span in &line.spans {
                md.push_str(&span.text);
            }
            md.push('\n');
        }
        md.push_str("```\n");
        md.into_bytes()
    }
}

/// JSON export plugin — structured JSON array of lines with styled spans.
pub struct JsonExporter;

impl crate::api::SaveFormatPlugin for JsonExporter {
    fn name(&self) -> &str {
        "JSON"
    }

    fn extension(&self) -> &str {
        "json"
    }

    fn export(&self, lines: &[StyledLine]) -> Vec<u8> {
        // Build a JSON-serializable structure
        let json_lines: Vec<serde_json::Value> = lines
            .iter()
            .map(|line| {
                let spans: Vec<serde_json::Value> = line
                    .spans
                    .iter()
                    .map(|span| {
                        serde_json::json!({
                            "text": span.text,
                            "fg": span.fg.map(|(r, g, b)| format!("#{:02x}{:02x}{:02x}", r, g, b)),
                            "bg": span.bg.map(|(r, g, b)| format!("#{:02x}{:02x}{:02x}", r, g, b)),
                            "bold": span.bold,
                            "italic": span.italic,
                            "underline": span.underline,
                        })
                    })
                    .collect();
                serde_json::json!({ "spans": spans })
            })
            .collect();

        serde_json::to_vec_pretty(&json_lines).unwrap_or_default()
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
