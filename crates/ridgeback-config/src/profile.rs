use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
    pub shader_effect: ShaderEffect,
    pub shader_params: ShaderParams,
    /// Default text foreground colour as "#RRGGBB". Overrides the colour scheme foreground.
    pub text_foreground: String,
    /// Draw a dark shadow behind text to keep it readable over bright backgrounds.
    pub text_shadow_enabled: bool,
    /// Shadow darkness: 0.0 = invisible, 1.0 = fully black.
    pub text_shadow_alpha: f32,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShaderEffect {
    None,
    Crt,
    Fire,
}

/// Parameters for shader effects, shared across all effect types.
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
            shader_effect: ShaderEffect::None,
            shader_params: ShaderParams::default(),
            text_foreground: "#CDD6F4".to_string(),
            text_shadow_enabled: true,
            text_shadow_alpha: 0.65,
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
            shader_effect: ShaderEffect::None,
            shader_params: ShaderParams::default(),
            text_foreground: "#CCCCCC".to_string(),
            text_shadow_enabled: true,
            text_shadow_alpha: 0.65,
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
            shader_effect: ShaderEffect::None,
            shader_params: ShaderParams::default(),
            text_foreground: "#CDD6F4".to_string(),
            text_shadow_enabled: true,
            text_shadow_alpha: 0.65,
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
            shader_effect: ShaderEffect::None,
            shader_params: ShaderParams::default(),
            text_foreground: "#CDD6F4".to_string(),
            text_shadow_enabled: true,
            text_shadow_alpha: 0.65,
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
            shader_effect: ShaderEffect::None,
            shader_params: ShaderParams::default(),
            text_foreground: "#CDD6F4".to_string(),
            text_shadow_enabled: true,
            text_shadow_alpha: 0.65,
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
            shader_effect: ShaderEffect::None,
            shader_params: ShaderParams::default(),
            text_foreground: "#CDD6F4".to_string(),
            text_shadow_enabled: true,
            text_shadow_alpha: 0.65,
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
            shader_effect: ShaderEffect::None,
            shader_params: ShaderParams::default(),
            text_foreground: "#CDD6F4".to_string(),
            text_shadow_enabled: true,
            text_shadow_alpha: 0.65,
        }
    }
}

impl Default for ShaderParams {
    fn default() -> Self {
        Self {
            scanline_intensity: 0.3,
            curvature: 0.2,
            bloom_strength: 0.4,
            chromatic_aberration: 0.5,
            fire_intensity: 0.8,
            fire_decay_rate: 0.05,
            fire_spread: 0.7,
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

