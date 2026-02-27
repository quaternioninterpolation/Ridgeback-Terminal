use crate::cell::{Cell, CellAttributes, Color};
use crate::grid::Grid;
use crate::buffer::ScrollbackBuffer;
use crate::sixel::{SixelDecoder, SixelImage, ImageLayer};
use ridgeback_config::CursorStyle;

/// VT sequence handler — implements vte::Perform to update terminal state.
pub struct VtHandler {
    pub grid: Grid,
    pub scrollback: ScrollbackBuffer,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub saved_cursor_row: usize,
    pub saved_cursor_col: usize,
    pub current_attrs: CellAttributes,
    pub scroll_top: usize,
    pub scroll_bottom: usize,
    pub dirty: bool,
    pub dirty_lines: std::collections::HashSet<usize>,
    pub hot_cells: Vec<(usize, usize)>,
    pub title: Option<String>,
    /// Cursor style from profile (Block / Bar / Underline).
    pub cursor_style: CursorStyle,
    osc_string: String,
    sixel_decoder: Option<SixelDecoder>,
    pub image_layer: ImageLayer,
    pub pending_images: Vec<SixelImage>,
}

impl VtHandler {
    pub fn new(rows: usize, cols: usize, scrollback_capacity: usize) -> Self {
        Self {
            scroll_bottom: rows.saturating_sub(1),
            grid: Grid::new(rows, cols),
            scrollback: ScrollbackBuffer::new(scrollback_capacity),
            cursor_row: 0,
            cursor_col: 0,
            saved_cursor_row: 0,
            saved_cursor_col: 0,
            current_attrs: CellAttributes::default(),
            scroll_top: 0,
            dirty: false,
            dirty_lines: std::collections::HashSet::new(),
            hot_cells: Vec::new(),
            title: None,
            cursor_style: CursorStyle::Block,
            osc_string: String::new(),
            sixel_decoder: None,
            image_layer: ImageLayer::new(),
            pending_images: Vec::new(),
        }
    }

    pub fn rows(&self) -> usize {
        self.grid.rows()
    }

    pub fn cols(&self) -> usize {
        self.grid.cols()
    }

    pub fn resize(&mut self, rows: usize, cols: usize) {
        self.grid.resize(rows, cols);
        self.scroll_bottom = rows.saturating_sub(1);
        if self.cursor_row >= rows {
            self.cursor_row = rows.saturating_sub(1);
        }
        if self.cursor_col >= cols {
            self.cursor_col = cols.saturating_sub(1);
        }
        self.dirty = true;
    }

    /// Clear dirty state for the next frame.
    pub fn clear_dirty(&mut self) {
        self.dirty = false;
        self.dirty_lines.clear();
        self.hot_cells.clear();
    }

    fn mark_dirty(&mut self, row: usize) {
        self.dirty = true;
        self.dirty_lines.insert(row);
    }

    fn scroll_up_one(&mut self) {
        let removed = self.grid.scroll_up();
        self.scrollback.push(removed);
        self.dirty = true;
        // All lines are dirty after a scroll
        for r in 0..self.grid.rows() {
            self.dirty_lines.insert(r);
        }
    }

    fn scroll_region_up_one(&mut self) {
        // Save the top line of the scroll region to scrollback if it's the screen top
        if self.scroll_top == 0 {
            if let Some(row) = self.grid.row(0) {
                self.scrollback.push(row.to_vec());
            }
        }
        self.grid.scroll_region_up(self.scroll_top, self.scroll_bottom);
        self.dirty = true;
        for r in self.scroll_top..=self.scroll_bottom {
            self.dirty_lines.insert(r);
        }
    }

    fn scroll_region_down_one(&mut self) {
        self.grid.scroll_region_down(self.scroll_top, self.scroll_bottom);
        self.dirty = true;
        for r in self.scroll_top..=self.scroll_bottom {
            self.dirty_lines.insert(r);
        }
    }

