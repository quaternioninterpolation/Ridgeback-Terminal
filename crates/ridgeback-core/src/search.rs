use crate::buffer::ScrollbackBuffer;
use crate::grid::Grid;

/// A match found by searching the terminal buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchMatch {
    /// Line number (0-indexed, relative to scrollback start).
    pub line: usize,
    /// Column start (0-indexed).
    pub col: usize,
    /// Length of the match in characters.
    pub len: usize,
    /// The matched text.
    pub text: String,
}

/// Options controlling search behavior.
#[derive(Debug, Clone)]
pub struct SearchOptions {
    /// The search pattern.
    pub pattern: String,
    /// Whether to use regex mode.
    pub use_regex: bool,
    /// Whether to ignore case (case-insensitive).
    pub ignore_case: bool,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            pattern: String::new(),
            use_regex: false,
            ignore_case: true,
        }
    }
}

/// Search through scrollback buffer and visible grid for matching text.
pub fn search(
    scrollback: &ScrollbackBuffer,
    grid: &Grid,
    options: &SearchOptions,
) -> Vec<SearchMatch> {
    if options.pattern.is_empty() {
        return Vec::new();
    }

    let mut results = Vec::new();

    // Build the regex or plain search pattern
    let regex = if options.use_regex {
        build_regex(&options.pattern, options.ignore_case)
    } else {
        build_regex(&regex::escape(&options.pattern), options.ignore_case)
    };

    let regex = match regex {
        Ok(r) => r,
        Err(_) => return Vec::new(), // Invalid pattern, return empty
    };

    // Search scrollback
    for (line_idx, line) in scrollback.iter().enumerate() {
        let text: String = line.iter().map(|c| c.ch).collect();
        let text = text.trim_end();
        for mat in regex.find_iter(text) {
            results.push(SearchMatch {
                line: line_idx,
                col: mat.start(),
                len: mat.len(),
                text: mat.as_str().to_string(),
            });
        }
    }

    // Search visible grid
    let scrollback_len = scrollback.len();
    for row in 0..grid.rows() {
        let text = grid.row_to_string(row);
        for mat in regex.find_iter(&text) {
            results.push(SearchMatch {
                line: scrollback_len + row,
                col: mat.start(),
                len: mat.len(),
                text: mat.as_str().to_string(),
            });
        }
    }

    results
}

fn build_regex(pattern: &str, ignore_case: bool) -> Result<regex::Regex, regex::Error> {
    let mut builder = String::new();
    if ignore_case {
        builder.push_str("(?i)");
    }
    builder.push_str(pattern);
    regex::Regex::new(&builder)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::Cell;

    #[test]
    fn test_basic_search() {
        let mut scrollback = ScrollbackBuffer::new(1000);
        let line: Vec<Cell> = "hello world".chars().map(Cell::new).collect();
        scrollback.push(line);
        let grid = Grid::new(24, 80);

        let opts = SearchOptions {
            pattern: "world".to_string(),
            use_regex: false,
            ignore_case: false,
        };

        let results = search(&scrollback, &grid, &opts);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].col, 6);
        assert_eq!(results[0].text, "world");
    }

    #[test]
    fn test_case_insensitive_search() {
        let mut scrollback = ScrollbackBuffer::new(1000);
        let line: Vec<Cell> = "Hello World".chars().map(Cell::new).collect();
        scrollback.push(line);
        let grid = Grid::new(24, 80);

        let opts = SearchOptions {
            pattern: "hello".to_string(),
            use_regex: false,
            ignore_case: true,
        };

        let results = search(&scrollback, &grid, &opts);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_regex_search() {
        let mut scrollback = ScrollbackBuffer::new(1000);
        let line: Vec<Cell> = "error: file not found".chars().map(Cell::new).collect();
        scrollback.push(line);
        let grid = Grid::new(24, 80);

        let opts = SearchOptions {
            pattern: r"error:\s+\w+".to_string(),
            use_regex: true,
            ignore_case: false,
        };

        let results = search(&scrollback, &grid, &opts);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].text, "error: file");
    }
}
