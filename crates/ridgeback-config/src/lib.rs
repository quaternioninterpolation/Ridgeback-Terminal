pub mod profile;
pub mod keybindings;
pub mod rendering;
pub mod ai;
pub mod theme;
pub mod wsl;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub use profile::{ColorScheme, CursorStyle, Profile, ShaderEffect, ShaderParams, ShellType};
pub use keybindings::KeyBindings;
pub use rendering::RenderingConfig;
pub use ai::AiConfig;
pub use theme::TabBarPosition;
pub use wsl::{WslDistro, detect_wsl_distros, wsl_profiles, is_wsl_available};

/// Top-level Ridgeback configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub general: GeneralConfig,
    pub rendering: RenderingConfig,
    pub keybindings: KeyBindings,
    pub ai: AiConfig,
    pub profiles: HashMap<String, Profile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralConfig {
    pub default_profile: String,
    pub tab_bar_position: TabBarPosition,
    pub confirm_close_with_multiple_tabs: bool,
    pub font: FontConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FontConfig {
    pub family: String,
    pub size: f32,
    pub bold_is_bright: bool,
}

impl Default for Config {
    fn default() -> Self {
        let mut profiles = HashMap::new();

        // Platform-appropriate default profiles
        #[cfg(target_os = "windows")]
        {
            profiles.insert("powershell".to_string(), Profile::default_powershell());
            profiles.insert("cmd".to_string(), Profile::default_cmd());
            profiles.insert("wsl".to_string(), Profile::default_wsl());
        }
        #[cfg(target_os = "macos")]
        {
            profiles.insert("zsh".to_string(), Profile::default_zsh());
            profiles.insert("bash".to_string(), Profile::default_bash());
        }
        #[cfg(target_os = "linux")]
        {
            profiles.insert("bash".to_string(), Profile::default_bash());
            profiles.insert("zsh".to_string(), Profile::default_zsh());
            profiles.insert("fish".to_string(), Profile::default_fish());
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        {
            profiles.insert("bash".to_string(), Profile::default_bash());
        }

        Self {
            general: GeneralConfig::default(),
            rendering: RenderingConfig::default(),
            keybindings: KeyBindings::default(),
            ai: AiConfig::default(),
            profiles,
        }
    }
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            default_profile: Self::platform_default_profile().to_string(),
            tab_bar_position: TabBarPosition::Top,
            confirm_close_with_multiple_tabs: true,
            font: FontConfig::default(),
        }
    }
}

impl GeneralConfig {
    fn platform_default_profile() -> &'static str {
        #[cfg(target_os = "windows")]
        { "powershell" }
        #[cfg(target_os = "macos")]
        { "zsh" }
        #[cfg(target_os = "linux")]
        { "bash" }
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        { "bash" }
    }
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            family: "Cascadia Mono".to_string(),
            size: 14.0,
            bold_is_bright: true,
        }
    }
}

impl Config {
    /// Returns the platform-appropriate config directory.
    pub fn config_dir() -> Result<PathBuf> {
        let dirs = directories::ProjectDirs::from("", "", "Ridgeback")
            .context("Failed to determine config directory")?;
        Ok(dirs.config_dir().to_path_buf())
    }

    /// Returns the full path to the config file.
    pub fn config_path() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("config.toml"))
    }

    /// Load config from the default path, creating defaults if it doesn't exist.
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        if path.exists() {
            Self::load_from(&path)
        } else {
            let config = Config::default();
            config.save()?;
            Ok(config)
        }
    }

    /// Load config from a specific path.
    pub fn load_from(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config from {}", path.display()))?;
        let config: Config = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config from {}", path.display()))?;
        Ok(config)
    }

    /// Save config to the default path.
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        self.save_to(&path)
    }

    /// Save config to a specific path.
    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config directory {}", parent.display()))?;
        }
        let content = toml::to_string_pretty(self)
            .context("Failed to serialize config")?;
        std::fs::write(path, content)
            .with_context(|| format!("Failed to write config to {}", path.display()))?;
        Ok(())
    }

    /// Get the default profile, falling back to the first available.
    pub fn default_profile(&self) -> Option<(&str, &Profile)> {
        self.profiles
            .get(&self.general.default_profile)
            .map(|p| (self.general.default_profile.as_str(), p))
            .or_else(|| self.profiles.iter().next().map(|(k, v)| (k.as_str(), v)))
    }
}