    fn linefeed(&mut self) {
        if self.cursor_row == self.scroll_bottom {
            self.scroll_region_up_one();
        } else if self.cursor_row < self.grid.rows() - 1 {
            self.cursor_row += 1;
        }
    }

    /// Apply an SGR (Select Graphic Rendition) parameter.
    fn apply_sgr(&mut self, params: &[&[u16]]) {
        let mut iter = params.iter();
        while let Some(param) = iter.next() {
            let code = param.first().copied().unwrap_or(0);
            match code {
                0 => self.current_attrs = CellAttributes::default(),
                1 => self.current_attrs.bold = true,
                2 => self.current_attrs.dim = true,
                3 => self.current_attrs.italic = true,
                4 => self.current_attrs.underline = true,
                5 | 6 => self.current_attrs.blink = true,
                7 => self.current_attrs.inverse = true,
                8 => self.current_attrs.hidden = true,
                9 => self.current_attrs.strikethrough = true,
                22 => {
                    self.current_attrs.bold = false;
                    self.current_attrs.dim = false;
                }
                23 => self.current_attrs.italic = false,
                24 => self.current_attrs.underline = false,
                25 => self.current_attrs.blink = false,
                27 => self.current_attrs.inverse = false,
                28 => self.current_attrs.hidden = false,
                29 => self.current_attrs.strikethrough = false,
                30..=37 | 90..=97 => {
                    if let Some(c) = Color::from_ansi_fg(code) {
                        self.current_attrs.fg = c;
                    }
                }
                38 => {
                    // Extended foreground color
                    if let Some(color) = self.parse_extended_color(&mut iter, params) {
                        self.current_attrs.fg = color;
                    }
                }
                39 => self.current_attrs.fg = Color::Default,
                40..=47 | 100..=107 => {
                    if let Some(c) = Color::from_ansi_bg(code) {
                        self.current_attrs.bg = c;
                    }
                }
                48 => {
                    // Extended background color
                    if let Some(color) = self.parse_extended_color(&mut iter, params) {
                        self.current_attrs.bg = color;
                    }
                }
                49 => self.current_attrs.bg = Color::Default,
                _ => {} // Unrecognized SGR, ignore
            }
        }
    }

    fn parse_extended_color<'a>(
        &self,
        iter: &mut impl Iterator<Item = &'a &'a [u16]>,
        _params: &[&[u16]],
    ) -> Option<Color> {
        let next = iter.next()?;
        let mode = next.first().copied().unwrap_or(0);
        match mode {
            5 => {
                // 256-color: ESC[38;5;{n}m
                let idx_param = iter.next()?;
                let idx = idx_param.first().copied().unwrap_or(0);
                Some(Color::Indexed(idx as u8))
            }
            2 => {
                // True color: ESC[38;2;{r};{g};{b}m
                let r_param = iter.next()?;
                let g_param = iter.next()?;
                let b_param = iter.next()?;
                let r = r_param.first().copied().unwrap_or(0) as u8;
                let g = g_param.first().copied().unwrap_or(0) as u8;
                let b = b_param.first().copied().unwrap_or(0) as u8;
                Some(Color::Rgb(r, g, b))
            }
            _ => None,
        }
    }

    /// Get visible lines including grid content converted to strings (for simple rendering).
    pub fn visible_lines(&self) -> Vec<String> {
        (0..self.grid.rows())
            .map(|r| self.grid.row_to_string(r))
            .collect()
    }

    /// Get visible rows as cell references (for GPU rendering).
    pub fn visible_cells(&self) -> &[Vec<Cell>] {
        self.grid.visible_rows()
    }
}

