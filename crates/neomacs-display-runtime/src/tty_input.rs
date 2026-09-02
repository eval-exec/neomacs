//! TTY input reader.
//!
//! GNU Emacs reads Unix TTY keyboard input as raw bytes into its own keyboard
//! buffer (`tty_read_avail_input` in src/keyboard.c).  Keep the Unix path byte
//! based for the same semantics; use crossterm's parsed events only where the
//! platform does not expose the same Unix TTY model.

use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::thread_comm::{InputEvent, LifecycleCommand, RenderCommand, RenderComms};
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

/// Map a crossterm key event to a typed frontend key event.
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

    let keysym = match event.code {
        KeyCode::Char(c)
            if event.modifiers.contains(KeyModifiers::CONTROL)
                && tty_control_char_keysym(c).is_some() =>
        {
            modifiers = without_control(modifiers);
            tty_control_char_keysym(c)
        }
        KeyCode::Char(c) => Some(c as u32),
        KeyCode::F(n) if (1..=12).contains(&n) => {
            Some(0xffbe + (n as u32 - 1)) // F1=0xffbe … F12=0xffc9
        }
        KeyCode::F(_) => None, // unsupported function key
        KeyCode::Esc => Some(XK_ESCAPE),
        KeyCode::Enter => Some(XK_RETURN),
        KeyCode::Tab => Some(XK_TAB),
        KeyCode::Backspace => Some(0x7f),
        KeyCode::Delete => Some(0xffff),
        KeyCode::Insert => Some(0xff63),
        KeyCode::Home => Some(0xff50),
        KeyCode::End => Some(0xff57),
        KeyCode::PageUp => Some(0xff55),
        KeyCode::PageDown => Some(0xff56),
        KeyCode::Left => Some(XK_LEFT),
        KeyCode::Up => Some(XK_UP),
        KeyCode::Right => Some(XK_RIGHT),
        KeyCode::Down => Some(XK_DOWN),
        KeyCode::Null => Some(0x00),
        KeyCode::CapsLock
        | KeyCode::ScrollLock
        | KeyCode::NumLock
        | KeyCode::PrintScreen
        | KeyCode::Pause
        | KeyCode::Menu
        | KeyCode::KeypadBegin
        | KeyCode::Media(_)
        | KeyCode::Modifier(_) => None, // suppress bare modifier/media keys
        KeyCode::BackTab => Some(0xff09), // same as Tab, but with shift modifier
    };

    let keysym = keysym?;

    Some(InputEvent::key(
        keysym,
        modifiers,
        event.kind == KeyEventKind::Press,
        0,
    ))
}

#[cfg(unix)]
fn raw_tty_input_event(bytes: Vec<u8>) -> InputEvent {
    InputEvent::RawTtyBytes {
        bytes,
        emacs_frame_id: 0,
    }
}

