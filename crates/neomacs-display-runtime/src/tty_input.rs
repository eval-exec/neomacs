//! TTY input reader.
//!
//! GNU Emacs reads Unix TTY keyboard input as raw bytes into its own keyboard
//! buffer (`tty_read_avail_input` in src/keyboard.c).  Keep the Unix path byte
//! based for the same semantics; use crossterm's parsed events only where the
//! platform does not expose the same Unix TTY model.

pub mod terminal_size;
pub use terminal_size::{TerminalCellSize, TerminalColumns, TerminalRows, TerminalSizeQuery};

use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(not(unix))]
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;

use crate::thread_comm::{InputEvent, RenderComms};
#[cfg(not(unix))]
use crate::thread_comm::{LifecycleCommand, RenderCommand};
#[cfg(not(unix))]
use crossterm::event::Event;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

#[cfg(unix)]
const GNU_KBD_BUFFER_SIZE: usize = 4096;

#[cfg(unix)]
const TTY_READER_POLL_INTERVAL: Duration = Duration::from_millis(50);

// Modifier masks — must match crates/neomacs-display-runtime/src/backend/wgpu/events.rs
// SHIFT/CTRL/SUPER are consumed only by the non-unix crossterm key mapper below.
#[allow(dead_code)]
const NEOMACS_SHIFT_MASK: u32 = 1 << 0;
#[allow(dead_code)]
const NEOMACS_CTRL_MASK: u32 = 1 << 1;
const NEOMACS_META_MASK: u32 = 1 << 2;
#[allow(dead_code)]
const NEOMACS_SUPER_MASK: u32 = 1 << 3;

const XK_RETURN: u32 = 0xff0d;
const XK_TAB: u32 = 0xff09;
#[allow(dead_code)] // used only by the non-unix crossterm key mapper below
const XK_ESCAPE: u32 = 0xff1b;
const XK_LEFT: u32 = 0xff51;
const XK_UP: u32 = 0xff52;
const XK_RIGHT: u32 = 0xff53;
const XK_DOWN: u32 = 0xff54;

// The crossterm `KeyEvent` -> `InputEvent` mapper (map_modifiers, without_control,
// tty_control_char_keysym, map_key_event) is wired into `read_tty_events` only on
// non-unix targets; on unix it is exercised solely by the tests, hence the allows.
/// Convert crossterm modifiers to our internal modifier mask.
#[allow(dead_code)]
fn map_modifiers(mods: KeyModifiers) -> u32 {
    let mut out = 0;
    if mods.contains(KeyModifiers::SHIFT) {
        out |= NEOMACS_SHIFT_MASK;
    }
    if mods.contains(KeyModifiers::CONTROL) {
        out |= NEOMACS_CTRL_MASK;
    }
    if mods.contains(KeyModifiers::ALT) {
        out |= NEOMACS_META_MASK;
    }
    if mods.contains(KeyModifiers::SUPER) {
        out |= NEOMACS_SUPER_MASK;
    }
    out
}

#[allow(dead_code)]
fn without_control(modifiers: u32) -> u32 {
    modifiers & !NEOMACS_CTRL_MASK
}

#[allow(dead_code)]
fn tty_control_char_keysym(c: char) -> Option<u32> {
    match c {
        '@' | '2' => Some(0x00),
        '[' | '3' => Some(0x1b),
        '\\' | '4' => Some(0x1c),
        ']' | '5' => Some(0x1d),
        '^' | '6' => Some(0x1e),
        '_' | '/' | '7' => Some(0x1f),
        '?' | '8' => Some(0x7f),
        _ => None,
    }
}

