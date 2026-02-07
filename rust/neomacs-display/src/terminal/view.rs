//! TerminalView: manages a single terminal instance (Term + PTY).
//!
//! Each TerminalView wraps an `alacritty_terminal::Term`, spawns a PTY
//! child process (shell), and runs a reader thread to feed PTY output
//! into the terminal state.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use parking_lot::FairMutex;

use alacritty_terminal::event::{Event as TermEvent, EventListener, OnResize, WindowSize};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::Column;
use alacritty_terminal::term::{Config as TermConfig, Term};
use alacritty_terminal::tty;
use alacritty_terminal::tty::EventedReadWrite;
use alacritty_terminal::vte::ansi;

use super::content::TerminalContent;
use super::{TerminalId, TerminalMode};

/// Grid dimensions for Term::new() and Term::resize().
///
/// alacritty_terminal's `WindowSize` doesn't implement `Dimensions`,
/// so we provide our own wrapper.
struct TermGridSize {
    columns: usize,
    screen_lines: usize,
}

impl TermGridSize {
    fn new(cols: u16, rows: u16) -> Self {
        Self {
            columns: cols as usize,
            screen_lines: rows as usize,
        }
    }
}

impl Dimensions for TermGridSize {
    fn total_lines(&self) -> usize {
        self.screen_lines
    }

    fn screen_lines(&self) -> usize {
        self.screen_lines
    }

    fn columns(&self) -> usize {
        self.columns
    }
}

/// Event listener that bridges alacritty events to neomacs.
#[derive(Clone)]
pub struct NeomacsEventProxy {
    id: TerminalId,
    /// Signals that the terminal has new content to render.
    wakeup: Arc<std::sync::atomic::AtomicBool>,
}

