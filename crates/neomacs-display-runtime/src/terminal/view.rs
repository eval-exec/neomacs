//! TerminalView: manages a single terminal instance (Crosswords + PTY).
//!
//! Each TerminalView wraps a `rio_vt::crosswords::Crosswords`, spawns a PTY
//! child process (shell) via `portable-pty`, and runs a reader thread
//! to feed PTY output into the terminal state.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use parking_lot::{FairMutex, Mutex};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};

use rio_vt::ansi::CursorShape;
use rio_vt::crosswords::{Crosswords, CrosswordsSize};
use rio_vt::event::{EventListener, RioEvent, WindowId};
use rio_vt::performer::handler::Processor;

use super::content::TerminalContent;
use super::{TerminalDisplayTarget, TerminalGridSize, TerminalId};

/// Scrollback history limit, matching the previous emulator default.
const SCROLLBACK_HISTORY: usize = 10_000;

/// Event listener that bridges rio-vt events to neomacs.
#[derive(Clone)]
pub struct NeomacsEventProxy {
    id: TerminalId,
    /// Signals that the terminal has new content to render.
    wakeup: Arc<std::sync::atomic::AtomicBool>,
    /// Signals that the terminal child process has exited.
    exited: Arc<std::sync::atomic::AtomicBool>,
    /// Replies the terminal wants written back to the PTY (DA/DSR/CPR).
    pending_writes: Arc<Mutex<Vec<u8>>>,
    /// Latest title not yet forwarded to the evaluator.
    pending_title: Arc<Mutex<Option<String>>>,
}

impl NeomacsEventProxy {
    fn new(id: TerminalId) -> Self {
        Self {
            id,
            wakeup: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            exited: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            pending_writes: Arc::new(Mutex::new(Vec::new())),
            pending_title: Arc::new(Mutex::new(None)),
        }
    }

    /// Check and clear the wakeup flag.
    pub fn take_wakeup(&self) -> bool {
        self.wakeup
            .swap(false, std::sync::atomic::Ordering::Relaxed)
    }

    /// Check if wakeup is pending without consuming it.
    pub fn peek_wakeup(&self) -> bool {
        self.wakeup.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Check if the terminal child process has exited.
    pub fn is_exited(&self) -> bool {
        self.exited.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn notify_wakeup(&self) {
        self.wakeup
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    fn notify_exit(&self) {
        tracing::info!("Terminal {}: child process exited", self.id);
        self.exited
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Drain replies queued for write-back to the PTY.
    fn take_pending_writes(&self) -> Vec<u8> {
        std::mem::take(&mut *self.pending_writes.lock())
    }

    pub fn take_title(&self) -> Option<String> {
        self.pending_title.lock().take()
    }
}

impl EventListener for NeomacsEventProxy {
    fn event(&self) -> (Option<RioEvent>, bool) {
        (None, false)
    }

    fn send_event(&self, event: RioEvent, _window_id: WindowId) {
        match event {
            RioEvent::PtyWrite(_route, text) => {
                self.pending_writes
                    .lock()
                    .extend_from_slice(text.as_bytes());
            }
            RioEvent::Title(title) => {
                tracing::debug!("Terminal {}: title changed to '{}'", self.id, title);
                *self.pending_title.lock() = Some(title);
                self.notify_wakeup();
            }
            RioEvent::Bell => {
                tracing::debug!("Terminal {}: bell", self.id);
            }
            _ => {}
        }
    }
}

/// A single terminal instance.
pub struct TerminalView {
    pub id: TerminalId,
    pub target: TerminalDisplayTarget,
    /// The terminal state (shared with PTY reader).
    pub term: Arc<FairMutex<Crosswords<NeomacsEventProxy>>>,
    /// Event proxy for wakeup notifications.
    pub event_proxy: NeomacsEventProxy,
    /// Owns every operating-system resource backing this terminal. Keeping
    /// the child, master handles, and reader join handle in one object makes
    /// an incomplete teardown impossible through the normal API.
    pty_session: PtySession,
    /// Cached content from last extraction.
    pub last_content: Option<TerminalContent>,
    /// Whether content changed since last render.
    pub dirty: bool,
    /// Whether the Emacs side has been notified about process exit.
    pub exit_notified: bool,
    /// Floating position (only used in Floating mode).
    pub float_x: f32,
    pub float_y: f32,
    pub float_opacity: f32,
}

struct PtySession {
    master: Option<Box<dyn MasterPty + Send>>,
    child: Option<Box<dyn Child + Send + Sync>>,
    writer: Option<Arc<Mutex<Box<dyn Write + Send>>>>,
    reader_thread: Option<JoinHandle<()>>,
}

#[derive(Debug, thiserror::Error)]
pub enum TerminalShutdownError {
    #[error("failed to reap terminal child: {0}")]
    Reap(std::io::Error),
    #[error("terminal reader thread panicked during shutdown")]
    ReaderPanicked,
}

impl PtySession {
    #[cfg(test)]
    fn process_id(&self) -> Option<u32> {
        self.child.as_ref().and_then(|child| child.process_id())
    }

    fn write(&self, data: &[u8]) -> std::io::Result<()> {
        let writer = self.writer.as_ref().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::BrokenPipe, "terminal is shut down")
        })?;
        let mut writer = writer.lock();
        writer.write_all(data)?;
        writer.flush()
    }

    fn resize(&self, size: PtySize) -> std::io::Result<()> {
        self.master
            .as_ref()
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::BrokenPipe, "terminal is shut down")
            })?
            .resize(size)
            .map_err(|error| std::io::Error::other(error.to_string()))
    }

    fn shutdown(&mut self) -> Result<(), TerminalShutdownError> {
        let reap_result = if let Some(mut child) = self.child.take() {
            match child.try_wait() {
                Ok(Some(_)) => Ok(()),
                Ok(None) | Err(_) => {
                    // A failed kill can race with natural exit. `wait` is the
                    // authoritative operation: it both observes that race and
                    // guarantees the child is reaped before teardown returns.
                    let _ = child.kill();
                    child
                        .wait()
                        .map(|_| ())
                        .map_err(TerminalShutdownError::Reap)
                }
            }
        } else {
            Ok(())
        };

        // Close the render thread's master/writer handles before joining. The
        // reader owns its cloned handles until the killed slave side reaches
        // EOF/EIO and the reader exits.
        self.writer.take();
        self.master.take();
        let join_result = self
            .reader_thread
            .take()
            .map(JoinHandle::join)
            .transpose()
            .map(|_| ())
            .map_err(|_| TerminalShutdownError::ReaderPanicked);

        reap_result.and(join_result)
    }
}

