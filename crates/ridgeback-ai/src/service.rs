use anyhow::Result;
use ridgeback_config::ai::AiConfig;
use ridgeback_config::ShellType;
use crate::backend::*;
use crate::local_manager::LocalModelManager;

/// High-level AI service coordinating autocomplete and command queries.
pub struct AiService {
    backend: Option<Box<dyn AiBackend>>,
    config: AiConfig,
}

impl AiService {
    pub fn new(config: &AiConfig, local_manager: Option<&LocalModelManager>) -> Self {
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
                    if let Some(mgr) = local_manager {
                        Some(Box::new(LocalModelBackend::new_with_manager(mgr.clone())))
                    } else {
                        Some(Box::new(LocalModelBackend::from_config(&config.backends.local)))
                    }
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
        tracing::info!("AiService::query_command called, query='{}', backend={:?}", query, self.config.default_backend);

        let backend = match &self.backend {
            Some(b) if b.is_available() => {
                tracing::info!("AiService: backend '{}' is available", b.name());
                b
            }
            Some(b) => {
                tracing::warn!("AiService: backend '{}' is NOT available, returning empty", b.name());
                return Ok(Vec::new());
            }
            _ => {
                tracing::warn!("AiService: no backend configured, returning empty");
                return Ok(Vec::new());
            }
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
            // Filter to lines that look like commands (non-empty, not just whitespace/prompts)
            let cmds: Vec<&str> = history.iter()
                .map(|s| s.trim())
                .filter(|l| !l.is_empty() && l.len() > 1)
                .rev()
                .take(3)
                .collect();
            if cmds.is_empty() {
                String::new()
            } else {
                format!("\nRecent terminal output:\n{}\n", cmds.join("\n"))
            }
        };

        let system_msg = format!(
            "You translate natural language into {shell_name} commands for {os_name}.\n\
            Rules:\n\
            - Output ONLY shell commands, one per line\n\
            - Each line must be a COMPLETE solution the user can run as-is\n\
            - If a task needs multiple steps, chain them with && on ONE line\n\
            - Output exactly {n} alternative command(s), each on its own line\n\
            - No explanations, no numbering, no markdown, no commentary\n\
            - Commands should be relative to the current directory when possible\n\
            - Use simple common commands (cd, ls, cat, grep, mkdir, etc.)\n\
            \n\
            Examples:\n\
            User: list files → ls -la\n\
            User: go to home → cd ~\n\
            User: open src folder → cd src\n\
            User: create temp and go into it → mkdir temp && cd temp\n\
            User: make a project folder with a readme → mkdir project && echo '# Project' > project/README.md\n\
            User: show disk usage → df -h\n\
            User: find large files → find . -size +100M\n\
            User: clean build and rebuild → rm -rf build && mkdir build && cd build\n\
            \n\
            Current directory: {cwd}{history_ctx}"
        );

        // For command queries, we don't need many tokens — commands are short
        let max_tokens = self.config.command_query.max_tokens.min(64);

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
            max_tokens,
            temperature: self.config.command_query.temperature,
            n: 1,
            stop: Some(vec![
                "\n\n".to_string(),  // Stop after a blank line (end of commands)
                "User:".to_string(), // Stop if model tries to continue the conversation
                "```".to_string(),   // Stop if model tries to use markdown
            ]),
        };

        tracing::info!("AiService: sending completion request (max_tokens={}, temp={})", max_tokens, request.temperature);
        let response = backend.complete(request).await?;
        tracing::info!("AiService: got {} choices from backend", response.choices.len());
        if let Some(first) = response.choices.first() {
            tracing::info!("AiService: raw response text: {:?}", first);
        }

        // Parse the response: each line is a suggestion.
        // Clean up common local-model artifacts: numbering, markdown, backticks, explanations.
        let raw_lines: Vec<String> = response
            .choices
            .first()
            .map(|text| {
                text.lines()
                    .map(|l| l.trim())
                    .filter(|l| !l.is_empty())
                    // Strip markdown code fences
                    .filter(|l| !l.starts_with("```"))
                    // Strip lines that look like explanations
                    .filter(|l| !l.starts_with("//") && !l.starts_with("Note:") && !l.starts_with("Explanation:"))
                    .map(|l| {
                        let mut s = l.to_string();
                        // Strip leading numbering: "1. ", "1) ", "- "
                        if let Some(rest) = s.strip_prefix("- ") {
                            s = rest.to_string();
                        } else if s.len() > 2 {
                            let bytes = s.as_bytes();
                            if bytes[0].is_ascii_digit() && (bytes[1] == b'.' || bytes[1] == b')') {
                                s = s[2..].trim_start().to_string();
                            } else if bytes[0].is_ascii_digit() && bytes[1].is_ascii_digit()
                                && bytes.len() > 3 && (bytes[2] == b'.' || bytes[2] == b')') {
                                s = s[3..].trim_start().to_string();
                            }
                        }
                        // Strip surrounding backticks
                        if s.starts_with('`') && s.ends_with('`') && s.len() > 2 {
                            s = s[1..s.len()-1].to_string();
                        }
                        s
                    })
                    .filter(|l| !l.is_empty())
                    .collect()
            })
            .unwrap_or_default();

        // Smart collapsing: if the model returned more lines than we asked for,
        // it likely produced sequential steps instead of alternatives.
        // Join them with && into a single command.
        let suggestions = if raw_lines.len() > n as usize {
            // More lines than requested — treat as sequential steps, chain them
            let chained = raw_lines.join(" && ");
            tracing::info!("AiService: collapsed {} lines into chained command", raw_lines.len());
            vec![chained]
        } else if n == 1 && raw_lines.len() > 1 {
            // Asked for 1 suggestion but got multiple lines — chain them
            let chained = raw_lines.join(" && ");
            tracing::info!("AiService: collapsed {} lines into single chained command", raw_lines.len());
            vec![chained]
        } else {
            // Line count matches requested count — treat as alternatives
            raw_lines.into_iter().take(n as usize).collect()
        };

        tracing::info!("AiService: parsed {} suggestions", suggestions.len());

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