impl NeomacsEventProxy {
    fn new(id: TerminalId) -> Self {
        Self {
            id,
            wakeup: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Check and clear the wakeup flag.
    pub fn take_wakeup(&self) -> bool {
        self.wakeup.swap(false, std::sync::atomic::Ordering::Relaxed)
    }

    /// Check if wakeup is pending without consuming it.
    pub fn peek_wakeup(&self) -> bool {
        self.wakeup.load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl EventListener for NeomacsEventProxy {
    fn send_event(&self, event: TermEvent) {
        match event {
            TermEvent::Wakeup => {
                self.wakeup.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            TermEvent::Title(title) => {
                log::debug!("Terminal {}: title changed to '{}'", self.id, title);
            }
            TermEvent::Bell => {
                log::debug!("Terminal {}: bell", self.id);
            }
            TermEvent::Exit => {
                log::info!("Terminal {}: child process exited", self.id);
            }
            _ => {}
        }
    }
}

/// A single terminal instance.
pub struct TerminalView {
    pub id: TerminalId,
    pub mode: TerminalMode,
    /// The terminal state (shared with PTY reader).
    pub term: Arc<FairMutex<Term<NeomacsEventProxy>>>,
    /// Event proxy for wakeup notifications.
    pub event_proxy: NeomacsEventProxy,
    /// PTY handle - MUST be kept alive to prevent SIGHUP to child shell.
    /// Also used for on_resize() to send TIOCSWINSZ to the child.
    pty: tty::Pty,
    /// PTY master (for writing input to the shell).
    pty_writer: Box<dyn Write + Send>,
    /// Reader thread handle.
    _reader_thread: Option<JoinHandle<()>>,
    /// Cached content from last extraction.
    pub last_content: Option<TerminalContent>,
    /// Whether content changed since last render.
    pub dirty: bool,
    /// Floating position (only used in Floating mode).
    pub float_x: f32,
    pub float_y: f32,
    pub float_opacity: f32,
}

impl TerminalView {
    /// Create a new terminal with the given grid dimensions.
    pub fn new(
        id: TerminalId,
        cols: u16,
        rows: u16,
        mode: TerminalMode,
        shell: Option<&str>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let event_proxy = NeomacsEventProxy::new(id);

        // Create the terminal with our Dimensions-compatible size
        let config = TermConfig::default();
        let grid_size = TermGridSize::new(cols, rows);

        let term = Term::new(config, &grid_size, event_proxy.clone());
        let term = Arc::new(FairMutex::new(term));

        // Create PTY and spawn shell (tty::new needs WindowSize)
        let window_size = WindowSize {
            num_cols: cols,
            num_lines: rows,
            cell_width: 8,
            cell_height: 16,
        };

        let mut pty_config = tty::Options::default();
        if let Some(shell_path) = shell {
            pty_config.shell = Some(alacritty_terminal::tty::Shell::new(
                shell_path.to_string(),
                vec![],
            ));
        }

        // Ensure TERM is set for the child shell process.
        // In neomacs, the display backend is GPU-based so TERM is typically unset.
        // alacritty_terminal's child inherits the parent's TERM.
        if std::env::var("TERM").unwrap_or_default().is_empty() {
            std::env::set_var("TERM", "xterm-256color");
        }

        let mut pty = tty::new(&pty_config, window_size, 0)
            .map_err(|e| format!("Failed to create PTY: {}", e))?;

        // Clone file handles for concurrent read/write from separate threads.
        // Both reader() and writer() return &mut File to the same PTY master fd;
        // try_clone() calls dup(2) to get independent file descriptors.
        let pty_read_file = pty.reader().try_clone()
            .map_err(|e| format!("Failed to clone PTY reader: {}", e))?;
        let pty_write_file = pty.writer().try_clone()
            .map_err(|e| format!("Failed to clone PTY writer: {}", e))?;

        // Spawn reader thread: reads from PTY, feeds into term via ansi::Processor
        let term_clone = Arc::clone(&term);
        let proxy_clone = event_proxy.clone();
        let reader_thread = thread::Builder::new()
            .name(format!("neo-term-{}-pty", id))
            .spawn(move || {
                let mut reader = pty_read_file;
                let mut processor: ansi::Processor = ansi::Processor::new();
                let mut buf = [0u8; 4096];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => {
                            // PTY closed (child exited)
                            proxy_clone.send_event(TermEvent::Exit);
                            break;
                        }
                        Ok(n) => {
                            let mut term = term_clone.lock();
                            processor.advance(&mut *term, &buf[..n]);
                            // Signal that content changed
                            proxy_clone.send_event(TermEvent::Wakeup);
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => {
                            continue;
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            // Non-blocking fd, wait and retry
                            std::thread::sleep(std::time::Duration::from_millis(10));
                            continue;
                        }
                        Err(e) => {
                            log::warn!("Terminal {} PTY read error: {}", id, e);
                            break;
                        }
                    }
                }
            })?;

        Ok(Self {
            id,
            mode,
            term,
            event_proxy,
            pty,
            pty_writer: Box::new(pty_write_file),
            _reader_thread: Some(reader_thread),
            last_content: None,
            dirty: true,
            float_x: 0.0,
            float_y: 0.0,
            float_opacity: 1.0,
        })
    }

    /// Write input data to the terminal's PTY (keyboard input from user).
    pub fn write(&mut self, data: &[u8]) -> std::io::Result<()> {
        self.pty_writer.write_all(data)?;
        self.pty_writer.flush()
    }

    /// Resize the terminal grid and PTY.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        let grid_size = TermGridSize::new(cols, rows);
        let mut term = self.term.lock();
        term.resize(grid_size);
        drop(term);

        // Send TIOCSWINSZ to the PTY so the child process gets SIGWINCH
        let window_size = WindowSize {
            num_cols: cols,
            num_lines: rows,
            cell_width: 8,
            cell_height: 16,
        };
        self.pty.on_resize(window_size);
        self.dirty = true;
    }

    /// Extract current content for rendering. Returns true if content changed.
    pub fn update_content(&mut self) -> bool {
        if self.event_proxy.take_wakeup() || self.dirty {
            let term = self.term.lock();
            self.last_content = Some(TerminalContent::from_term(&*term));
            self.dirty = false;
            true
        } else {
            false
        }
    }

    /// Get the last extracted content.
    pub fn content(&self) -> Option<&TerminalContent> {
        self.last_content.as_ref()
    }

    /// Extract text from a region of the terminal.
    pub fn get_text(&self, start_row: usize, start_col: usize,
                    end_row: usize, end_col: usize) -> String {
        let term = self.term.lock();
        super::content::extract_text(&*term, start_row, start_col, end_row, end_col)
    }

    /// Get all visible text.
    pub fn get_visible_text(&self) -> String {
        let term = self.term.lock();
        let grid = term.grid();
        let cols = grid.columns();
        let rows = grid.screen_lines();
        super::content::extract_text(&*term, 0, 0, rows.saturating_sub(1), cols.saturating_sub(1))
    }
}

/// Manages all terminal instances.
pub struct TerminalManager {
    pub terminals: HashMap<TerminalId, TerminalView>,
    next_id: TerminalId,
}

impl TerminalManager {
    pub fn new() -> Self {
        Self {
            terminals: HashMap::new(),
            next_id: 1,
        }
    }

    /// Create a new terminal and return its ID.
    pub fn create(
        &mut self,
        cols: u16,
        rows: u16,
        mode: TerminalMode,
        shell: Option<&str>,
    ) -> Result<TerminalId, Box<dyn std::error::Error>> {
        let id = self.next_id;
        self.next_id += 1;
        let view = TerminalView::new(id, cols, rows, mode, shell)?;
        self.terminals.insert(id, view);
        Ok(id)
    }

    /// Destroy a terminal.
    pub fn destroy(&mut self, id: TerminalId) -> bool {
        self.terminals.remove(&id).is_some()
    }

    /// Get a terminal by ID.
    pub fn get(&self, id: TerminalId) -> Option<&TerminalView> {
        self.terminals.get(&id)
    }

    /// Get a mutable terminal by ID.
    pub fn get_mut(&mut self, id: TerminalId) -> Option<&mut TerminalView> {
        self.terminals.get_mut(&id)
    }

    /// Update all terminals (extract content if changed). Returns IDs that changed.
    pub fn update_all(&mut self) -> Vec<TerminalId> {
        let mut changed = Vec::new();
        for (id, view) in &mut self.terminals {
            if view.update_content() {
                changed.push(*id);
            }
        }
        changed
    }

    /// Get all terminal IDs.
    pub fn ids(&self) -> Vec<TerminalId> {
        self.terminals.keys().copied().collect()
    }

    /// Number of active terminals.
    pub fn len(&self) -> usize {
        self.terminals.len()
    }

    pub fn is_empty(&self) -> bool {
        self.terminals.is_empty()
    }
}

impl Default for TerminalManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alacritty_pty_explicit_cmd() {
        use std::io::Read;

        let ws = WindowSize { num_cols: 80, num_lines: 24, cell_width: 8, cell_height: 16 };
        let mut opts = tty::Options::default();
        opts.shell = Some(alacritty_terminal::tty::Shell::new(
            "/bin/sh".to_string(),
            vec!["-c".to_string(), "echo ALACRITTY_PTY_OK; sleep 1".to_string()],
        ));

        let mut pty = tty::new(&opts, ws, 0).expect("create pty");
        let mut reader = pty.reader().try_clone().expect("clone");
        let mut buf = [0u8; 4096];
        std::thread::sleep(std::time::Duration::from_millis(500));

        match reader.read(&mut buf) {
            Ok(n) if n > 0 => {
                let output = String::from_utf8_lossy(&buf[..n]);
                assert!(output.contains("ALACRITTY_PTY_OK"));
            }
            Ok(_) => panic!("EOF"),
            Err(e) => panic!("Read error: {}", e),
        }
    }
}
