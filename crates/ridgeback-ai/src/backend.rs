use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Role in an AI conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiRole {
    System,
    User,
    Assistant,
}

/// A message in an AI conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiMessage {
    pub role: AiRole,
    pub content: String,
}

/// Request to an AI backend.
#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub messages: Vec<AiMessage>,
    pub max_tokens: u32,
    pub temperature: f32,
    /// Number of completions to return.
    pub n: u8,
    pub stop: Option<Vec<String>>,
}

/// Response from an AI backend.
#[derive(Debug, Clone)]
pub struct CompletionResponse {
    pub choices: Vec<String>,
    pub model: String,
}

/// Common trait for all AI backends.
pub trait AiBackend: Send + Sync {
    /// Human-readable name of this backend.
    fn name(&self) -> &str;

    /// Check if the backend is available (e.g., LM Studio running, API key set).
    fn is_available(&self) -> bool;

    /// Send a completion request and get a response.
    fn complete(
        &self,
        request: CompletionRequest,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<CompletionResponse>> + Send + '_>>;
}

// ── LM Studio Backend ──────────────────────────────────────────────────

/// LM Studio backend using OpenAI-compatible API.
pub struct LmStudioBackend {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub timeout_secs: u64,
}

impl LmStudioBackend {
    pub fn new(base_url: String, api_key: String, model: String, timeout_secs: u64) -> Self {
        Self {
            base_url,
            api_key,
            model,
            timeout_secs,
        }
    }

    pub fn from_config(config: &ridgeback_config::ai::LmStudioConfig) -> Self {
        Self::new(
            config.base_url.clone(),
            config.api_key.clone(),
            config.model.clone(),
            config.timeout_secs,
        )
    }
}

impl AiBackend for LmStudioBackend {
    fn name(&self) -> &str {
        "LM Studio"
    }

    fn is_available(&self) -> bool {
        !self.base_url.is_empty()
    }

    fn complete(
        &self,
        request: CompletionRequest,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<CompletionResponse>> + Send + '_>>
    {
        Box::pin(async move {
            use async_openai::config::OpenAIConfig;
            use async_openai::types::{
                ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
                ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequestArgs,
            };
            use async_openai::Client;

            let config = OpenAIConfig::new()
                .with_api_base(&self.base_url)
                .with_api_key(&self.api_key);
            let client = Client::with_config(config);

            let messages: Vec<ChatCompletionRequestMessage> = request
                .messages
                .iter()
                .map(|m| match m.role {
                    AiRole::System => ChatCompletionRequestSystemMessageArgs::default()
                        .content(m.content.clone())
                        .build()
                        .unwrap()
                        .into(),
                    AiRole::User => ChatCompletionRequestUserMessageArgs::default()
                        .content(m.content.clone())
                        .build()
                        .unwrap()
                        .into(),
                    AiRole::Assistant => {
                        // For assistant messages we use user with a prefix
                        ChatCompletionRequestUserMessageArgs::default()
                            .content(m.content.clone())
                            .build()
                            .unwrap()
                            .into()
                    }
                })
                .collect();

            let mut req_builder = CreateChatCompletionRequestArgs::default();
            req_builder
                .model(if self.model.is_empty() {
                    "local-model"
                } else {
                    &self.model
                })
                .messages(messages)
                .max_tokens(request.max_tokens as u16)
                .temperature(request.temperature)
                .n(request.n);

            if let Some(stop) = &request.stop {
                req_builder.stop(stop.clone());
            }

            let req = req_builder.build()?;
            let response = client.chat().create(req).await?;

            let choices: Vec<String> = response
                .choices
                .iter()
                .filter_map(|c| c.message.content.clone())
                .collect();

            Ok(CompletionResponse {
                choices,
                model: response.model,
            })
        })
    }
}

// ── OpenAI Backend ──────────────────────────────────────────────────────

/// OpenAI API backend (GPT-4o, GPT-4, etc.).
pub struct OpenAiBackend {
    pub api_key: String,
    pub model: String,
    pub timeout_secs: u64,
}

impl OpenAiBackend {
    pub fn new(api_key: String, model: String, timeout_secs: u64) -> Self {
        Self {
            api_key,
            model,
            timeout_secs,
        }
    }

