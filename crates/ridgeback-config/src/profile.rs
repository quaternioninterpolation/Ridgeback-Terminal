use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

// ── Shader plugin config ────────────────────────────────────────────────────

/// Selects a shader effect plugin by ID, with free-form key-value parameters.
///
/// Built-in plugin IDs: `"none"`, `"fire"`, `"crt"`.
/// Users can register additional IDs via `.lua` plugin files.
///
/// **Backward-compatible deserialization**: accepts both the old plain string
/// format (`shader_effect = "none"`) and the new table format
/// (`shader_effect = { plugin_id = "fire", params = { ... } }`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ShaderEffectConfig {
    /// Plugin ID, e.g. `"fire"`, `"crt"`, `"none"`, or a user-defined ID.
    pub plugin_id: String,
    /// Path to a custom `.wgsl` file (overrides the plugin's default shader).
    /// Empty string means "use the plugin's built-in shader".
    #[serde(default)]
    pub wgsl_override: String,
    /// Freeform parameters forwarded to the shader and to Lua `on_frame()`.
    #[serde(default)]
    pub params: HashMap<String, serde_json::Value>,
}

impl<'de> Deserialize<'de> for ShaderEffectConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::{self, MapAccess, Visitor};

        struct ShaderEffectVisitor;

        impl<'de> Visitor<'de> for ShaderEffectVisitor {
            type Value = ShaderEffectConfig;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a shader effect string (\"none\", \"fire\", \"crt\") or a table { plugin_id, ... }")
            }

            // Old format: shader_effect = "none"
            fn visit_str<E: de::Error>(self, v: &str) -> Result<ShaderEffectConfig, E> {
                Ok(string_to_shader_effect_config(v))
            }

            // New format: shader_effect = { plugin_id = "fire", params = { ... } }
            fn visit_map<M: MapAccess<'de>>(self, map: M) -> Result<ShaderEffectConfig, M::Error> {
                #[derive(Deserialize)]
                #[serde(default)]
                struct Inner {
                    plugin_id: String,
                    wgsl_override: String,
                    params: HashMap<String, serde_json::Value>,
                }
                impl Default for Inner {
                    fn default() -> Self {
                        Self {
                            plugin_id: "none".to_string(),
                            wgsl_override: String::new(),
                            params: HashMap::new(),
                        }
                    }
                }
                let inner = Inner::deserialize(de::value::MapAccessDeserializer::new(map))?;
                Ok(ShaderEffectConfig {
                    plugin_id: inner.plugin_id,
                    wgsl_override: inner.wgsl_override,
                    params: inner.params,
                })
            }
        }

        deserializer.deserialize_any(ShaderEffectVisitor)
    }
}

/// Convert a legacy string value to a `ShaderEffectConfig` with sensible defaults.
fn string_to_shader_effect_config(s: &str) -> ShaderEffectConfig {
    match s {
        "fire" => {
            let mut cfg = ShaderEffectConfig::builtin("fire");
            cfg.params.insert("intensity".into(), serde_json::json!(1.0));
            cfg.params.insert("height".into(), serde_json::json!(0.25));
            cfg.params.insert("particle_multiplier".into(), serde_json::json!(1.0));
            cfg.params.insert("color_base".into(), serde_json::json!("#1a0000"));
            cfg.params.insert("color_mid".into(), serde_json::json!("#ff4400"));
            cfg.params.insert("color_top".into(), serde_json::json!("#ffdd00"));
            cfg
        }
        "crt" => {
            let mut cfg = ShaderEffectConfig::builtin("crt");
            cfg.params.insert("scanline_intensity".into(), serde_json::json!(0.3));
            cfg.params.insert("curvature".into(), serde_json::json!(0.1));
            cfg.params.insert("bloom_strength".into(), serde_json::json!(0.15));
            cfg
        }
        _ => ShaderEffectConfig::builtin("none"),
    }
}

impl Default for ShaderEffectConfig {
    fn default() -> Self {
        Self {
            plugin_id: "none".to_string(),
            wgsl_override: String::new(),
            params: HashMap::new(),
        }
    }
}

impl ShaderEffectConfig {
    /// Convenience constructor: select a built-in by name with empty params.
    pub fn builtin(id: &str) -> Self {
        Self { plugin_id: id.to_string(), ..Default::default() }
    }

    /// Get a param as f32, falling back to `default`.
    pub fn param_f32(&self, key: &str, default: f32) -> f32 {
        self.params.get(key)
            .and_then(|v| v.as_f64())
            .map(|f| f as f32)
            .unwrap_or(default)
    }

