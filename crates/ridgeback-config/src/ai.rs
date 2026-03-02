use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// AI feature configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AiConfig {
    pub enabled: bool,
    pub default_backend: AiBackendType,
    pub autocomplete: AutocompleteConfig,
    pub command_query: CommandQueryConfig,
    pub backends: AiBackends,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiBackendType {
    LmStudio,
    OpenAi,
    Claude,
    Local,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AutocompleteConfig {
    pub enabled: bool,
    pub debounce_ms: u32,
    pub max_tokens: u32,
    pub temperature: f32,
    pub context_lines: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CommandQueryConfig {
    pub enabled: bool,
    pub max_suggestions: u8,
    pub max_tokens: u32,
    pub temperature: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AiBackends {
    pub lm_studio: LmStudioConfig,
    pub openai: OpenAiConfig,
    pub claude: ClaudeConfig,
    pub local: LocalModelConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LmStudioConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OpenAiConfig {
    pub api_key: String,
    pub model: String,
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ClaudeConfig {
    pub api_key: String,
    pub model: String,
    pub max_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LocalModelConfig {
    /// Full HuggingFace URL, e.g. "https://huggingface.co/Qwen/Qwen2.5-Coder-1.5B-Instruct"
    pub huggingface_url: String,
    /// Inference device: "auto", "cpu", "metal", "cuda"
    pub device: String,
    /// Max context length for local inference.
    pub context_length: u32,
}

impl LocalModelConfig {
    /// Returns the platform-appropriate models cache directory.
    /// e.g. ~/.local/share/Ridgeback/models/ on Linux,
    ///      ~/Library/Application Support/Ridgeback/models/ on macOS.
    pub fn models_dir() -> Option<PathBuf> {
        directories::ProjectDirs::from("", "", "Ridgeback")
            .map(|d| d.data_dir().join("models"))
    }

    /// Extract "Owner/Repo" slug from the HuggingFace URL.
    /// e.g. "https://huggingface.co/Qwen/Qwen2.5-Coder-1.5B-Instruct" → "Qwen--Qwen2.5-Coder-1.5B-Instruct"
    pub fn repo_slug(&self) -> Option<String> {
        let url = self.huggingface_url.trim().trim_end_matches('/');
        let prefix = "https://huggingface.co/";
        if let Some(rest) = url.strip_prefix(prefix) {
            let parts: Vec<&str> = rest.splitn(3, '/').collect();
            if parts.len() >= 2 {
                return Some(format!("{}--{}", parts[0], parts[1]));
            }
        }
        None
    }

    /// Extract "owner/repo" from the HuggingFace URL for API calls.
    pub fn repo_id(&self) -> Option<String> {
        let url = self.huggingface_url.trim().trim_end_matches('/');
        let prefix = "https://huggingface.co/";
        if let Some(rest) = url.strip_prefix(prefix) {
            let parts: Vec<&str> = rest.splitn(3, '/').collect();
            if parts.len() >= 2 {
                return Some(format!("{}/{}", parts[0], parts[1]));
            }
        }
        None
    }

    /// Returns the local directory where this model's files are stored.
    pub fn model_path(&self) -> Option<PathBuf> {
        let slug = self.repo_slug()?;
        Self::models_dir().map(|d| d.join(slug))
    }
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_backend: AiBackendType::LmStudio,
            autocomplete: AutocompleteConfig::default(),
            command_query: CommandQueryConfig::default(),
            backends: AiBackends::default(),
        }
    }
}

impl Default for AutocompleteConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            debounce_ms: 250,
            max_tokens: 64,
            temperature: 0.2,
            context_lines: 10,
        }
    }
}

impl Default for CommandQueryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_suggestions: 3,
            max_tokens: 256,
            temperature: 0.4,
        }
    }
}

impl Default for AiBackends {
    fn default() -> Self {
        Self {
            lm_studio: LmStudioConfig::default(),
            openai: OpenAiConfig::default(),
            claude: ClaudeConfig::default(),
            local: LocalModelConfig::default(),
        }
    }
}

impl Default for LmStudioConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:1234/v1".to_string(),
            api_key: "lm-studio".to_string(),
            model: String::new(),
            timeout_secs: 30,
        }
    }
}

impl Default for OpenAiConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: "gpt-4o-mini".to_string(),
            timeout_secs: 30,
        }
    }
}

impl Default for ClaudeConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: "claude-sonnet-4-20250514".to_string(),
            max_tokens: 1024,
        }
    }
}

impl Default for LocalModelConfig {
    fn default() -> Self {
        Self {
            huggingface_url: "https://huggingface.co/Qwen/Qwen2.5-Coder-1.5B-Instruct".to_string(),
            device: "auto".to_string(),
            context_length: 2048,
        }
    }
}