#[cfg(unix)]
fn read_tty_events(
    tx: crossbeam_channel::Sender<InputEvent>,
    stop: Arc<AtomicBool>,
    _paused: Arc<AtomicBool>,
) {
    let mut last_size = crossterm::terminal::size().ok();

    while !stop.load(Ordering::Relaxed) {
        if let Ok(size) = crossterm::terminal::size()
            && last_size != Some(size)
        {
            last_size = Some(size);
            let event = InputEvent::viewport_changed(size.0 as u32, size.1 as u32, 1.0, 0)
                .expect("one is a valid TTY scale");
            if tx.send(event).is_err() {
                tracing::warn!("tty_input: channel closed");
                return;
            }
        }

        let mut pollfd = libc::pollfd {
            fd: libc::STDIN_FILENO,
            events: libc::POLLIN,
            revents: 0,
        };
        // This timeout only lets the reader observe stop/resize state. It is
        // not an input-sequence or ESC ambiguity deadline.
        let poll_timeout_ms = TTY_READER_POLL_INTERVAL.as_millis() as libc::c_int;
        let poll_result = unsafe { libc::poll(&mut pollfd, 1, poll_timeout_ms) };
        if poll_result < 0 {
            let err = io::Error::last_os_error();
            if err.kind() != io::ErrorKind::Interrupted {
                tracing::warn!("tty_input: poll error: {}", err);
                thread::sleep(Duration::from_millis(100));
            }
            continue;
        }
        if poll_result == 0 || pollfd.revents & libc::POLLIN == 0 {
            continue;
        }

        let mut buf = [0u8; GNU_KBD_BUFFER_SIZE - 1];
        let nread = unsafe {
            libc::read(
                libc::STDIN_FILENO,
                buf.as_mut_ptr().cast::<libc::c_void>(),
                buf.len(),
            )
        };

        if nread < 0 {
            let err = io::Error::last_os_error();
            if err.kind() != io::ErrorKind::Interrupted && err.kind() != io::ErrorKind::WouldBlock {
                tracing::warn!("tty_input: read error: {}", err);
                thread::sleep(Duration::from_millis(100));
            }
            continue;
        }
        if nread == 0 {
            thread::sleep(Duration::from_millis(10));
            continue;
        }

        let event = raw_tty_input_event(buf[..nread as usize].to_vec());
        tracing::debug!("tty_input: raw byte batch {:?}", event);
        if tx.send(event).is_err() {
            tracing::warn!("tty_input: channel closed");
            return;
        }
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
                        let event = InputEvent::viewport_changed(cols as u32, rows as u32, 1.0, 0)
                            .expect("one is a valid TTY scale");
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

/// A standalone TTY input reader that forwards terminal key and resize
/// events to `RenderComms` using crossterm.
///
/// Used by the `-nw` path when rendering goes through `TtyRif` on the
/// evaluator thread.
pub struct TtyInputReader {
    handle: Option<JoinHandle<()>>,
    stop: Arc<AtomicBool>,
}

impl TtyInputReader {
    /// Spawn a background thread that reads terminal input and sends events
    /// through `comms.send_input()`.
    pub fn spawn(comms: RenderComms) -> Self {
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
            .expect("Failed to spawn tty-input-reader thread");

        Self {
            handle: Some(handle),
            stop,
        }
    }

    /// Signal the input reader to stop and wait for it to finish.
    pub fn join(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEventKind, KeyEventState};

    #[cfg(unix)]
    #[test]
    fn unix_tty_read_batch_is_forwarded_as_raw_bytes() {
        let event = raw_tty_input_event(b"\x1b[A".to_vec());

        assert!(matches!(
            event,
            InputEvent::RawTtyBytes {
                bytes,
                emacs_frame_id: 0,
            } if bytes == b"\x1b[A"
        ));
    }

    fn key_event(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        }
    }

    fn key_parts(code: KeyCode, modifiers: KeyModifiers) -> (u32, u32) {
        match map_key_event(key_event(code, modifiers)).expect("key event") {
            InputEvent::Frontend(neomacs_app::frontend_event::FrontendEvent::Key(key)) => {
                (key.symbol().get(), key.modifiers().bits())
            }
            _ => panic!("expected key event"),
        }
    }

    #[test]
    fn tty_control_digit_aliases_are_raw_control_bytes() {
        let cases = [
            ('2', 0x00),
            ('3', 0x1b),
            ('4', 0x1c),
            ('5', 0x1d),
            ('6', 0x1e),
            ('7', 0x1f),
            ('8', 0x7f),
            ('/', 0x1f),
        ];

        for (input, expected) in cases {
            let (keysym, modifiers) = key_parts(KeyCode::Char(input), KeyModifiers::CONTROL);
            assert_eq!(keysym, expected, "input C-{input}");
            assert_eq!(modifiers & NEOMACS_CTRL_MASK, 0, "input C-{input}");
        }
    }

    #[test]
    fn tty_meta_control_alias_preserves_meta_only() {
        let (keysym, modifiers) = key_parts(
            KeyCode::Char('4'),
            KeyModifiers::CONTROL | KeyModifiers::ALT,
        );

        assert_eq!(keysym, 0x1c);
        assert_eq!(modifiers & NEOMACS_CTRL_MASK, 0);
        assert_ne!(modifiers & NEOMACS_META_MASK, 0);
    }

    #[test]
    fn tty_backspace_is_raw_del_byte() {
        let (keysym, modifiers) = key_parts(KeyCode::Backspace, KeyModifiers::ALT);

        assert_eq!(keysym, 0x7f);
        assert_ne!(modifiers & NEOMACS_META_MASK, 0);
    }
}
