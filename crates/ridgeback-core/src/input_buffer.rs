/// A fully-editable input buffer for the terminal input line.
///
/// Supports cursor movement, selection, undo/redo, clipboard operations,
/// command history, and ghost-text (AI autocomplete suggestions).
/// Has zero GUI dependencies — fully unit-testable.
#[derive(Debug, Clone)]
pub struct InputBuffer {
    text: String,
    cursor: usize,
    selection: Option<Selection>,
    history: Vec<String>,
    history_index: Option<usize>,
    /// Text being composed before history navigation.
    draft: String,
    undo_stack: Vec<UndoEntry>,
    redo_stack: Vec<UndoEntry>,
    /// Ghost text shown as dimmed suggestion (not part of `text`).
    pub ghost_text: Option<String>,
}

/// A selection range in the input buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    /// Where the selection started (anchor).
    pub anchor: usize,
    /// Where the selection currently extends to (head/cursor end).
    pub head: usize,
}

/// An undo/redo entry capturing a snapshot.
#[derive(Debug, Clone)]
struct UndoEntry {
    text: String,
    cursor: usize,
    selection: Option<Selection>,
}

/// Result of an input buffer operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputAction {
    /// Nothing changed.
    None,
    /// The buffer content or cursor changed — request a repaint.
    Changed,
    /// The user submitted the input line — send it to the PTY.
    Submit(String),
    /// Text to copy to the clipboard.
    Copy(String),
    /// Text to cut to the clipboard.
    Cut(String),
    /// Send raw bytes to the PTY (e.g., tab for path completion).
    SendToPty(Vec<u8>),
    /// Send SIGINT (Ctrl+C with no selection and empty input).
    Interrupt,
}

