use crate::cell::Cell;
use std::collections::VecDeque;

/// Ring buffer for scrollback history.
#[derive(Debug, Clone)]
pub struct ScrollbackBuffer {
    lines: VecDeque<Vec<Cell>>,
    capacity: usize,
}

impl ScrollbackBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            lines: VecDeque::with_capacity(capacity.min(1024)),
            capacity,
        }
    }

    /// Push a line into the scrollback. Drops oldest if at capacity.
    pub fn push(&mut self, line: Vec<Cell>) {
        if self.lines.len() >= self.capacity {
            self.lines.pop_front();
        }
        self.lines.push_back(line);
    }

    /// Pop the most recent line (used if scrolling back down).
    pub fn pop(&mut self) -> Option<Vec<Cell>> {
        self.lines.pop_back()
    }

    /// Number of lines in scrollback.
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Get a line by index (0 = oldest).
    pub fn line(&self, index: usize) -> Option<&Vec<Cell>> {
        self.lines.get(index)
    }

    /// Get the last N lines (most recent).
    pub fn last_n_lines(&self, n: usize) -> Vec<String> {
        let skip = self.lines.len().saturating_sub(n);
        self.lines
            .iter()
            .skip(skip)
            .map(|line| {
                let s: String = line.iter().map(|c| c.ch).collect();
                s.trim_end().to_string()
            })
            .collect()
    }

    /// Get all lines as strings.
    pub fn all_lines_as_strings(&self) -> Vec<String> {
        self.lines
            .iter()
            .map(|line| {
                let s: String = line.iter().map(|c| c.ch).collect();
                s.trim_end().to_string()
            })
            .collect()
    }

    /// Get full log as a single string.
    pub fn full_log(&self) -> String {
        self.all_lines_as_strings().join("\n")
    }

    /// Clear all scrollback.
    pub fn clear(&mut self) {
        self.lines.clear();
    }

    /// Resize the capacity, dropping oldest lines if needed.
    pub fn set_capacity(&mut self, new_capacity: usize) {
        self.capacity = new_capacity;
        while self.lines.len() > self.capacity {
            self.lines.pop_front();
        }
    }

    /// Iterator over all lines.
    pub fn iter(&self) -> impl Iterator<Item = &Vec<Cell>> {
        self.lines.iter()
    }
}