impl Drop for PtySession {
    fn drop(&mut self) {
        if let Err(error) = self.shutdown() {
            tracing::warn!("Terminal PTY shutdown during drop failed: {error}");
        }
    }
}

impl TerminalView {
    /// Create a new terminal with the given grid dimensions.
    pub fn new(
        id: TerminalId,
        size: TerminalGridSize,
        target: TerminalDisplayTarget,
        shell: Option<&str>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let event_proxy = NeomacsEventProxy::new(id);
        let cols = size.cols.get();
        let rows = size.rows.get();

        let grid_size = CrosswordsSize::new(cols.max(1) as usize, rows.max(1) as usize);
        let term = Crosswords::new(
            grid_size,
            CursorShape::Block,
            event_proxy.clone(),
            WindowId::from(0),
            0,
            SCROLLBACK_HISTORY,
        );
        let term = Arc::new(FairMutex::new(term));

        // Create PTY and spawn shell using portable-pty.
        let pty_system = native_pty_system();
        let pty_size = PtySize {
            rows,
            cols,
            pixel_width: 8u16.saturating_mul(cols),
            pixel_height: 16u16.saturating_mul(rows),
        };
        let pty_pair = pty_system
            .openpty(pty_size)
            .map_err(|e| format!("Failed to create PTY: {}", e))?;

        let mut cmd = if let Some(shell_path) = shell {
            CommandBuilder::new(shell_path)
        } else {
            CommandBuilder::new_default_prog()
        };

        // Ensure TERM is set for the child shell process.
        // In neomacs, the display backend is GPU-based so TERM is typically unset.
        let term_env = std::env::var("TERM").unwrap_or_else(|_| "xterm-256color".to_string());
        if term_env.is_empty() {
            cmd.env("TERM", "xterm-256color");
        } else {
            cmd.env("TERM", &term_env);
        }

        let pty_child = pty_pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("Failed to spawn PTY child: {}", e))?;

        // Split independent read/write handles for the reader thread and input writes.
        let pty_read_file = pty_pair
            .master
            .try_clone_reader()
            .map_err(|e| format!("Failed to clone PTY reader: {}", e))?;
        let pty_write_file = pty_pair
            .master
            .take_writer()
            .map_err(|e| format!("Failed to take PTY writer: {}", e))?;

