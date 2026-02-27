use anyhow::{Context, Result};
use portable_pty::{CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::sync::mpsc;
use ridgeback_config::Profile;

/// Manages a PTY (pseudo-terminal) session.
pub struct PtySession {
    master_writer: Arc<Mutex<Box<dyn Write + Send>>>,
    _master: Box<dyn portable_pty::MasterPty + Send>,
    _child: Box<dyn portable_pty::Child + Send + Sync>,
    pub cols: u16,
    pub rows: u16,
}

/// Messages from the PTY reader thread.
pub enum PtyOutput {
    /// New data read from the PTY.
    Data(Vec<u8>),
    /// The child process has exited.
    Exited(Option<portable_pty::ExitStatus>),
    /// An error occurred while reading.
    Error(String),
}

impl PtySession {
    /// Spawn a new PTY session from a profile.
    pub fn spawn(
        profile: &Profile,
        rows: u16,
        cols: u16,
    ) -> Result<(Self, mpsc::Receiver<PtyOutput>)> {
        let pty_system = portable_pty::native_pty_system();

        let pty_size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };

        let pair = pty_system
            .openpty(pty_size)
            .context("Failed to open PTY")?;

        let mut cmd = CommandBuilder::new(&profile.shell);
        for arg in &profile.args {
            cmd.arg(arg);
        }

        // Set working directory
        let working_dir = if profile.working_directory.to_str() == Some("~") {
            directories::UserDirs::new()
                .map(|d: _| d.home_dir().to_path_buf())
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
        } else if profile.working_directory.exists() {
            profile.working_directory.clone()
        } else {
            std::env::current_dir().unwrap_or_default()
        };
        cmd.cwd(&working_dir);

        tracing::info!("Spawning: {:?} {:?}", profile.shell, profile.args);

        // Reader and writer must be obtained before spawning on Windows ConPTY
        let child = pair.slave.spawn_command(cmd)
            .context("Failed to spawn shell process")?;

        let mut reader = pair.master.try_clone_reader()
            .context("Failed to clone PTY reader")?;

        let writer = pair.master.take_writer()
            .context("Failed to get PTY writer")?;

        let master_writer = Arc::new(Mutex::new(writer));

        let (tx, rx) = mpsc::channel::<PtyOutput>();

        std::thread::Builder::new()
            .name("pty-reader".to_string())
            .spawn(move || {
                let mut buf = [0u8; 8192];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => {
                            // On Windows ConPTY, Ok(0) is not EOF — just no data yet. Retry.
                            std::thread::sleep(std::time::Duration::from_millis(5));
                        }
                        Ok(n) => {
                            if tx.send(PtyOutput::Data(buf[..n].to_vec())).is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            tracing::info!("PTY reader closed: {}", e);
                            let _ = tx.send(PtyOutput::Exited(None));
                            break;
                        }
                    }
                }
            })?;

        let session = PtySession {
            master_writer,
            _master: pair.master,
            _child: child,
            cols,
            rows,
        };

        Ok((session, rx))
    }

    /// Write bytes to the PTY (e.g., user input).
    pub fn write(&mut self, data: &[u8]) -> Result<()> {
        let mut w = self.master_writer.lock().map_err(|_| anyhow::anyhow!("PTY writer lock poisoned"))?;
        w.write_all(data).context("Failed to write to PTY")?;
        w.flush().context("Failed to flush PTY")?;
        Ok(())
    }

    /// Write a string to the PTY.
    pub fn write_str(&mut self, s: &str) -> Result<()> {
        self.write(s.as_bytes())
    }

    /// Resize the PTY.
    pub fn resize(&mut self, rows: u16, cols: u16) -> Result<()> {
        self.rows = rows;
        self.cols = cols;
        Ok(())
    }
}
