use serde::{Deserialize, Serialize};

/// Trait exposed to plugins for querying terminal buffer content.
pub trait TerminalQuery {
    /// Get the last N lines of terminal output.
    fn last_n_lines(&self, n: usize) -> Vec<String>;
    /// Get the entire terminal log as a single string.
    fn full_log(&self) -> String;
    /// Search for a pattern in the terminal buffer.
    fn search(&self, pattern: &str, use_regex: bool, ignore_case: bool) -> Vec<SearchMatch>;
}

/// A search match returned by the plugin API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchMatch {
    pub line: usize,
    pub col: usize,
    pub len: usize,
    pub text: String,
}

/// A line with styled spans, for export plugins that preserve color/attributes.
#[derive(Debug, Clone)]
pub struct StyledLine {
    pub spans: Vec<StyledSpan>,
}

/// A contiguous run of text with the same style.
#[derive(Debug, Clone)]
pub struct StyledSpan {
    pub text: String,
    pub fg: Option<(u8, u8, u8)>,
    pub bg: Option<(u8, u8, u8)>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

/// Trait for plugins that register additional save/export formats.
pub trait SaveFormatPlugin: Send + Sync {
    /// Display name of the format (e.g., "HTML", "Markdown").
    fn name(&self) -> &str;
    /// File extension (e.g., "html", "md").
    fn extension(&self) -> &str;
    /// Export the styled lines into the target format.
    fn export(&self, lines: &[StyledLine]) -> Vec<u8>;
}