impl Default for InputBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl InputBuffer {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            cursor: 0,
            selection: None,
            history: Vec::new(),
            history_index: None,
            draft: String::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            ghost_text: None,
        }
    }

    // ── Accessors ──────────────────────────────────────────────────────

    pub fn text(&self) -> &str {
        &self.text
    }

    /// Cursor position as a char offset.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn selection(&self) -> Option<Selection> {
        self.selection
    }

    /// Get the selected text range (start, end) in sorted order.
    pub fn selection_range(&self) -> Option<(usize, usize)> {
        self.selection.map(|s| {
            let start = s.anchor.min(s.head);
            let end = s.anchor.max(s.head);
            (start, end)
        })
    }

    /// Get the selected text.
    pub fn selected_text(&self) -> Option<&str> {
        self.selection_range().map(|(start, end)| {
            let start = self.char_to_byte(start);
            let end = self.char_to_byte(end);
            &self.text[start..end]
        })
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn char_count(&self) -> usize {
        self.text.chars().count()
    }

    // ── Private helpers ────────────────────────────────────────────────

    fn char_to_byte(&self, char_idx: usize) -> usize {
        self.text
            .char_indices()
            .nth(char_idx)
            .map(|(i, _)| i)
            .unwrap_or(self.text.len())
    }

    fn save_undo(&mut self) {
        self.undo_stack.push(UndoEntry {
            text: self.text.clone(),
            cursor: self.cursor,
            selection: self.selection,
        });
        self.redo_stack.clear();
        // Limit undo stack size
        if self.undo_stack.len() > 100 {
            self.undo_stack.remove(0);
        }
    }

    fn delete_selection_inner(&mut self) -> bool {
        if let Some((start, end)) = self.selection_range() {
            let byte_start = self.char_to_byte(start);
            let byte_end = self.char_to_byte(end);
            self.text.replace_range(byte_start..byte_end, "");
            self.cursor = start;
            self.selection = None;
            true
        } else {
            false
        }
    }

    fn clear_ghost(&mut self) {
        self.ghost_text = None;
    }

    fn is_word_char(c: char) -> bool {
        c.is_alphanumeric() || c == '_' || c == '-'
    }

    fn find_word_boundary_left(&self, from: usize) -> usize {
        if from == 0 {
            return 0;
        }
        let chars: Vec<char> = self.text.chars().collect();
        let mut pos = from - 1;
        // Skip non-word chars
        while pos > 0 && !Self::is_word_char(chars[pos]) {
            pos -= 1;
        }
        // Skip word chars
        while pos > 0 && Self::is_word_char(chars[pos - 1]) {
            pos -= 1;
        }
        if !Self::is_word_char(chars[pos]) && pos < from - 1 {
            pos + 1
        } else {
            pos
        }
    }

    fn find_word_boundary_right(&self, from: usize) -> usize {
        let chars: Vec<char> = self.text.chars().collect();
        let len = chars.len();
        if from >= len {
            return len;
        }
        let mut pos = from;
        // Skip word chars
        while pos < len && Self::is_word_char(chars[pos]) {
            pos += 1;
        }
        // Skip non-word chars
        while pos < len && !Self::is_word_char(chars[pos]) {
            pos += 1;
        }
        pos
    }

    fn find_word_start_at(&self, at: usize) -> usize {
        let chars: Vec<char> = self.text.chars().collect();
        if at >= chars.len() {
            return at;
        }
        let mut start = at;
        while start > 0 && Self::is_word_char(chars[start - 1]) {
            start -= 1;
        }
        start
    }

    fn find_word_end_at(&self, at: usize) -> usize {
        let chars: Vec<char> = self.text.chars().collect();
        if at >= chars.len() {
            return chars.len();
        }
        let mut end = at;
        while end < chars.len() && Self::is_word_char(chars[end]) {
            end += 1;
        }
        end
    }

    // ── Editing ────────────────────────────────────────────────────────

    /// Insert a single character at the cursor position.
    pub fn insert_char(&mut self, ch: char) -> InputAction {
        self.save_undo();
        self.delete_selection_inner();
        let byte_pos = self.char_to_byte(self.cursor);
        self.text.insert(byte_pos, ch);
        self.cursor += 1;
        self.clear_ghost();
        self.history_index = None;
        InputAction::Changed
    }

    /// Insert a string at the cursor position (paste, AI suggestion).
    pub fn insert_text(&mut self, s: &str) -> InputAction {
        if s.is_empty() {
            return InputAction::None;
        }
        self.save_undo();
        self.delete_selection_inner();
        let byte_pos = self.char_to_byte(self.cursor);
        self.text.insert_str(byte_pos, s);
        self.cursor += s.chars().count();
        self.selection = None;
        self.clear_ghost();
        self.history_index = None;
        InputAction::Changed
    }

    /// Replace the entire input buffer with new text, cursor at end.
    pub fn set_text(&mut self, s: &str) {
        self.save_undo();
        self.text = s.to_string();
        self.cursor = s.chars().count();
        self.selection = None;
        self.clear_ghost();
        self.history_index = None;
    }

    /// Delete character before cursor (Backspace).
    pub fn delete_back(&mut self) -> InputAction {
        self.save_undo();
        if self.delete_selection_inner() {
            self.clear_ghost();
            return InputAction::Changed;
        }
        if self.cursor == 0 {
            return InputAction::None;
        }
        self.cursor -= 1;
        let byte_pos = self.char_to_byte(self.cursor);
        let next_byte = self.char_to_byte(self.cursor + 1);
        self.text.replace_range(byte_pos..next_byte, "");
        self.clear_ghost();
        InputAction::Changed
    }

    /// Delete character after cursor (Delete key).
    pub fn delete_forward(&mut self) -> InputAction {
        self.save_undo();
        if self.delete_selection_inner() {
            self.clear_ghost();
            return InputAction::Changed;
        }
        if self.cursor >= self.char_count() {
            return InputAction::None;
        }
        let byte_pos = self.char_to_byte(self.cursor);
        let next_byte = self.char_to_byte(self.cursor + 1);
        self.text.replace_range(byte_pos..next_byte, "");
        self.clear_ghost();
        InputAction::Changed
    }

    /// Delete word before cursor (Ctrl+Backspace).
    pub fn delete_word_back(&mut self) -> InputAction {
        self.save_undo();
        if self.delete_selection_inner() {
            self.clear_ghost();
            return InputAction::Changed;
        }
        let target = self.find_word_boundary_left(self.cursor);
        if target == self.cursor {
            return InputAction::None;
        }
        let byte_start = self.char_to_byte(target);
        let byte_end = self.char_to_byte(self.cursor);
        self.text.replace_range(byte_start..byte_end, "");
        self.cursor = target;
        self.clear_ghost();
        InputAction::Changed
    }

    /// Delete word after cursor (Ctrl+Delete).
    pub fn delete_word_forward(&mut self) -> InputAction {
        self.save_undo();
        if self.delete_selection_inner() {
            self.clear_ghost();
            return InputAction::Changed;
        }
        let target = self.find_word_boundary_right(self.cursor);
        if target == self.cursor {
            return InputAction::None;
        }
        let byte_start = self.char_to_byte(self.cursor);
        let byte_end = self.char_to_byte(target);
        self.text.replace_range(byte_start..byte_end, "");
        self.clear_ghost();
        InputAction::Changed
    }

    // ── Cursor movement ────────────────────────────────────────────────

    /// Move cursor left one character.
    pub fn move_left(&mut self) -> InputAction {
        if let Some((start, _)) = self.selection_range() {
            self.cursor = start;
            self.selection = None;
            return InputAction::Changed;
        }
        if self.cursor > 0 {
            self.cursor -= 1;
            InputAction::Changed
        } else {
            InputAction::None
        }
    }

    /// Move cursor right one character.
    pub fn move_right(&mut self) -> InputAction {
        if let Some((_, end)) = self.selection_range() {
            self.cursor = end;
            self.selection = None;
            return InputAction::Changed;
        }
        if self.cursor < self.char_count() {
            self.cursor += 1;
            InputAction::Changed
        } else {
            InputAction::None
        }
    }

    /// Move cursor left one word (Ctrl+Left).
    pub fn move_word_left(&mut self) -> InputAction {
        self.selection = None;
        let target = self.find_word_boundary_left(self.cursor);
        if target != self.cursor {
            self.cursor = target;
            InputAction::Changed
        } else {
            InputAction::None
        }
    }

    /// Move cursor right one word (Ctrl+Right).
    /// If ghost text is present and cursor is at end, accepts one word of ghost text.
    pub fn move_word_right(&mut self) -> InputAction {
        self.selection = None;
        // Accept ghost text word-by-word if at end
        if self.cursor >= self.char_count() {
            if let Some(ghost) = self.ghost_text.take() {
                let chars: Vec<char> = ghost.chars().collect();
                let mut end = 0;
                // Find the end of the first word in ghost text
                while end < chars.len() && Self::is_word_char(chars[end]) {
                    end += 1;
                }
                // Also grab trailing whitespace
                while end < chars.len() && chars[end] == ' ' {
                    end += 1;
                }
                if end == 0 {
                    end = 1.min(chars.len());
                }
                let accepted: String = chars[..end].iter().collect();
                let remaining: String = chars[end..].iter().collect();
                self.text.push_str(&accepted);
                self.cursor += accepted.chars().count();
                if !remaining.is_empty() {
                    self.ghost_text = Some(remaining);
                }
                return InputAction::Changed;
            }
        }
        let target = self.find_word_boundary_right(self.cursor);
        if target != self.cursor {
            self.cursor = target;
            InputAction::Changed
        } else {
            InputAction::None
        }
    }

    /// Move cursor to beginning of input (Home).
    pub fn move_home(&mut self) -> InputAction {
        self.selection = None;
        if self.cursor != 0 {
            self.cursor = 0;
            InputAction::Changed
        } else {
            InputAction::None
        }
    }

    /// Move cursor to end of input (End).
    pub fn move_end(&mut self) -> InputAction {
        self.selection = None;
        let end = self.char_count();
        if self.cursor != end {
            self.cursor = end;
            InputAction::Changed
        } else {
            InputAction::None
        }
    }

    // ── Selection ──────────────────────────────────────────────────────

    fn extend_selection(&mut self, new_head: usize) {
        if let Some(ref mut sel) = self.selection {
            sel.head = new_head;
        } else {
            self.selection = Some(Selection {
                anchor: self.cursor,
                head: new_head,
            });
        }
        self.cursor = new_head;
    }

    /// Extend selection one char left (Shift+Left).
    pub fn select_left(&mut self) -> InputAction {
        if self.cursor > 0 {
            let new_head = self.cursor - 1;
            self.extend_selection(new_head);
            InputAction::Changed
        } else {
            InputAction::None
        }
    }

    /// Extend selection one char right (Shift+Right).
    pub fn select_right(&mut self) -> InputAction {
        if self.cursor < self.char_count() {
            let new_head = self.cursor + 1;
            self.extend_selection(new_head);
            InputAction::Changed
        } else {
            InputAction::None
        }
    }

    /// Extend selection one word left (Ctrl+Shift+Left).
    pub fn select_word_left(&mut self) -> InputAction {
        let target = self.find_word_boundary_left(self.cursor);
        if target != self.cursor {
            self.extend_selection(target);
            InputAction::Changed
        } else {
            InputAction::None
        }
    }

    /// Extend selection one word right (Ctrl+Shift+Right).
    pub fn select_word_right(&mut self) -> InputAction {
        let target = self.find_word_boundary_right(self.cursor);
        if target != self.cursor {
            self.extend_selection(target);
            InputAction::Changed
        } else {
            InputAction::None
        }
    }

    /// Extend selection to beginning (Shift+Home).
    pub fn select_home(&mut self) -> InputAction {
        if self.cursor > 0 {
            self.extend_selection(0);
            InputAction::Changed
        } else {
            InputAction::None
        }
    }

    /// Extend selection to end (Shift+End).
    pub fn select_end(&mut self) -> InputAction {
        let end = self.char_count();
        if self.cursor < end {
            self.extend_selection(end);
            InputAction::Changed
        } else {
            InputAction::None
        }
    }

    /// Select all input text (Ctrl+A).
    pub fn select_all(&mut self) -> InputAction {
        let len = self.char_count();
        if len == 0 {
            return InputAction::None;
        }
        self.selection = Some(Selection {
            anchor: 0,
            head: len,
        });
        self.cursor = len;
        InputAction::Changed
    }

    /// Set cursor at a specific column (mouse click).
    pub fn set_cursor_at(&mut self, col: usize) -> InputAction {
        let target = col.min(self.char_count());
        self.cursor = target;
        self.selection = None;
        InputAction::Changed
    }

    /// Extend selection to a column (mouse drag).
    pub fn select_to(&mut self, col: usize) -> InputAction {
        let target = col.min(self.char_count());
        self.extend_selection(target);
        InputAction::Changed
    }

    /// Select the word at a column (double-click).
    pub fn select_word_at(&mut self, col: usize) -> InputAction {
        let at = col.min(self.char_count().saturating_sub(1));
        let start = self.find_word_start_at(at);
        let end = self.find_word_end_at(at);
        if start < end {
            self.selection = Some(Selection {
                anchor: start,
                head: end,
            });
            self.cursor = end;
            InputAction::Changed
        } else {
            InputAction::None
        }
    }

    /// Select entire input (triple-click).
    pub fn select_all_by_click(&mut self) -> InputAction {
        self.select_all()
    }

    // ── Clipboard ──────────────────────────────────────────────────────

    /// Copy selected text. If no selection and input is empty, send SIGINT.
    pub fn copy(&self) -> InputAction {
        if let Some(text) = self.selected_text() {
            InputAction::Copy(text.to_string())
        } else if self.text.is_empty() {
            InputAction::Interrupt
        } else {
            InputAction::None
        }
    }

    /// Cut selected text.
    pub fn cut(&mut self) -> InputAction {
        if let Some(text) = self.selected_text().map(|s| s.to_string()) {
            self.save_undo();
            self.delete_selection_inner();
            self.clear_ghost();
            InputAction::Cut(text)
        } else {
            InputAction::None
        }
    }

    /// Paste text from clipboard.
    pub fn paste(&mut self, text: &str) -> InputAction {
        self.insert_text(text)
    }

    // ── Undo / Redo ────────────────────────────────────────────────────

    /// Undo the last edit operation (Ctrl+Z).
    pub fn undo(&mut self) -> InputAction {
        if let Some(entry) = self.undo_stack.pop() {
            self.redo_stack.push(UndoEntry {
                text: self.text.clone(),
                cursor: self.cursor,
                selection: self.selection,
            });
            self.text = entry.text;
            self.cursor = entry.cursor;
            self.selection = entry.selection;
            self.clear_ghost();
            InputAction::Changed
        } else {
            InputAction::None
        }
    }

    /// Redo the last undone operation (Ctrl+Y / Ctrl+Shift+Z).
    pub fn redo(&mut self) -> InputAction {
        if let Some(entry) = self.redo_stack.pop() {
            self.undo_stack.push(UndoEntry {
                text: self.text.clone(),
                cursor: self.cursor,
                selection: self.selection,
            });
            self.text = entry.text;
            self.cursor = entry.cursor;
            self.selection = entry.selection;
            self.clear_ghost();
            InputAction::Changed
        } else {
            InputAction::None
        }
    }

    // ── Submit / History ───────────────────────────────────────────────

    /// Submit the current input (Enter key). Returns the text to send to PTY.
    pub fn submit(&mut self) -> InputAction {
        let text = self.text.clone();
        if !text.trim().is_empty() {
            self.history.push(text.clone());
        }
        self.text.clear();
        self.cursor = 0;
        self.selection = None;
        self.history_index = None;
        self.ghost_text = None;
        self.undo_stack.clear();
        self.redo_stack.clear();
        InputAction::Submit(text)
    }

    /// Navigate to previous command in history (Up arrow).
    pub fn history_prev(&mut self) -> InputAction {
        if self.history.is_empty() {
            return InputAction::None;
        }
        match self.history_index {
            None => {
                self.draft = self.text.clone();
                let idx = self.history.len() - 1;
                self.history_index = Some(idx);
                self.text = self.history[idx].clone();
            }
            Some(idx) if idx > 0 => {
                let new_idx = idx - 1;
                self.history_index = Some(new_idx);
                self.text = self.history[new_idx].clone();
            }
            _ => return InputAction::None,
        }
        self.cursor = self.char_count();
        self.selection = None;
        self.clear_ghost();
        InputAction::Changed
    }

    /// Navigate to next command in history (Down arrow).
    pub fn history_next(&mut self) -> InputAction {
        match self.history_index {
            Some(idx) => {
                if idx < self.history.len() - 1 {
                    let new_idx = idx + 1;
                    self.history_index = Some(new_idx);
                    self.text = self.history[new_idx].clone();
                } else {
                    self.history_index = None;
                    self.text = self.draft.clone();
                }
                self.cursor = self.char_count();
                self.selection = None;
                self.clear_ghost();
                InputAction::Changed
            }
            None => InputAction::None,
        }
    }

    // ── Ghost text (AI autocomplete) ───────────────────────────────────

    /// Accept the full ghost text (Tab key).
    pub fn accept_ghost_text(&mut self) -> InputAction {
        if let Some(ghost) = self.ghost_text.take() {
            self.save_undo();
            self.text.push_str(&ghost);
            self.cursor = self.char_count();
            self.selection = None;
            InputAction::Changed
        } else {
            // No ghost text — send Tab to PTY for path completion
            InputAction::SendToPty(b"\t".to_vec())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_cursor() {
        let mut buf = InputBuffer::new();
        buf.insert_char('h');
        buf.insert_char('e');
        buf.insert_char('l');
        buf.insert_char('l');
        buf.insert_char('o');
        assert_eq!(buf.text(), "hello");
        assert_eq!(buf.cursor(), 5);
    }

    #[test]
    fn test_backspace() {
        let mut buf = InputBuffer::new();
        buf.insert_text("hello");
        buf.delete_back();
        assert_eq!(buf.text(), "hell");
        assert_eq!(buf.cursor(), 4);
    }

    #[test]
    fn test_cursor_movement() {
        let mut buf = InputBuffer::new();
        buf.insert_text("hello world");
        buf.move_home();
        assert_eq!(buf.cursor(), 0);
        buf.move_end();
        assert_eq!(buf.cursor(), 11);
        buf.move_left();
        assert_eq!(buf.cursor(), 10);
        buf.move_right();
        assert_eq!(buf.cursor(), 11);
    }

    #[test]
    fn test_word_movement() {
        let mut buf = InputBuffer::new();
        buf.insert_text("hello world foo");
        buf.move_home();
        buf.move_word_right();
        // Should be at position after "hello" + space = 6
        assert!(buf.cursor() >= 5);
    }

    #[test]
    fn test_selection() {
        let mut buf = InputBuffer::new();
        buf.insert_text("hello");
        buf.move_home();
        buf.select_right();
        buf.select_right();
        buf.select_right();
        assert_eq!(buf.selected_text(), Some("hel"));
    }

    #[test]
    fn test_select_all_and_replace() {
        let mut buf = InputBuffer::new();
        buf.insert_text("hello");
        buf.select_all();
        buf.insert_text("world");
        assert_eq!(buf.text(), "world");
    }

    #[test]
    fn test_undo_redo() {
        let mut buf = InputBuffer::new();
        buf.insert_text("hello");
        buf.insert_text(" world");
        assert_eq!(buf.text(), "hello world");
        buf.undo();
        assert_eq!(buf.text(), "hello");
        buf.redo();
        assert_eq!(buf.text(), "hello world");
    }

    #[test]
    fn test_history() {
        let mut buf = InputBuffer::new();
        buf.insert_text("first");
        buf.submit();
        buf.insert_text("second");
        buf.submit();

        buf.history_prev();
        assert_eq!(buf.text(), "second");
        buf.history_prev();
        assert_eq!(buf.text(), "first");
        buf.history_next();
        assert_eq!(buf.text(), "second");
        buf.history_next();
        assert_eq!(buf.text(), ""); // Back to draft
    }

    #[test]
    fn test_ghost_text_accept() {
        let mut buf = InputBuffer::new();
        buf.insert_text("git ch");
        buf.ghost_text = Some("eckout main".to_string());
        buf.accept_ghost_text();
        assert_eq!(buf.text(), "git checkout main");
        assert!(buf.ghost_text.is_none());
    }

    #[test]
    fn test_ghost_text_word_accept() {
        let mut buf = InputBuffer::new();
        buf.insert_text("git ");
        buf.ghost_text = Some("checkout main".to_string());
        buf.move_word_right();
        assert_eq!(buf.text(), "git checkout ");
        assert_eq!(buf.ghost_text.as_deref(), Some("main"));
    }

    #[test]
    fn test_copy_empty_is_interrupt() {
        let buf = InputBuffer::new();
        assert_eq!(buf.copy(), InputAction::Interrupt);
    }

    #[test]
    fn test_cut() {
        let mut buf = InputBuffer::new();
        buf.insert_text("hello world");
        buf.move_home();
        for _ in 0..5 {
            buf.select_right();
        }
        let action = buf.cut();
        assert!(matches!(action, InputAction::Cut(ref s) if s == "hello"));
        assert_eq!(buf.text(), " world");
    }

    #[test]
    fn test_word_select() {
        let mut buf = InputBuffer::new();
        buf.insert_text("hello world");
        buf.select_word_at(2); // double-click on "hello"
        assert_eq!(buf.selected_text(), Some("hello"));
    }

    #[test]
    fn test_delete_word_back() {
        let mut buf = InputBuffer::new();
        buf.insert_text("hello world");
        buf.delete_word_back();
        assert_eq!(buf.text(), "hello ");
    }

    #[test]
    fn test_insert_at_cursor_middle() {
        let mut buf = InputBuffer::new();
        buf.insert_text("helo");
        buf.move_left(); // cursor after "hel"
        buf.move_left();
        buf.insert_char('l');
        assert_eq!(buf.text(), "hello");
    }
}
