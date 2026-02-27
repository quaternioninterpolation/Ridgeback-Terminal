//! Ridgeback AI integration.
//!
//! Abstracts LLM backends (LM Studio, OpenAI, Claude, local models)
//! behind a common trait. Provides:
//! - AI-powered terminal autocomplete (ghost text)
//! - Natural language → command generation (Ctrl+/ query)
//!
//! Currently implements LM Studio backend via async-openai.

pub mod backend;
pub mod service;

pub use backend::{AiBackend, CompletionRequest, CompletionResponse, AiMessage, AiRole};
pub use service::AiService;
