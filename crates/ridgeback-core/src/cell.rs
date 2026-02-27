use serde::{Deserialize, Serialize};

/// A single cell in the terminal grid.
#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    pub ch: char,
    pub attrs: CellAttributes,
}

/// Visual attributes for a terminal cell.
#[derive(Debug, Clone, PartialEq)]
pub struct CellAttributes {
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub dim: bool,
    pub inverse: bool,
    pub hidden: bool,
    pub blink: bool,
}

/// Terminal color representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Color {
    Default,
    /// ANSI indexed color (0-255).
    Indexed(u8),
    /// True color RGB.
    Rgb(u8, u8, u8),
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            attrs: CellAttributes::default(),
        }
    }
}

impl Cell {
    pub fn new(ch: char) -> Self {
        Self {
            ch,
            attrs: CellAttributes::default(),
        }
    }

    pub fn with_attrs(ch: char, attrs: CellAttributes) -> Self {
        Self { ch, attrs }
    }

    pub fn is_empty(&self) -> bool {
        self.ch == ' ' && self.attrs == CellAttributes::default()
    }

    pub fn clear(&mut self) {
        self.ch = ' ';
        self.attrs = CellAttributes::default();
    }
}

impl Default for CellAttributes {
    fn default() -> Self {
        Self {
            fg: Color::Default,
            bg: Color::Default,
            bold: false,
            italic: false,
            underline: false,
            strikethrough: false,
            dim: false,
            inverse: false,
            hidden: false,
            blink: false,
        }
    }
}

impl Color {
    /// Map a standard ANSI foreground code (30-37, 90-97) to indexed color.
    pub fn from_ansi_fg(code: u16) -> Option<Self> {
        match code {
            30..=37 => Some(Color::Indexed((code - 30) as u8)),
            90..=97 => Some(Color::Indexed((code - 90 + 8) as u8)),
            39 => Some(Color::Default),
            _ => None,
        }
    }

    /// Map a standard ANSI background code (40-47, 100-107) to indexed color.
    pub fn from_ansi_bg(code: u16) -> Option<Self> {
        match code {
            40..=47 => Some(Color::Indexed((code - 40) as u8)),
            100..=107 => Some(Color::Indexed((code - 100 + 8) as u8)),
            49 => Some(Color::Default),
            _ => None,
        }
    }
}