    /// Get a param as a "#RRGGBB" string, returning None if missing.
    pub fn param_color(&self, key: &str) -> Option<&str> {
        self.params.get(key)?.as_str()
    }

    pub fn param_bool(&self, key: &str, default: bool) -> bool {
        self.params.get(key)
            .and_then(|v| v.as_bool())
            .unwrap_or(default)
    }
}

/// Selects a typing-particle plugin by ID with free-form parameters.
///
/// Backward-compatible: accepts `"none"` / `"fire"` strings or a full table.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TypingParticlesConfig {
    /// Plugin ID (`"none"` to disable, `"fire"` for built-in fire particles,
    /// or any user-registered ID).
    pub plugin_id: String,
    /// Freeform parameters forwarded to the Lua `on_keypress()` hook.
    #[serde(default)]
    pub params: HashMap<String, serde_json::Value>,
}

impl<'de> Deserialize<'de> for TypingParticlesConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::{self, MapAccess, Visitor};

        struct ParticleVisitor;

        impl<'de> Visitor<'de> for ParticleVisitor {
            type Value = TypingParticlesConfig;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a particle plugin string (\"none\", \"fire\") or a table { plugin_id, ... }")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<TypingParticlesConfig, E> {
                Ok(TypingParticlesConfig {
                    plugin_id: v.to_string(),
                    params: HashMap::new(),
                })
            }

            fn visit_map<M: MapAccess<'de>>(self, map: M) -> Result<TypingParticlesConfig, M::Error> {
                #[derive(Deserialize)]
                #[serde(default)]
                struct Inner {
                    plugin_id: String,
                    params: HashMap<String, serde_json::Value>,
                }
                impl Default for Inner {
                    fn default() -> Self {
                        Self { plugin_id: "none".to_string(), params: HashMap::new() }
                    }
                }
                let inner = Inner::deserialize(de::value::MapAccessDeserializer::new(map))?;
                Ok(TypingParticlesConfig {
                    plugin_id: inner.plugin_id,
                    params: inner.params,
                })
            }
        }

        deserializer.deserialize_any(ParticleVisitor)
    }
}

impl Default for TypingParticlesConfig {
    fn default() -> Self {
        Self { plugin_id: "none".to_string(), params: HashMap::new() }
    }
}

impl TypingParticlesConfig {
    pub fn builtin(id: &str) -> Self {
        Self { plugin_id: id.to_string(), ..Default::default() }
    }
    pub fn param_f32(&self, key: &str, default: f32) -> f32 {
        self.params.get(key)
            .and_then(|v| v.as_f64())
            .map(|f| f as f32)
            .unwrap_or(default)
    }
}

// ── Legacy types (kept for backward-compat with old config files) ───────────

/// Legacy shader effect discriminant. Old TOML configs that stored `shader_effect = "fire"`
/// are migrated on load via `Profile`'s `#[serde(default)]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShaderEffect {
    None,
    Crt,
    Fire,
}

impl ShaderEffect {
    pub fn to_config(self) -> ShaderEffectConfig {
        match self {
            ShaderEffect::None => ShaderEffectConfig::builtin("none"),
            ShaderEffect::Crt => {
                let mut cfg = ShaderEffectConfig::builtin("crt");
                cfg.params.insert("scanline_intensity".into(), serde_json::json!(0.3));
                cfg.params.insert("curvature".into(), serde_json::json!(0.1));
                cfg.params.insert("bloom_strength".into(), serde_json::json!(0.15));
                cfg.params.insert("chromatic_aberration".into(), serde_json::json!(0.003));
                cfg
            }
            ShaderEffect::Fire => {
                let mut cfg = ShaderEffectConfig::builtin("fire");
                cfg.params.insert("intensity".into(), serde_json::json!(1.0));
                cfg.params.insert("decay_rate".into(), serde_json::json!(0.03));
                cfg.params.insert("spread".into(), serde_json::json!(0.5));
                cfg.params.insert("height".into(), serde_json::json!(0.25));
                cfg.params.insert("particle_multiplier".into(), serde_json::json!(1.0));
                cfg.params.insert("color_base".into(), serde_json::json!("#1a0000"));
                cfg.params.insert("color_mid".into(), serde_json::json!("#ff4400"));
                cfg.params.insert("color_top".into(), serde_json::json!("#ffdd00"));
                cfg
            }
        }
    }
}