/// Map a crossterm key event to an `InputEvent::Key`.
///
/// Returns `None` for modifier-only keys (Shift, Ctrl, Alt, Super,
/// CapsLock, NumLock, etc.) — those are tracked by crossterm's modifier
/// state on subsequent key events, matching how winit delivers them.
#[allow(dead_code)]
fn map_key_event(event: KeyEvent) -> Option<InputEvent> {
    // Ignore key releases — Emacs only cares about press/repeat.
    if event.kind == KeyEventKind::Release {
        return None;
    }

    let mut modifiers = map_modifiers(event.modifiers);

    let native = |keysym| Some(neovm_core::keyboard::FrontendKey::Keysym(keysym));
    let key = match event.code {
        KeyCode::Char(c)
            if event.modifiers.contains(KeyModifiers::CONTROL)
                && tty_control_char_keysym(c).is_some() =>
        {
            modifiers = without_control(modifiers);
            tty_control_char_keysym(c).map(neovm_core::keyboard::FrontendKey::Keysym)
        }
        KeyCode::Char(c) => Some(neovm_core::keyboard::FrontendKey::Character(c)),
        KeyCode::F(n) if (1..=12).contains(&n) => {
            native(0xffbe + (n as u32 - 1)) // F1=0xffbe … F12=0xffc9
        }
        KeyCode::F(_) => None, // unsupported function key
        KeyCode::Esc => native(XK_ESCAPE),
        KeyCode::Enter => native(XK_RETURN),
        KeyCode::Tab => native(XK_TAB),
        KeyCode::Backspace => native(0x7f),
        KeyCode::Delete => native(0xffff),
        KeyCode::Insert => native(0xff63),
        KeyCode::Home => native(0xff50),
        KeyCode::End => native(0xff57),
        KeyCode::PageUp => native(0xff55),
        KeyCode::PageDown => native(0xff56),
        KeyCode::Left => native(XK_LEFT),
        KeyCode::Up => native(XK_UP),
        KeyCode::Right => native(XK_RIGHT),
        KeyCode::Down => native(XK_DOWN),
        KeyCode::Null => native(0x00),
        KeyCode::CapsLock
        | KeyCode::ScrollLock
        | KeyCode::NumLock
        | KeyCode::PrintScreen
        | KeyCode::Pause
        | KeyCode::Menu
        | KeyCode::KeypadBegin
        | KeyCode::Media(_)
        | KeyCode::Modifier(_) => None, // suppress bare modifier/media keys
        KeyCode::BackTab => native(0xff09), // same as Tab, but with shift modifier
    };

    let key = key?;

    Some(InputEvent::Key {
        key,
        modifiers,
        pressed: event.kind == KeyEventKind::Press,
        emacs_frame_id: 0,
    })
}

#[cfg(unix)]
fn raw_tty_input_event(bytes: Vec<u8>) -> InputEvent {
    InputEvent::RawTtyBytes {
        bytes,
        emacs_frame_id: 0,
    }
}

