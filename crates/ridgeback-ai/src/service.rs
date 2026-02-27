use anyhow::Result;
use ridgeback_config::ai::AiConfig;
use ridgeback_config::ShellType;
use crate::backend::*;

/// High-level AI service coordinating autocomplete and command queries.
pub struct AiService {
    backend: Option<Box<dyn AiBackend>>,
    config: AiConfig,
}

impl AiService {
    pub fn new(config: &AiConfig) -> Self {
        let backend: Option<Box<dyn AiBackend>> = if config.enabled {
            match config.default_backend {
                ridgeback_config::ai::AiBackendType::LmStudio => {
                    Some(Box::new(LmStudioBackend::from_config(&config.backends.lm_studio)))
                }
                ridgeback_config::ai::AiBackendType::OpenAi => {
                    Some(Box::new(OpenAiBackend::from_config(&config.backends.openai)))
                }
                ridgeback_config::ai::AiBackendType::Claude => {
                    Some(Box::new(ClaudeBackend::from_config(&config.backends.claude)))
                }
                ridgeback_config::ai::AiBackendType::Local => {
                    Some(Box::new(LocalModelBackend::from_config(&config.backends.local)))
                }
            }
        } else {
            None
        };

        Self {
            backend,
            config: config.clone(),
        }
    }

    /// Check if the AI service is available.
    pub fn is_available(&self) -> bool {
        self.backend.as_ref().map_or(false, |b| b.is_available())
    }

    /// Clone the config so a background thread can create its own AiService.
    pub fn config_clone(&self) -> AiConfig {
        self.config.clone()
    }

    /// Generate autocomplete suggestions for the current terminal input.
    pub async fn suggest_completion(
        &self,
        partial_input: &str,
        context_lines: &[String],
        shell_type: ShellType,
        cwd: &str,
    ) -> Result<Option<String>> {
        let backend = match &self.backend {
            Some(b) if b.is_available() => b,
            _ => return Ok(None),
        };

        if !self.config.autocomplete.enabled || partial_input.is_empty() {
            return Ok(None);
        }

        let shell_name = match shell_type {
            ShellType::Powershell => "PowerShell",
            ShellType::Cmd => "Command Prompt (cmd.exe)",
            ShellType::Wsl => "WSL/Bash",
            ShellType::Bash => "Bash",
            ShellType::Zsh => "Zsh",
            ShellType::Fish => "Fish",
            ShellType::Custom => "Terminal",
        };

        let context = if context_lines.is_empty() {
            String::new()
        } else {
            format!(
                "\nRecent terminal output:\n{}\n",
                context_lines.join("\n")
            )
        };

        let os_name = current_os_name();

        let system_msg = format!(
            "You are a terminal command autocomplete engine. \
            Given the user's partial input, suggest ONLY the remaining characters to complete the command. \
            Do not repeat what the user has already typed. Return only the completion text, nothing else. \
            No explanation, no quotes, no markdown.\n\
            Shell: {shell_name}. OS: {os_name}. CWD: {cwd}{context}"
        );

        let request = CompletionRequest {
            messages: vec![
                AiMessage {
                    role: AiRole::System,
                    content: system_msg,
                },
                AiMessage {
                    role: AiRole::User,
                    content: partial_input.to_string(),
                },
            ],
            max_tokens: self.config.autocomplete.max_tokens,
            temperature: self.config.autocomplete.temperature,
            n: 1,
            stop: Some(vec!["\n".to_string()]),
        };

        let response = backend.complete(request).await?;
        Ok(response.choices.into_iter().next())
    }

    /// Generate command suggestions from a natural language query.
    pub async fn query_command(
        &self,
        query: &str,
        shell_type: ShellType,
        cwd: &str,
        history: &[String],
    ) -> Result<Vec<String>> {
        let backend = match &self.backend {
            Some(b) if b.is_available() => b,
            _ => return Ok(Vec::new()),
        };

        if !self.config.command_query.enabled || query.is_empty() {
            return Ok(Vec::new());
        }

        let shell_name = match shell_type {
            ShellType::Powershell => "PowerShell",
            ShellType::Cmd => "Command Prompt (cmd.exe)",
            ShellType::Wsl => "WSL/Bash",
            ShellType::Bash => "Bash",
            ShellType::Zsh => "Zsh",
            ShellType::Fish => "Fish",
            ShellType::Custom => "Terminal",
        };

        let n = self.config.command_query.max_suggestions;
        let os_name = current_os_name();

        let history_ctx = if history.is_empty() {
            String::new()
        } else {
            format!(
                "\nRecent commands:\n{}\n",
                history.iter().rev().take(5).map(|s| s.as_str()).collect::<Vec<_>>().join("\n")
            )
        };

        let system_msg = format!(
            "You are a terminal command generator for {shell_name} on {os_name}. \
            Given a natural language request, return exactly {n} different command suggestions. \
            Return ONLY the commands, one per line, no numbering, no explanation, no markdown. \
            CWD: {cwd}{history_ctx}"
        );

        let request = CompletionRequest {
            messages: vec![
                AiMessage {
                    role: AiRole::System,
                    content: system_msg,
                },
                AiMessage {
                    role: AiRole::User,
                    content: query.to_string(),
                },
            ],
            max_tokens: self.config.command_query.max_tokens,
            temperature: self.config.command_query.temperature,
            n: 1, // We ask for N suggestions in one response
            stop: None,
        };

        let response = backend.complete(request).await?;

        // Parse the response: each line is a suggestion
        let suggestions: Vec<String> = response
            .choices
            .first()
            .map(|text| {
                text.lines()
                    .map(|l| l.trim())
                    .filter(|l| !l.is_empty())
                    .map(|l| l.to_string())
                    .take(n as usize)
                    .collect()
            })
            .unwrap_or_default();

        Ok(suggestions)
    }
}

/// Returns a human-readable OS name for AI prompts.
fn current_os_name() -> &'static str {
    #[cfg(target_os = "windows")]
    { "Windows" }
    #[cfg(target_os = "macos")]
    { "macOS" }
    #[cfg(target_os = "linux")]
    { "Linux" }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    { "Unix" }
}