/// Legacy flat parameter block. Kept so that old Rust code that references
/// `ShaderParams` still compiles; new code should use `ShaderEffectConfig::params`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShaderParams {
    // CRT params
    pub scanline_intensity: f32,
    pub curvature: f32,
    pub bloom_strength: f32,
    pub chromatic_aberration: f32,
    // Fire params
    pub fire_intensity: f32,
    pub fire_decay_rate: f32,
    pub fire_spread: f32,
}

impl Default for ShaderParams {
    fn default() -> Self {
        Self {
            scanline_intensity: 0.3,
            curvature: 0.1,
            bloom_strength: 0.15,
            chromatic_aberration: 0.003,
            fire_intensity: 1.0,
            fire_decay_rate: 0.03,
            fire_spread: 0.5,
        }
    }
}

// ── Terminal padding ────────────────────────────────────────────────────────

/// Terminal viewport padding as a percentage of screen width/height.
/// Each value is 0.0–25.0 representing a percentage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TerminalPadding {
    pub top: f32,
    pub bottom: f32,
    pub left: f32,
    pub right: f32,
}

impl Default for TerminalPadding {
    fn default() -> Self {
        Self { top: 2.5, bottom: 2.5, left: 2.5, right: 2.5 }
    }
}

// ── Profile ─────────────────────────────────────────────────────────────────

/// A terminal profile defining shell, appearance, and shader settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub name: String,
    pub shell: String,
    pub args: Vec<String>,
    pub working_directory: PathBuf,
    pub shell_type: ShellType,
    pub scrollback_limit: usize,
    pub cursor_style: CursorStyle,
    pub cursor_blink: bool,
    pub cursor_blink_ms: u32,
    pub colors: ColorScheme,
    /// New plugin-driven shader effect config.
    pub shader_effect: ShaderEffectConfig,
    /// New plugin-driven typing particles config.
    pub typing_particles: TypingParticlesConfig,
    /// Default text foreground colour as "#RRGGBB". Overrides the colour scheme foreground.
    pub text_foreground: String,
    /// Draw a dark shadow behind text to keep it readable over bright backgrounds.
    pub text_shadow_enabled: bool,
    /// Shadow darkness: 0.0 = invisible, 1.0 = fully black.
    pub text_shadow_alpha: f32,
    /// Terminal viewport padding (percentage of width/height).
    pub padding: TerminalPadding,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShellType {
    Powershell,
    Cmd,
    Wsl,
    Bash,
    Zsh,
    Fish,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CursorStyle {
    Block,
    Bar,
    Underline,
}


/// Terminal color scheme with ANSI 16-color palette.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ColorScheme {
    pub background: String,
    pub foreground: String,
    pub cursor: String,
    pub selection_bg: String,
    pub selection_fg: String,
    // ANSI colors 0-15
    pub black: String,
    pub red: String,
    pub green: String,
    pub yellow: String,
    pub blue: String,
    pub magenta: String,
    pub cyan: String,
    pub white: String,
    pub bright_black: String,
    pub bright_red: String,
    pub bright_green: String,
    pub bright_yellow: String,
    pub bright_blue: String,
    pub bright_magenta: String,
    pub bright_cyan: String,
    pub bright_white: String,
}

impl Default for Profile {
    fn default() -> Self {
        Self::platform_default()
    }
}

impl Profile {
    /// Returns the best default profile for the current platform.
    pub fn platform_default() -> Self {
        #[cfg(target_os = "windows")]
        { Self::default_powershell() }
        #[cfg(target_os = "macos")]
        { Self::default_zsh() }
        #[cfg(target_os = "linux")]
        { Self::default_bash() }
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        { Self::default_bash() }
    }

    pub fn default_powershell() -> Self {
        Self {
            name: "PowerShell".to_string(),
            #[cfg(target_os = "windows")]
            shell: find_pwsh_windows(),
            #[cfg(not(target_os = "windows"))]
            shell: "pwsh".to_string(),
            args: vec!["-NoLogo".to_string()],
            working_directory: dirs_home(),
            shell_type: ShellType::Powershell,
            scrollback_limit: 10_000,
            cursor_style: CursorStyle::Bar,
            cursor_blink: true,
            cursor_blink_ms: 530,
            colors: ColorScheme::catppuccin_mocha(),
            shader_effect: ShaderEffectConfig::default(),
            typing_particles: TypingParticlesConfig::default(),
            text_foreground: "#CDD6F4".to_string(),
            text_shadow_enabled: true,
            text_shadow_alpha: 0.65,
            padding: TerminalPadding::default(),
        }
    }