    pub fn from_config(config: &ridgeback_config::ai::OpenAiConfig) -> Self {
        Self::new(
            config.api_key.clone(),
            config.model.clone(),
            config.timeout_secs,
        )
    }
}

impl AiBackend for OpenAiBackend {
    fn name(&self) -> &str {
        "OpenAI"
    }

    fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }

    fn complete(
        &self,
        request: CompletionRequest,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<CompletionResponse>> + Send + '_>>
    {
        Box::pin(async move {
            use async_openai::config::OpenAIConfig;
            use async_openai::types::{
                ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
                ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequestArgs,
            };
            use async_openai::Client;

            let config = OpenAIConfig::new().with_api_key(&self.api_key);
            let client = Client::with_config(config);

            let messages: Vec<ChatCompletionRequestMessage> = request
                .messages
                .iter()
                .map(|m| match m.role {
                    AiRole::System => ChatCompletionRequestSystemMessageArgs::default()
                        .content(m.content.clone())
                        .build()
                        .unwrap()
                        .into(),
                    AiRole::User | AiRole::Assistant => {
                        ChatCompletionRequestUserMessageArgs::default()
                            .content(m.content.clone())
                            .build()
                            .unwrap()
                            .into()
                    }
                })
                .collect();

            let mut req_builder = CreateChatCompletionRequestArgs::default();
            req_builder
                .model(&self.model)
                .messages(messages)
                .max_tokens(request.max_tokens as u16)
                .temperature(request.temperature)
                .n(request.n);

            if let Some(stop) = &request.stop {
                req_builder.stop(stop.clone());
            }

            let req = req_builder.build()?;
            let response = client.chat().create(req).await?;

            let choices: Vec<String> = response
                .choices
                .iter()
                .filter_map(|c| c.message.content.clone())
                .collect();

            Ok(CompletionResponse {
                choices,
                model: response.model,
            })
        })
    }
}

// ── Claude Backend ──────────────────────────────────────────────────────

/// Anthropic Claude backend using the Messages API.
pub struct ClaudeBackend {
    pub api_key: String,
    pub model: String,
    pub max_tokens: u32,
}

impl ClaudeBackend {
    pub fn new(api_key: String, model: String, max_tokens: u32) -> Self {
        Self {
            api_key,
            model,
            max_tokens,
        }
    }

    pub fn from_config(config: &ridgeback_config::ai::ClaudeConfig) -> Self {
        Self::new(
            config.api_key.clone(),
            config.model.clone(),
            config.max_tokens,
        )
    }
}

impl AiBackend for ClaudeBackend {
    fn name(&self) -> &str {
        "Claude"
    }

    fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }

