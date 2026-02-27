use anyhow::Result;
use ridgeback_config::{Profile, ShellType};
use std::sync::mpsc;

use crate::input_buffer::{InputBuffer, InputAction};
use crate::vt_handler::VtHandler;
use crate::pty::{PtySession, PtyOutput};
use crate::search::{self, SearchMatch, SearchOptions};

/// Events emitted by a Terminal to the UI layer.
#[derive(Debug)]
pub enum TerminalEvent {
    /// Terminal content changed — repaint needed.
    ContentChanged,
    /// Title changed via OSC sequence.
    TitleChanged(String),
    /// The shell process has exited.
    ProcessExited,
}

/// A full terminal session: PTY + VT state + input buffer.
pub struct Terminal {
    pub vt: VtHandler,
    pub input: InputBuffer,
    pub pty: Option<PtySession>,
    pty_rx: Option<mpsc::Receiver<PtyOutput>>,
    vt_parser: vte::Parser,
    pub profile_name: String,
    pub shell_type: ShellType,
    pub exited: bool,
}

impl Terminal {
    /// Create a new terminal session from a profile.
    pub fn spawn(profile_name: &str, profile: &Profile, rows: u16, cols: u16) -> Result<Self> {
        let mut vt = VtHandler::new(rows as usize, cols as usize, profile.scrollback_limit);
        vt.cursor_style = profile.cursor_style;
        let (pty, pty_rx) = PtySession::spawn(profile, rows, cols)?;

        Ok(Self {
            vt,
            input: InputBuffer::new(),
            pty: Some(pty),
            pty_rx: Some(pty_rx),
            vt_parser: vte::Parser::new(),
            profile_name: profile_name.to_string(),
            shell_type: profile.shell_type,
            exited: false,
        })
    }

    /// Process any pending PTY output. Returns true if content changed.
    pub fn process_pty_output(&mut self) -> bool {
        let mut changed = false;

        if let Some(ref mut rx) = self.pty_rx {
            // Drain all available messages without blocking
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    PtyOutput::Data(data) => {
                        for &byte in &data {
                            self.vt_parser.advance(&mut self.vt, byte);
                        }
                        changed = true;
                    }
                    PtyOutput::Exited(_status) => {
                        tracing::info!("PTY process exited");
                        self.exited = true;
                        changed = true;
                    }
                    PtyOutput::Error(err) => {
                        tracing::error!("PTY read error: {}", err);
                        self.exited = true;
                        changed = true;
                    }
                }
            }
        }

        changed
    }

    /// Write user input to the PTY.
    pub fn write_to_pty(&mut self, data: &[u8]) -> Result<()> {
        if self.exited {
            return Err(anyhow::anyhow!("Terminal has exited"));
        }
        if let Some(ref mut pty) = self.pty {
            pty.write(data)?;
        }
        Ok(())
    }

    /// Submit the input buffer content to the PTY.
    pub fn submit_input(&mut self) -> Result<()> {
        let action = self.input.submit();
        if let InputAction::Submit(text) = action {
            self.write_to_pty(text.as_bytes())?;
            self.write_to_pty(b"\r")?;
        }
        Ok(())
    }

    /// Send Ctrl+C (SIGINT) to the PTY.
    pub fn send_interrupt(&mut self) -> Result<()> {
        self.write_to_pty(&[0x03])?; // ETX
        Ok(())
    }

    /// Get visible lines as strings (for simple text rendering).
    pub fn visible_lines(&self) -> Vec<String> {
        self.vt.visible_lines()
    }

    /// Get the terminal title (set by shell via OSC).
    pub fn title(&self) -> Option<&str> {
        self.vt.title.as_deref()
    }

    /// Get the display title for the tab.
    pub fn tab_title(&self) -> String {
        self.vt
            .title
            .clone()
            .unwrap_or_else(|| self.profile_name.clone())
    }

    /// Resize the terminal.
    pub fn resize(&mut self, rows: u16, cols: u16) {
        self.vt.resize(rows as usize, cols as usize);
        if let Some(ref mut pty) = self.pty {
            let _ = pty.resize(rows, cols);
        }
    }

    // ── Plugin API / Query interface ───────────────────────────────────

    /// Get the last N lines from scrollback.
    pub fn last_n_lines(&self, n: usize) -> Vec<String> {
        self.vt.scrollback.last_n_lines(n)
    }

    /// Get the full log (all scrollback + visible).
    pub fn full_log(&self) -> String {
        let mut lines = self.vt.scrollback.all_lines_as_strings();
        lines.extend(self.vt.visible_lines());
        lines.join("\n")
    }

    /// Search the terminal buffer.
    pub fn search(&self, options: &SearchOptions) -> Vec<SearchMatch> {
        search::search(&self.vt.scrollback, &self.vt.grid, options)
    }
}