    pub fn default_cmd() -> Self {
        Self {
            name: "Command Prompt".to_string(),
            shell: "cmd.exe".to_string(),
            args: vec![],
            working_directory: dirs_home(),
            shell_type: ShellType::Cmd,
            scrollback_limit: 10_000,
            cursor_style: CursorStyle::Block,
            cursor_blink: true,
            cursor_blink_ms: 530,
            colors: ColorScheme::default_dark(),
            shader_effect: ShaderEffectConfig::default(),
            typing_particles: TypingParticlesConfig::default(),
            text_foreground: "#CCCCCC".to_string(),
            text_shadow_enabled: true,
            text_shadow_alpha: 0.65,
            padding: TerminalPadding::default(),
        }
    }

    pub fn default_bash() -> Self {
        Self {
            name: "Bash".to_string(),
            shell: "/bin/bash".to_string(),
            args: vec!["--login".to_string()],
            working_directory: dirs_home(),
            shell_type: ShellType::Bash,
            scrollback_limit: 10_000,
            cursor_style: CursorStyle::Bar,
            cursor_blink: true,
            cursor_blink_ms: 530,
            colors: ColorScheme::catppuccin_mocha(),
            shader_effect: ShaderEffectConfig::default(),
            typing_particles: TypingParticlesConfig::default(),
            text_foreground: "#CDD6F4".to_string(),
            text_shadow_enabled: true,
            text_shadow_alpha: 0.65,
            padding: TerminalPadding::default(),
        }
    }

    pub fn default_zsh() -> Self {
        Self {
            name: "Zsh".to_string(),
            shell: "/bin/zsh".to_string(),
            args: vec!["--login".to_string()],
            working_directory: dirs_home(),
            shell_type: ShellType::Zsh,
            scrollback_limit: 10_000,
            cursor_style: CursorStyle::Bar,
            cursor_blink: true,
            cursor_blink_ms: 530,
            colors: ColorScheme::catppuccin_mocha(),
            shader_effect: ShaderEffectConfig::default(),
            typing_particles: TypingParticlesConfig::default(),
            text_foreground: "#CDD6F4".to_string(),
            text_shadow_enabled: true,
            text_shadow_alpha: 0.65,
            padding: TerminalPadding::default(),
        }
    }

    pub fn default_fish() -> Self {
        Self {
            name: "Fish".to_string(),
            shell: "/usr/bin/fish".to_string(),
            args: vec!["--login".to_string()],
            working_directory: dirs_home(),
            shell_type: ShellType::Fish,
            scrollback_limit: 10_000,
            cursor_style: CursorStyle::Bar,
            cursor_blink: true,
            cursor_blink_ms: 530,
            colors: ColorScheme::catppuccin_mocha(),
            shader_effect: ShaderEffectConfig::default(),
            typing_particles: TypingParticlesConfig::default(),
            text_foreground: "#CDD6F4".to_string(),
            text_shadow_enabled: true,
            text_shadow_alpha: 0.65,
            padding: TerminalPadding::default(),
        }
    }

    pub fn default_wsl() -> Self {
        Self {
            name: "WSL".to_string(),
            shell: "wsl.exe".to_string(),
            args: vec![],
            working_directory: dirs_home(),
            shell_type: ShellType::Wsl,
            scrollback_limit: 10_000,
            cursor_style: CursorStyle::Bar,
            cursor_blink: true,
            cursor_blink_ms: 530,
            colors: ColorScheme::catppuccin_mocha(),
            shader_effect: ShaderEffectConfig::default(),
            typing_particles: TypingParticlesConfig::default(),
            text_foreground: "#CDD6F4".to_string(),
            text_shadow_enabled: true,
            text_shadow_alpha: 0.65,
            padding: TerminalPadding::default(),
        }
    }

    pub fn wsl_distro(distro_name: &str) -> Self {
        Self {
            name: format!("WSL ({})", distro_name),
            shell: "wsl.exe".to_string(),
            args: vec!["-d".to_string(), distro_name.to_string()],
            working_directory: dirs_home(),
            shell_type: ShellType::Wsl,
            scrollback_limit: 10_000,
            cursor_style: CursorStyle::Bar,
            cursor_blink: true,
            cursor_blink_ms: 530,
            colors: ColorScheme::catppuccin_mocha(),
            shader_effect: ShaderEffectConfig::default(),
            typing_particles: TypingParticlesConfig::default(),
            text_foreground: "#CDD6F4".to_string(),
            text_shadow_enabled: true,
            text_shadow_alpha: 0.65,
            padding: TerminalPadding::default(),
        }
    }
}