    fn complete(
        &self,
        request: CompletionRequest,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<CompletionResponse>> + Send + '_>>
    {
        Box::pin(async move {
            let client = reqwest::Client::new();

            // Extract system message if present
            let system_text: Option<String> = request
                .messages
                .iter()
                .find(|m| m.role == AiRole::System)
                .map(|m| m.content.clone());

            // Build messages array (non-system only for Claude API)
            let api_messages: Vec<serde_json::Value> = request
                .messages
                .iter()
                .filter(|m| m.role != AiRole::System)
                .map(|m| {
                    let role = match m.role {
                        AiRole::User => "user",
                        AiRole::Assistant => "assistant",
                        AiRole::System => unreachable!(),
                    };
                    serde_json::json!({
                        "role": role,
                        "content": m.content,
                    })
                })
                .collect();

            let max_tokens = if request.max_tokens > 0 {
                request.max_tokens
            } else {
                self.max_tokens
            };

            let mut body = serde_json::json!({
                "model": self.model,
                "max_tokens": max_tokens,
                "messages": api_messages,
                "temperature": request.temperature,
            });

            if let Some(system) = &system_text {
                body["system"] = serde_json::Value::String(system.clone());
            }

            if let Some(stop) = &request.stop {
                body["stop_sequences"] = serde_json::json!(stop);
            }

            let resp = client
                .post("https://api.anthropic.com/v1/messages")
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .json(&body)
                .send()
                .await?;

            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                anyhow::bail!("Claude API error {status}: {text}");
            }

            let json: serde_json::Value = resp.json().await?;

            let choices: Vec<String> = json["content"]
                .as_array()
                .map(|blocks| {
                    blocks
                        .iter()
                        .filter_map(|b| {
                            if b["type"].as_str() == Some("text") {
                                b["text"].as_str().map(|s| s.to_string())
                            } else {
                                None
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();

            let model = json["model"]
                .as_str()
                .unwrap_or(&self.model)
                .to_string();

            Ok(CompletionResponse { choices, model })
        })
    }
}

// ── Local Model Backend ─────────────────────────────────────────────────

/// Local model backend for self-hosted LLMs (Ollama, llama.cpp server, etc.).
///
/// Communicates via the OpenAI-compatible API that local model servers expose.
pub struct LocalModelBackend {
    pub model_repo: String,
    pub quantization: String,
    pub device: String,
    pub context_length: u32,
    /// Base URL for the local inference server (defaults to Ollama).
    pub base_url: String,
}

impl LocalModelBackend {
    pub fn from_config(config: &ridgeback_config::ai::LocalModelConfig) -> Self {
        Self {
            model_repo: config.model_repo.clone(),
            quantization: config.quantization.clone(),
            device: config.device.clone(),
            context_length: config.context_length,
            base_url: "http://localhost:11434/v1".to_string(),
        }
    }
}

impl AiBackend for LocalModelBackend {
    fn name(&self) -> &str {
        "Local Model"
    }

    fn is_available(&self) -> bool {
        !self.model_repo.is_empty()
    }

    fn complete(
        &self,
        request: CompletionRequest,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<CompletionResponse>> + Send + '_>>
    {
        Box::pin(async move {
            use async_openai::config::OpenAIConfig;
            use async_openai::types::{
                ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
                ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequestArgs,
            };
            use async_openai::Client;

            // Use OpenAI-compatible API (Ollama, llama.cpp server, etc.)
            let config = OpenAIConfig::new()
                .with_api_base(&self.base_url)
                .with_api_key("local");
            let client = Client::with_config(config);

            let messages: Vec<ChatCompletionRequestMessage> = request
                .messages
                .iter()
                .map(|m| match m.role {
                    AiRole::System => ChatCompletionRequestSystemMessageArgs::default()
                        .content(m.content.clone())
                        .build()
                        .unwrap()
                        .into(),
                    AiRole::User | AiRole::Assistant => {
                        ChatCompletionRequestUserMessageArgs::default()
                            .content(m.content.clone())
                            .build()
                            .unwrap()
                            .into()
                    }
                })
                .collect();

            let model_name = if self.model_repo.is_empty() {
                "llama3"
            } else {
                &self.model_repo
            };

            let mut req_builder = CreateChatCompletionRequestArgs::default();
            req_builder
                .model(model_name)
                .messages(messages)
                .max_tokens(request.max_tokens.min(self.context_length) as u16)
                .temperature(request.temperature)
                .n(request.n);

            if let Some(stop) = &request.stop {
                req_builder.stop(stop.clone());
            }

            let req = req_builder.build()?;
            let response = client.chat().create(req).await?;

            let choices: Vec<String> = response
                .choices
                .iter()
                .filter_map(|c| c.message.content.clone())
                .collect();

            Ok(CompletionResponse {
                choices,
                model: response.model,
            })
        })
    }
}