        let pty_writer: Arc<Mutex<Box<dyn Write + Send>>> =
            Arc::new(Mutex::new(Box::new(pty_write_file)));

        // Spawn reader thread: reads from PTY, feeds into term via Processor
        let term_clone = Arc::clone(&term);
        let proxy_clone = event_proxy.clone();
        let writer_clone = Arc::clone(&pty_writer);
        let reader_thread = thread::Builder::new()
            .name(format!("neo-term-{}-pty", id))
            .spawn(move || {
                let mut reader = pty_read_file;
                let mut processor = Processor::default();
                let mut buf = [0u8; 4096];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => {
                            // PTY closed (child exited)
                            proxy_clone.notify_exit();
                            break;
                        }
                        Ok(n) => {
                            {
                                let mut term = term_clone.lock();
                                processor.advance(&mut *term, &buf[..n]);
                            }
                            // Answer the terminal's queries (DA/DSR/CPR)
                            let replies = proxy_clone.take_pending_writes();
                            if !replies.is_empty() {
                                let mut writer = writer_clone.lock();
                                if let Err(e) =
                                    writer.write_all(&replies).and_then(|_| writer.flush())
                                {
                                    tracing::warn!("Terminal {} PTY reply write failed: {}", id, e);
                                }
                            }
                            // Signal that content changed
                            proxy_clone.notify_wakeup();
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
                            // Treat a dead PTY like an exited child so the
                            // render thread reports it to Emacs.
                            tracing::warn!("Terminal {} PTY read error: {}", id, e);
                            proxy_clone.notify_exit();
                            break;
                        }
                    }
                }
            })?;

        Ok(Self {
            id,
            target,
            term,
            event_proxy,
            pty_session: PtySession {
                master: Some(pty_pair.master),
                child: Some(pty_child),
                writer: Some(pty_writer),
                reader_thread: Some(reader_thread),
            },
            last_content: None,
            dirty: true,
            exit_notified: false,
            float_x: 0.0,
            float_y: 0.0,
            float_opacity: 1.0,
        })
    }

    /// Write input data to the terminal's PTY (keyboard input from user).
    pub fn write(&mut self, data: &[u8]) -> std::io::Result<()> {
        self.pty_session.write(data)
    }

    /// Resize the terminal grid and PTY.
    pub fn resize(&mut self, size: TerminalGridSize) {
        let cols = size.cols.get();
        let rows = size.rows.get();
        let grid_size = CrosswordsSize::new(cols as usize, rows as usize);
        let mut term = self.term.lock();
        term.resize(grid_size);
        drop(term);

        let pty_size = PtySize {
            rows,
            cols,
            pixel_width: 8u16.saturating_mul(cols),
            pixel_height: 16u16.saturating_mul(rows),
        };
        if let Err(e) = self.pty_session.resize(pty_size) {
            tracing::warn!("Terminal {} PTY resize failed: {}", self.id, e);
        }
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
    pub fn get_text(
        &self,
        start_row: usize,
        start_col: usize,
        end_row: usize,
        end_col: usize,
    ) -> String {
        let term = self.term.lock();
        super::content::extract_text(&*term, start_row, start_col, end_row, end_col)
    }

    /// Get all visible text.
    pub fn get_visible_text(&self) -> String {
        let term = self.term.lock();
        let cols = term.columns();
        let rows = term.screen_lines();
        super::content::extract_text(&*term, 0, 0, rows.saturating_sub(1), cols.saturating_sub(1))
    }

    /// Terminate and reap the PTY child, close all master handles, and join
    /// the reader thread. Safe to call repeatedly.
    pub fn shutdown(&mut self) -> Result<(), TerminalShutdownError> {
        self.pty_session.shutdown()
    }

    #[cfg(test)]
    fn child_process_id(&self) -> Option<u32> {
        self.pty_session.process_id()
    }
}

/// Manages all terminal instances.
pub struct TerminalManager {
    pub terminals: HashMap<TerminalId, TerminalView>,
}

impl TerminalManager {
    pub fn new() -> Self {
        Self {
            terminals: HashMap::new(),
        }
    }

    /// Destroy a terminal.
    pub fn destroy(&mut self, id: TerminalId) -> Result<bool, TerminalShutdownError> {
        let Some(mut view) = self.terminals.remove(&id) else {
            return Ok(false);
        };
        view.shutdown()?;
        Ok(true)
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
        let mut ids: Vec<_> = self.terminals.keys().copied().collect();
        ids.sort_unstable_by_key(|id| id.get());
        ids
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
#[path = "view_test.rs"]
mod tests;