impl vte::Perform for VtHandler {
    fn print(&mut self, ch: char) {
        if self.cursor_col >= self.grid.cols() {
            // Autowrap
            self.cursor_col = 0;
            self.linefeed();
        }
        if let Some(cell) = self.grid.cell_mut(self.cursor_row, self.cursor_col) {
            cell.ch = ch;
            cell.attrs = self.current_attrs.clone();
        }
        self.hot_cells.push((self.cursor_row, self.cursor_col));
        self.mark_dirty(self.cursor_row);
        self.cursor_col += 1;
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            // BEL
            0x07 => {}
            // Backspace
            0x08 => {
                if self.cursor_col > 0 {
                    self.cursor_col -= 1;
                }
            }
            // Horizontal tab
            0x09 => {
                let next_tab = ((self.cursor_col / 8) + 1) * 8;
                self.cursor_col = next_tab.min(self.grid.cols() - 1);
            }
            // Line feed, vertical tab, form feed
            0x0A | 0x0B | 0x0C => {
                self.linefeed();
            }
            // Carriage return
            0x0D => {
                self.cursor_col = 0;
            }
            _ => {}
        }
    }

    fn hook(&mut self, params: &vte::Params, _intermediates: &[u8], _ignore: bool, action: char) {
        // Sixel DCS: ESC P <params> q <sixel-data> ESC \
        // The action char is 'q' for sixel
        if action == 'q' {
            let decoder = SixelDecoder::new();
            // Parse P1 (pixel aspect ratio numerator), P2 (background mode), P3 (horizontal grid size)
            let mut param_iter = params.iter();
            let _p1 = param_iter.next().and_then(|p| p.first().copied()).unwrap_or(0);
            let p2 = param_iter.next().and_then(|p| p.first().copied()).unwrap_or(0);
            // p2: 0 or 2 = pixel positions are set; 1 = no background modification
            let _ = p2;
            self.sixel_decoder = Some(decoder);
        }
    }

    fn put(&mut self, byte: u8) {
        if let Some(ref mut decoder) = self.sixel_decoder {
            decoder.feed(byte);
        }
    }

    fn unhook(&mut self) {
        if let Some(decoder) = self.sixel_decoder.take() {
            if let Some(image) = decoder.finish() {
                self.pending_images.push(image);
                self.dirty = true;
            }
        }
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        if params.len() >= 2 {
            let cmd = params[0];
            if cmd == b"0" || cmd == b"2" {
                // Set window title
                if let Ok(title) = std::str::from_utf8(params[1]) {
                    self.title = Some(title.to_string());
                }
            } else if cmd == b"1337" {
                // iTerm2 inline image protocol: ESC ] 1337 ; File=<params>:<base64data> BEL
                if let Ok(payload) = std::str::from_utf8(params[1]) {
                    if let Some(rest) = payload.strip_prefix("File=") {
                        if let Some(image) = crate::sixel::decode_iterm2_image(rest) {
                            self.pending_images.push(image);
                            self.dirty = true;
                        }
                    }
                }
            }
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        let params_vec: Vec<&[u16]> = params.iter().collect();
        let param = |idx: usize, default: usize| -> usize {
            params_vec
                .get(idx)
                .and_then(|p| p.first())
                .map(|&v| if v == 0 { default } else { v as usize })
                .unwrap_or(default)
        };

        match (action, intermediates) {
            // CUU — Cursor Up
            ('A', []) => {
                let n = param(0, 1);
                self.cursor_row = self.cursor_row.saturating_sub(n);
            }
            // CUD — Cursor Down
            ('B', []) => {
                let n = param(0, 1);
                self.cursor_row = (self.cursor_row + n).min(self.grid.rows() - 1);
            }
            // CUF — Cursor Forward
            ('C', []) => {
                let n = param(0, 1);
                self.cursor_col = (self.cursor_col + n).min(self.grid.cols() - 1);
            }
            // CUB — Cursor Back
            ('D', []) => {
                let n = param(0, 1);
                self.cursor_col = self.cursor_col.saturating_sub(n);
            }
            // CNL — Cursor Next Line
            ('E', []) => {
                let n = param(0, 1);
                self.cursor_row = (self.cursor_row + n).min(self.grid.rows() - 1);
                self.cursor_col = 0;
            }
            // CPL — Cursor Previous Line
            ('F', []) => {
                let n = param(0, 1);
                self.cursor_row = self.cursor_row.saturating_sub(n);
                self.cursor_col = 0;
            }
            // CHA — Cursor Horizontal Absolute
            ('G', []) => {
                let col = param(0, 1).saturating_sub(1);
                self.cursor_col = col.min(self.grid.cols() - 1);
            }
            // CUP — Cursor Position
            ('H', []) | ('f', []) => {
                let row = param(0, 1).saturating_sub(1);
                let col = param(1, 1).saturating_sub(1);
                self.cursor_row = row.min(self.grid.rows() - 1);
                self.cursor_col = col.min(self.grid.cols() - 1);
            }
            // ED — Erase in Display
            ('J', []) => {
                let mode = param(0, 0);
                match mode {
                    0 => self.grid.erase_below(self.cursor_row, self.cursor_col),
                    1 => self.grid.erase_above(self.cursor_row, self.cursor_col),
                    2 | 3 => {
                        self.grid.clear();
                        self.cursor_row = 0;
                        self.cursor_col = 0;
                    }
                    _ => {}
                }
                self.dirty = true;
                for r in 0..self.grid.rows() {
                    self.dirty_lines.insert(r);
                }
            }
            // EL — Erase in Line
            ('K', []) => {
                let mode = param(0, 0);
                match mode {
                    0 => self.grid.erase_to_eol(self.cursor_row, self.cursor_col),
                    1 => self.grid.erase_from_bol(self.cursor_row, self.cursor_col),
                    2 => self.grid.erase_line(self.cursor_row),
                    _ => {}
                }
                self.mark_dirty(self.cursor_row);
            }
            // IL — Insert Lines
            ('L', []) => {
                let n = param(0, 1);
                for _ in 0..n {
                    self.scroll_region_down_one();
                }
            }
            // DL — Delete Lines
            ('M', []) => {
                let n = param(0, 1);
                for _ in 0..n {
                    self.scroll_region_up_one();
                }
            }
            // DCH — Delete Characters
            ('P', []) => {
                let n = param(0, 1);
                self.grid.delete_chars(self.cursor_row, self.cursor_col, n);
                self.mark_dirty(self.cursor_row);
            }
            // SU — Scroll Up
            ('S', []) => {
                let n = param(0, 1);
                for _ in 0..n {
                    self.scroll_region_up_one();
                }
            }
            // SD — Scroll Down
            ('T', []) => {
                let n = param(0, 1);
                for _ in 0..n {
                    self.scroll_region_down_one();
                }
            }
            // ICH — Insert Characters
            ('@', []) => {
                let n = param(0, 1);
                self.grid.insert_chars(self.cursor_row, self.cursor_col, n);
                self.mark_dirty(self.cursor_row);
            }
            // SGR — Select Graphic Rendition
            ('m', []) => {
                if params_vec.is_empty() {
                    self.current_attrs = CellAttributes::default();
                } else {
                    self.apply_sgr(&params_vec);
                }
            }
            // DECSTBM — Set Scrolling Region
            ('r', []) => {
                let top = param(0, 1).saturating_sub(1);
                let bottom = param(1, self.grid.rows()).saturating_sub(1);
                self.scroll_top = top.min(self.grid.rows() - 1);
                self.scroll_bottom = bottom.min(self.grid.rows() - 1);
                self.cursor_row = 0;
                self.cursor_col = 0;
            }
            // DECSC — Save Cursor (via CSI s)
            ('s', []) => {
                self.saved_cursor_row = self.cursor_row;
                self.saved_cursor_col = self.cursor_col;
            }
            // DECRC — Restore Cursor (via CSI u)
            ('u', []) => {
                self.cursor_row = self.saved_cursor_row;
                self.cursor_col = self.saved_cursor_col;
            }
            // DSR — Device Status Report
            ('n', []) => {
                // We don't respond directly here; the terminal host would need
                // to handle this by writing a response to the PTY.
            }
            // DECSET/DECRST — Private mode set/reset
            ('h', [b'?']) | ('l', [b'?']) => {
                // Common private modes we can ignore for now:
                // ?1 (application cursor keys), ?25 (cursor visibility),
                // ?1049 (alternate screen buffer), etc.
                // TODO: Implement alternate screen buffer
            }
            _ => {
                tracing::trace!(
                    "Unhandled CSI: action={}, intermediates={:?}, params={:?}",
                    action,
                    intermediates,
                    params_vec
                );
            }
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], _ignore: bool, byte: u8) {
        match (byte, intermediates) {
            // DECSC — Save Cursor
            (b'7', []) => {
                self.saved_cursor_row = self.cursor_row;
                self.saved_cursor_col = self.cursor_col;
            }
            // DECRC — Restore Cursor
            (b'8', []) => {
                self.cursor_row = self.saved_cursor_row;
                self.cursor_col = self.saved_cursor_col;
            }
            // RI — Reverse Index (scroll down if at top)
            (b'M', []) => {
                if self.cursor_row == self.scroll_top {
                    self.scroll_region_down_one();
                } else if self.cursor_row > 0 {
                    self.cursor_row -= 1;
                }
            }
            // IND — Index (scroll up if at bottom)
            (b'D', []) => {
                self.linefeed();
            }
            // NEL — Next Line
            (b'E', []) => {
                self.cursor_col = 0;
                self.linefeed();
            }
            // RIS — Reset
            (b'c', []) => {
                let rows = self.grid.rows();
                let cols = self.grid.cols();
                let cap = self.scrollback.capacity();
                *self = Self::new(rows, cols, cap);
            }
            _ => {
                tracing::trace!(
                    "Unhandled ESC: byte=0x{:02X}, intermediates={:?}",
                    byte,
                    intermediates
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process_bytes(handler: &mut VtHandler, bytes: &[u8]) {
        let mut parser = vte::Parser::new();
        for &byte in bytes {
            parser.advance(handler, byte);
        }
    }

    #[test]
    fn test_basic_print() {
        let mut handler = VtHandler::new(24, 80, 1000);
        process_bytes(&mut handler, b"Hello");
        assert_eq!(handler.grid.row_to_string(0), "Hello");
        assert_eq!(handler.cursor_col, 5);
    }

    #[test]
    fn test_cursor_movement() {
        let mut handler = VtHandler::new(24, 80, 1000);
        // Print, then move cursor back and overwrite
        process_bytes(&mut handler, b"Hello\x1b[5DWorld");
        assert_eq!(handler.grid.row_to_string(0), "World");
    }

    #[test]
    fn test_newline() {
        let mut handler = VtHandler::new(24, 80, 1000);
        process_bytes(&mut handler, b"Line1\r\nLine2");
        assert_eq!(handler.grid.row_to_string(0), "Line1");
        assert_eq!(handler.grid.row_to_string(1), "Line2");
    }

    #[test]
    fn test_ansi_color() {
        let mut handler = VtHandler::new(24, 80, 1000);
        process_bytes(&mut handler, b"\x1b[31mRed\x1b[0m");
        let cell = handler.grid.cell(0, 0).unwrap();
        assert_eq!(cell.ch, 'R');
        assert_eq!(cell.attrs.fg, Color::Indexed(1)); // Red
    }

    #[test]
    fn test_erase_in_display() {
        let mut handler = VtHandler::new(24, 80, 1000);
        process_bytes(&mut handler, b"Hello World");
        process_bytes(&mut handler, b"\x1b[2J"); // Clear screen
        assert_eq!(handler.grid.row_to_string(0), "");
    }

    #[test]
    fn test_scroll() {
        let mut handler = VtHandler::new(3, 80, 1000);
        process_bytes(&mut handler, b"Line1\r\nLine2\r\nLine3\r\nLine4");
        // Line1 should have scrolled into scrollback
        assert_eq!(handler.scrollback.len(), 1);
        assert_eq!(handler.grid.row_to_string(0), "Line2");
        assert_eq!(handler.grid.row_to_string(1), "Line3");
        assert_eq!(handler.grid.row_to_string(2), "Line4");
    }
}