#[cfg(not(unix))]
fn read_tty_events(
    tx: crossbeam_channel::Sender<InputEvent>,
    stop: Arc<AtomicBool>,
    _paused: Arc<AtomicBool>,
) {
    while !stop.load(Ordering::Relaxed) {
        match crossterm::event::poll(Duration::from_millis(50)) {
            Ok(true) => {
                match crossterm::event::read() {
                    Ok(Event::Key(key)) => {
                        if let Some(event) = map_key_event(key) {
                            tracing::debug!("tty_input: key event {:?}", event);
                            if tx.send(event).is_err() {
                                tracing::warn!("tty_input: channel closed");
                                return;
                            }
                        }
                    }
                    Ok(Event::Resize(cols, rows)) => {
                        tracing::debug!("tty_input: resize {}x{}", cols, rows);
                        let event = InputEvent::WindowResize {
                            width: cols as u32,
                            height: rows as u32,
                            scale_factor: 1.0,
                            emacs_frame_id: 0,
                        };
                        if tx.send(event).is_err() {
                            tracing::warn!("tty_input: channel closed");
                            return;
                        }
                    }
                    Ok(Event::Mouse(mouse)) => {
                        tracing::debug!("tty_input: mouse event {:?}", mouse);
                        // Mouse events are not yet wired to the evaluator;
                        // they can be added later when TTY mouse support is
                        // needed. For now we just log them.
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!("tty_input: crossterm read error: {}", e);
                        thread::sleep(Duration::from_millis(100));
                    }
                }
            }
            Ok(false) => continue,
            Err(e) => {
                tracing::warn!("tty_input: crossterm poll error: {}", e);
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// A standalone TTY input reader that publishes byte and resize facts through
/// the existing bounded RenderComms channel. Non-Unix keeps crossterm mapping.
///
/// Used by the `-nw` path when rendering goes through `TtyRif` on the
/// evaluator thread.
#[derive(Debug)]
pub struct TtyInputReader {
    handle: Option<JoinHandle<()>>,
    stop: Arc<AtomicBool>,
    #[cfg(unix)]
    capacity_notifier: TtyInputCapacityNotifier,
}

#[cfg(unix)]
mod unix_reader;
#[cfg(unix)]
pub use unix_reader::TtyInputCapacityNotifier;

/// Non-Unix input retains its existing forwarding path and has no capacity wake.
#[cfg(not(unix))]
#[derive(Clone, Debug)]
pub struct TtyInputCapacityNotifier {
    _private: (),
}

#[cfg(not(unix))]
impl TtyInputCapacityNotifier {
    #[must_use = "a failed capacity wake can delay queued TTY input"]
    pub fn notify_capacity(&self) -> io::Result<()> {
        Ok(())
    }
}

// The owner and bridge may share the notifier; the join/stop handle may move
// between native threads. These compile-only contracts execute no code.
const _: fn() = || {
    fn assert_shareable_debug<T: Send + Sync + std::fmt::Debug>() {}
    assert_shareable_debug::<TtyInputCapacityNotifier>();
    assert_shareable_debug::<TtyInputReader>();
};

impl TtyInputReader {
    /// Spawn terminal input acquisition. Unix owns its pending FIFO and direct
    /// bounded delivery; non-Unix retains its existing forwarding worker.
    pub fn spawn(comms: RenderComms) -> io::Result<Self> {
        #[cfg(unix)]
        {
            unix_reader::spawn_default(comms)
        }
        #[cfg(not(unix))]
        {
            let stop = Arc::new(AtomicBool::new(false));
            let input_stop = Arc::clone(&stop);
            let handle = thread::Builder::new()
            .name("tty-input-reader".to_string())
            .spawn(move || {
                let pause = Arc::new(AtomicBool::new(false));
                let (tx, rx) = crossbeam_channel::unbounded();
                let reader_stop = Arc::clone(&input_stop);
                let reader_pause = Arc::clone(&pause);
                let reader_handle = thread::Builder::new()
                    .name("tty-input-raw".to_string())
                    .spawn(move || read_tty_events(tx, reader_stop, reader_pause))
                    .ok();

                // Forward events to the RenderComms channel and listen for
                // shutdown commands.
                loop {
                    crossbeam_channel::select! {
                        recv(comms.cmd_rx) -> msg => {
                            match msg {
                                Ok(RenderCommand::Lifecycle(LifecycleCommand::Shutdown)) | Err(_) => break,
                                Ok(_) => {}
                            }
                        }
                        recv(rx) -> msg => {
                            match msg {
                                Ok(event) => comms.send_input(event),
                                Err(_) => break,
                            }
                        }
                        default(Duration::from_millis(50)) => {}
                    }
                }

                input_stop.store(true, Ordering::Relaxed);
                if let Some(h) = reader_handle {
                    let _ = h.join();
                }
            })
            ?;

            Ok(Self {
                handle: Some(handle),
                stop,
            })
        }
    }

    /// Capacity notification for the primary bridge after it receives an event.
    pub fn capacity_notifier(&self) -> Option<TtyInputCapacityNotifier> {
        #[cfg(unix)]
        {
            Some(self.capacity_notifier.clone())
        }
        #[cfg(not(unix))]
        {
            None
        }
    }

    fn request_stop(&self) -> io::Result<()> {
        self.stop.store(true, Ordering::Release);
        #[cfg(unix)]
        {
            self.capacity_notifier.notify_owner_stop()
        }
        #[cfg(not(unix))]
        {
            Ok(())
        }
    }

    #[cfg(all(unix, test))]
    fn spawn_with_test_source(
        comms: RenderComms,
        input: std::os::fd::OwnedFd,
        size_query: TerminalSizeQuery,
        hook: Option<Arc<dyn Fn(unix_reader::ReaderTestObservation) + Send + Sync>>,
    ) -> io::Result<Self> {
        unix_reader::spawn_with_source(comms, input, size_query, hook)
    }

    #[cfg(all(unix, test))]
    fn request_stop_for_test(&self) -> io::Result<()> {
        self.request_stop()
    }

    /// Signal the input reader to stop and wait for it to finish.
    pub fn join(mut self) {
        if let Err(error) = self.request_stop() {
            tracing::warn!(%error, "tty_input: failed to wake stopped reader");
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
#[path = "tty_input/tests/tty_input_test.rs"]
mod tests;

#[cfg(all(test, unix))]
#[path = "tty_input/tests/tty_relay_test.rs"]
mod relay_tests;
