use crate::cell::Cell;

/// A fixed-size grid of terminal cells representing the visible area.
#[derive(Debug, Clone)]
pub struct Grid {
    cells: Vec<Vec<Cell>>,
    rows: usize,
    cols: usize,
}

impl Grid {
    pub fn new(rows: usize, cols: usize) -> Self {
        let cells = vec![vec![Cell::default(); cols]; rows];
        Self { cells, rows, cols }
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    /// Get a reference to a cell at (row, col).
    pub fn cell(&self, row: usize, col: usize) -> Option<&Cell> {
        self.cells.get(row).and_then(|r| r.get(col))
    }

    /// Get a mutable reference to a cell at (row, col).
    pub fn cell_mut(&mut self, row: usize, col: usize) -> Option<&mut Cell> {
        self.cells.get_mut(row).and_then(|r| r.get_mut(col))
    }

    /// Get an entire row.
    pub fn row(&self, row: usize) -> Option<&[Cell]> {
        self.cells.get(row).map(|r| r.as_slice())
    }

    /// Get a mutable reference to an entire row.
    pub fn row_mut(&mut self, row: usize) -> Option<&mut Vec<Cell>> {
        self.cells.get_mut(row)
    }

    /// Convert a row to a string, trimming trailing spaces.
    pub fn row_to_string(&self, row: usize) -> String {
        match self.cells.get(row) {
            Some(cells) => {
                let s: String = cells.iter().map(|c| c.ch).collect();
                s.trim_end().to_string()
            }
            None => String::new(),
        }
    }

    /// Clear the entire grid.
    pub fn clear(&mut self) {
        for row in &mut self.cells {
            for cell in row.iter_mut() {
                cell.clear();
            }
        }
    }

    /// Clear a specific row.
    pub fn clear_row(&mut self, row: usize) {
        if let Some(r) = self.cells.get_mut(row) {
            for cell in r.iter_mut() {
                cell.clear();
            }
        }
    }

    /// Scroll up by one line: removes the top row and adds a blank row at the bottom.
    /// Returns the removed top row.
    pub fn scroll_up(&mut self) -> Vec<Cell> {
        let removed = self.cells.remove(0);
        self.cells.push(vec![Cell::default(); self.cols]);
        removed
    }

    /// Scroll a region up by one line within [top, bottom] inclusive.
    pub fn scroll_region_up(&mut self, top: usize, bottom: usize) {
        if top < bottom && bottom < self.rows {
            let removed = self.cells.remove(top);
            self.cells.insert(bottom, vec![Cell::default(); self.cols]);
            let _ = removed;
        }
    }

    /// Scroll a region down by one line within [top, bottom] inclusive.
    pub fn scroll_region_down(&mut self, top: usize, bottom: usize) {
        if top < bottom && bottom < self.rows {
            let removed = self.cells.remove(bottom);
            self.cells.insert(top, vec![Cell::default(); self.cols]);
            let _ = removed;
        }
    }

    /// Resize the grid, preserving content where possible.
    pub fn resize(&mut self, new_rows: usize, new_cols: usize) {
        // Resize columns for existing rows
        for row in &mut self.cells {
            row.resize(new_cols, Cell::default());
        }
        // Add or remove rows
        self.cells.resize(new_rows, vec![Cell::default(); new_cols]);
        self.rows = new_rows;
        self.cols = new_cols;
    }

    /// Get all visible rows.
    pub fn visible_rows(&self) -> &[Vec<Cell>] {
        &self.cells
    }

    /// Erase from cursor position to end of line.
    pub fn erase_to_eol(&mut self, row: usize, col: usize) {
        if let Some(r) = self.cells.get_mut(row) {
            for c in col..r.len() {
                r[c].clear();
            }
        }
    }

    /// Erase from beginning of line to cursor position.
    pub fn erase_from_bol(&mut self, row: usize, col: usize) {
        if let Some(r) = self.cells.get_mut(row) {
            for c in 0..=col.min(r.len().saturating_sub(1)) {
                r[c].clear();
            }
        }
    }

    /// Erase an entire line.
    pub fn erase_line(&mut self, row: usize) {
        self.clear_row(row);
    }

    /// Erase from cursor to end of screen.
    pub fn erase_below(&mut self, row: usize, col: usize) {
        self.erase_to_eol(row, col);
        for r in (row + 1)..self.rows {
            self.clear_row(r);
        }
    }

    /// Erase from beginning of screen to cursor.
    pub fn erase_above(&mut self, row: usize, col: usize) {
        for r in 0..row {
            self.clear_row(r);
        }
        self.erase_from_bol(row, col);
    }

    /// Insert blank characters at position, shifting existing chars right.
    pub fn insert_chars(&mut self, row: usize, col: usize, count: usize) {
        if let Some(r) = self.cells.get_mut(row) {
            for _ in 0..count {
                if col < r.len() {
                    r.insert(col, Cell::default());
                    r.truncate(self.cols);
                }
            }
        }
    }

    /// Delete characters at position, shifting remaining chars left.
    pub fn delete_chars(&mut self, row: usize, col: usize, count: usize) {
        if let Some(r) = self.cells.get_mut(row) {
            for _ in 0..count {
                if col < r.len() {
                    r.remove(col);
                    r.push(Cell::default());
                }
            }
        }
    }
}