impl ColorScheme {
    pub fn catppuccin_mocha() -> Self {
        Self {
            background: "#1E1E2E".to_string(),
            foreground: "#CDD6F4".to_string(),
            cursor: "#F5E0DC".to_string(),
            selection_bg: "#585B70".to_string(),
            selection_fg: "#CDD6F4".to_string(),
            black: "#45475A".to_string(),
            red: "#F38BA8".to_string(),
            green: "#A6E3A1".to_string(),
            yellow: "#F9E2AF".to_string(),
            blue: "#89B4FA".to_string(),
            magenta: "#F5C2E7".to_string(),
            cyan: "#94E2D5".to_string(),
            white: "#BAC2DE".to_string(),
            bright_black: "#585B70".to_string(),
            bright_red: "#F38BA8".to_string(),
            bright_green: "#A6E3A1".to_string(),
            bright_yellow: "#F9E2AF".to_string(),
            bright_blue: "#89B4FA".to_string(),
            bright_magenta: "#F5C2E7".to_string(),
            bright_cyan: "#94E2D5".to_string(),
            bright_white: "#A6ADC8".to_string(),
        }
    }

    pub fn default_dark() -> Self {
        Self {
            background: "#0C0C0C".to_string(),
            foreground: "#CCCCCC".to_string(),
            cursor: "#FFFFFF".to_string(),
            selection_bg: "#264F78".to_string(),
            selection_fg: "#FFFFFF".to_string(),
            black: "#0C0C0C".to_string(),
            red: "#C50F1F".to_string(),
            green: "#13A10E".to_string(),
            yellow: "#C19C00".to_string(),
            blue: "#0037DA".to_string(),
            magenta: "#881798".to_string(),
            cyan: "#3A96DD".to_string(),
            white: "#CCCCCC".to_string(),
            bright_black: "#767676".to_string(),
            bright_red: "#E74856".to_string(),
            bright_green: "#16C60C".to_string(),
            bright_yellow: "#F9F1A5".to_string(),
            bright_blue: "#3B78FF".to_string(),
            bright_magenta: "#B4009E".to_string(),
            bright_cyan: "#61D6D6".to_string(),
            bright_white: "#F2F2F2".to_string(),
        }
    }

    /// Parse a hex color string like "#RRGGBB" into (r, g, b) bytes.
    pub fn parse_hex(hex: &str) -> Option<(u8, u8, u8)> {
        let hex = hex.strip_prefix('#')?;
        if hex.len() != 6 {
            return None;
        }
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        Some((r, g, b))
    }

    /// Get an ANSI color by index (0-15).
    pub fn ansi_color(&self, index: u8) -> &str {
        match index {
            0 => &self.black,
            1 => &self.red,
            2 => &self.green,
            3 => &self.yellow,
            4 => &self.blue,
            5 => &self.magenta,
            6 => &self.cyan,
            7 => &self.white,
            8 => &self.bright_black,
            9 => &self.bright_red,
            10 => &self.bright_green,
            11 => &self.bright_yellow,
            12 => &self.bright_blue,
            13 => &self.bright_magenta,
            14 => &self.bright_cyan,
            15 => &self.bright_white,
            _ => &self.foreground,
        }
    }
}

impl Default for ColorScheme {
    fn default() -> Self {
        Self::catppuccin_mocha()
    }
}

fn dirs_home() -> PathBuf {
    directories::UserDirs::new()
        .map(|d| d.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("~"))
}

/// Find the best available PowerShell executable on Windows.
/// Prefers PS7 (pwsh.exe) but falls back to inbox PS5.1 (powershell.exe).
#[cfg(target_os = "windows")]
fn find_pwsh_windows() -> String {
    // PowerShell 7+ standard install locations
    let candidates = [
        r"C:\Program Files\PowerShell\7\pwsh.exe",
        r"C:\Program Files\PowerShell\6\pwsh.exe",
    ];
    for path in &candidates {
        if std::path::Path::new(path).exists() {
            return path.to_string();
        }
    }
    // Try pwsh.exe on PATH (e.g. installed via winget/scoop)
    if std::process::Command::new("pwsh")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
    {
        return "pwsh.exe".to_string();
    }
    // Fall back to Windows PowerShell 5.1 (always present on Windows 10/11)
    r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe".to_string()
}

