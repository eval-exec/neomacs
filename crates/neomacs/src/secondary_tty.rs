//! Additional text-terminal sessions opened by `make-terminal-frame`.
//!
//! The VM owns Lisp, frame, and terminal identity. This module owns the Unix
//! resources behind one explicit `(tty . DEVICE)` request: file descriptors,
//! termios state, raw input, terminal-size observation, and one `TtyRif`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use neovm_core::emacs_core::process::WaitNotifier;
use neovm_core::emacs_core::terminal::pure::{
    OpenedTtyFrameHost, TerminalHost, TtyFrameHostFactory, TtyFrameOpenRequest, TtyFrameSize,
};
use neovm_core::keyboard::InputEvent;

use super::frame_layout;

#[derive(Clone, Default)]
pub struct SecondaryTtyRegistry {
    sessions: Arc<Mutex<HashMap<u64, SecondaryTtySession>>>,
}

impl SecondaryTtyRegistry {
    pub fn render_selected(&self, eval: &mut neovm_core::emacs_core::Context) -> bool {
        frame_layout::REDISPLAY_RUNTIME.with(|runtime| self.render_selected_with(eval, runtime))
    }

    pub fn render_selected_with(
        &self,
        eval: &mut neovm_core::emacs_core::Context,
        runtime: &neomacs_app::presentation::EditorPresentationRuntime,
    ) -> bool {
        #[cfg(not(unix))]
        {
            let _ = (eval, runtime);
            return false;
        }
        #[cfg(unix)]
        {
            self.render_selected_unix(eval, runtime)
        }
    }

    #[cfg(unix)]
    fn render_selected_unix(
        &self,
        eval: &mut neovm_core::emacs_core::Context,
        runtime: &neomacs_app::presentation::EditorPresentationRuntime,
    ) -> bool {
        let Some((terminal_id, is_tty)) = eval
            .frame_manager()
            .selected_frame()
            .map(|frame| (frame.terminal_id, frame.effective_window_system().is_none()))
        else {
            return false;
        };
        if !is_tty
            || !self
                .sessions
                .lock()
                .expect("secondary TTY registry poisoned")
                .contains_key(&terminal_id)
        {
            return false;
        }

        let presentations = frame_layout::run_tty_layout_tree_with(runtime, eval);
        let mut sessions = self
            .sessions
            .lock()
            .expect("secondary TTY registry poisoned");
        let Some(session) = sessions.get_mut(&terminal_id) else {
            return true;
        };
        if session.device.is_active()
            && let Some((root, children)) = presentations
        {
            frame_layout::run_tty_rif_redisplay_to(
                &mut session.rif,
                &root,
                &children,
                &mut session.device,
            );
        }
        true
    }

    #[cfg(unix)]
    fn suspend(&self, terminal_id: u64) -> Result<(), String> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| "secondary TTY registry poisoned".to_string())?;
        sessions
            .get_mut(&terminal_id)
            .ok_or_else(|| "TTY terminal host unavailable".to_string())?
            .suspend()
    }

    #[cfg(not(unix))]
    fn suspend(&self, _terminal_id: u64) -> Result<(), String> {
        Err("additional text terminals are not supported on this platform".to_string())
    }

    #[cfg(unix)]
    fn resume(&self, terminal_id: u64) -> Result<(), String> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| "secondary TTY registry poisoned".to_string())?;
        sessions
            .get_mut(&terminal_id)
            .ok_or_else(|| "TTY terminal host unavailable".to_string())?
            .resume()
    }

    #[cfg(not(unix))]
    fn resume(&self, _terminal_id: u64) -> Result<(), String> {
        Err("additional text terminals are not supported on this platform".to_string())
    }

    fn remove(&self, terminal_id: u64) -> Result<(), String> {
        let session = self
            .sessions
            .lock()
            .map_err(|_| "secondary TTY registry poisoned".to_string())?
            .remove(&terminal_id);
        session
            .map(drop)
            .ok_or_else(|| "TTY terminal host unavailable".to_string())
    }
}

pub struct SecondaryTtyFactory {
    registry: SecondaryTtyRegistry,
    input_tx: crossbeam_channel::Sender<InputEvent>,
    notifier: Option<WaitNotifier>,
    quit_requested: Arc<AtomicBool>,
}

impl SecondaryTtyFactory {
    pub fn new(
        registry: SecondaryTtyRegistry,
        input_tx: crossbeam_channel::Sender<InputEvent>,
        notifier: Option<WaitNotifier>,
        quit_requested: Arc<AtomicBool>,
    ) -> Self {
        Self {
            registry,
            input_tx,
            notifier,
            quit_requested,
        }
    }
}

impl TtyFrameHostFactory for SecondaryTtyFactory {
    fn open_tty(&mut self, request: TtyFrameOpenRequest) -> Result<OpenedTtyFrameHost, String> {
        #[cfg(unix)]
        {
            let (session, size, attributes) = SecondaryTtySession::open(
                &request,
                self.input_tx.clone(),
                self.notifier.clone(),
                Arc::clone(&self.quit_requested),
            )?;
            let terminal_id = request.terminal_id();
            let mut sessions = self
                .registry
                .sessions
                .lock()
                .map_err(|_| "secondary TTY registry poisoned".to_string())?;
            if let std::collections::hash_map::Entry::Vacant(entry) = sessions.entry(terminal_id) {
                entry.insert(session);
            } else {
                return Err(format!("terminal {terminal_id} is already open"));
            }
            Ok(OpenedTtyFrameHost::new(
                size,
                attributes,
                Box::new(SecondaryTtyHost {
                    registry: self.registry.clone(),
                    terminal_id,
                }),
            ))
        }
        #[cfg(not(unix))]
        {
            let _ = request;
            Err("additional text terminals are not supported on this platform".to_string())
        }
    }
}

struct SecondaryTtyHost {
    registry: SecondaryTtyRegistry,
    terminal_id: u64,
}

impl TerminalHost for SecondaryTtyHost {
    fn suspend_tty(&mut self) -> Result<(), String> {
        self.registry.suspend(self.terminal_id)
    }

    fn resume_tty(&mut self) -> Result<(), String> {
        self.registry.resume(self.terminal_id)
    }

    fn delete_terminal(&mut self) -> Result<(), String> {
        self.registry.remove(self.terminal_id)
    }
}

struct SecondaryTtySession {
    #[cfg(unix)]
    device: TtyDevice,
    #[cfg(unix)]
    rif: neomacs_display_runtime::backend::tty::rif::TtyRif,
    #[cfg(unix)]
    stop: Arc<AtomicBool>,
    #[cfg(unix)]
    paused: Arc<AtomicBool>,
    #[cfg(unix)]
    reader: Option<std::thread::JoinHandle<()>>,
}

/// One opened terminal and the mode snapshot required to put it back exactly
/// as we found it.  Constructing this type means restoration is now mandatory:
/// setup errors and panics take the same cleanup path as normal deletion.
#[cfg(unix)]
struct TtyDevice {
    file: std::fs::File,
    original_termios: libc::termios,
    active: bool,
}

#[cfg(unix)]
impl TtyDevice {
    fn open(path: &str) -> Result<Self, String> {
        use std::io::Write;
        use std::os::fd::AsRawFd;
        use std::os::unix::fs::OpenOptionsExt;

        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NOCTTY)
            .open(path)
            .map_err(|error| format!("cannot open {path}: {error}"))?;
        let mut original_termios = unsafe { std::mem::zeroed::<libc::termios>() };
        if unsafe { libc::tcgetattr(file.as_raw_fd(), &mut original_termios) } != 0 {
            return Err(format!(
                "cannot read terminal modes for {path}: {}",
                std::io::Error::last_os_error()
            ));
        }
        set_raw_mode(file.as_raw_fd(), &original_termios)?;

        // From this point onward every `?` drops an active TtyDevice and thus
        // restores termios.  This closes the partial-initialization hole.
        let mut device = Self {
            file,
            original_termios,
            active: true,
        };
        device
            .file
            .write_all(super::tty_init::tty_enter_sequence())
            .and_then(|()| device.file.flush())
            .map_err(|error| format!("cannot initialize {path}: {error}"))?;
        Ok(device)
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn suspend(&mut self) -> Result<(), String> {
        use std::io::Write;
        use std::os::fd::AsRawFd;

        if !self.active {
            return Ok(());
        }
        let leave_result = self
            .file
            .write_all(super::tty_init::tty_leave_sequence())
            .and_then(|()| self.file.flush())
            .map_err(|error| format!("cannot suspend TTY renderer: {error}"));
        let restore_result = if unsafe {
            libc::tcsetattr(self.file.as_raw_fd(), libc::TCSANOW, &self.original_termios)
        } == 0
        {
            self.active = false;
            Ok(())
        } else {
            Err(format!(
                "cannot restore terminal modes: {}",
                std::io::Error::last_os_error()
            ))
        };
        leave_result.and(restore_result)
    }

    fn resume(&mut self) -> Result<(), String> {
        use std::io::Write;
        use std::os::fd::AsRawFd;

        if self.active {
            return Ok(());
        }
        set_raw_mode(self.file.as_raw_fd(), &self.original_termios)?;
        self.active = true;
        if let Err(error) = self
            .file
            .write_all(super::tty_init::tty_enter_sequence())
            .and_then(|()| self.file.flush())
        {
            let _ = self.suspend();
            return Err(format!("cannot resume TTY renderer: {error}"));
        }
        Ok(())
    }
}

#[cfg(unix)]
impl std::io::Write for TtyDevice {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        std::io::Write::write(&mut self.file, buffer)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        std::io::Write::flush(&mut self.file)
    }
}

#[cfg(unix)]
impl Drop for TtyDevice {
    fn drop(&mut self) {
        let _ = self.suspend();
    }
}

#[cfg(unix)]
impl SecondaryTtySession {
    fn open(
        request: &TtyFrameOpenRequest,
        input_tx: crossbeam_channel::Sender<InputEvent>,
        notifier: Option<WaitNotifier>,
        quit_requested: Arc<AtomicBool>,
    ) -> Result<
        (
            Self,
            TtyFrameSize,
            neomacs_display_protocol::tty_capabilities::TtyAttributeCapabilities,
        ),
        String,
    > {
        use std::os::unix::fs::OpenOptionsExt;

        super::terminal_capabilities::check_terminal_powerful_enough(request.terminal_type())?;
        let device = TtyDevice::open(request.device())?;

        let size = query_size(std::os::fd::AsRawFd::as_raw_fd(&device.file)).unwrap_or_else(|| {
            TtyFrameSize::new(80, 25).expect("fallback TTY dimensions are non-zero")
        });
        let attributes = super::tty_init::tty_attribute_capabilities(
            &std::env::var("COLORTERM").unwrap_or_default(),
            request.terminal_type(),
            super::terminal_capabilities::open_terminal_capability_database,
        );
        let term_caps = super::terminal_capabilities::term_caps_for_term(request.terminal_type())
            .unwrap_or_else(|| {
                neomacs_display_runtime::backend::tty::rif::TermCaps::unknown_terminal()
            });
        let rif = neomacs_display_runtime::backend::tty::rif::TtyRif::new_with_caps(
            size.columns() as usize,
            size.rows() as usize,
            term_caps,
        );
        // Input has its own nonblocking open-file description.  A `dup`/clone
        // would share O_NONBLOCK with renderer output and could make redraws
        // fail spuriously; a blocking reader could hang terminal teardown.
        let input = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOCTTY | libc::O_NONBLOCK)
            .open(request.device())
            .map_err(|error| format!("cannot open input for {}: {error}", request.device()))?;
        let stop = Arc::new(AtomicBool::new(false));
        let paused = Arc::new(AtomicBool::new(false));
        let reader_stop = Arc::clone(&stop);
        let reader_paused = Arc::clone(&paused);
        let frame_id = request.frame_id().0;
        let terminal_id = request.terminal_id();
        let reader = std::thread::Builder::new()
            .name(format!("tty-input-{frame_id}"))
            .spawn(move || {
                read_secondary_tty(
                    input,
                    frame_id,
                    terminal_id,
                    size,
                    input_tx,
                    notifier,
                    quit_requested,
                    reader_stop,
                    reader_paused,
                );
            })
            .map_err(|error| format!("cannot start TTY input reader: {error}"))?;

        Ok((
            Self {
                device,
                rif,
                stop,
                paused,
                reader: Some(reader),
            },
            size,
            attributes,
        ))
    }

    fn suspend(&mut self) -> Result<(), String> {
        self.paused.store(true, Ordering::Release);
        self.device.suspend()
    }

    fn resume(&mut self) -> Result<(), String> {
        if self.device.is_active() {
            return Ok(());
        }
        self.device.resume()?;
        self.rif.force_redraw();
        self.paused.store(false, Ordering::Release);
        Ok(())
    }
}

#[cfg(unix)]
impl Drop for SecondaryTtySession {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        // TtyDevice's Drop is the single mandatory terminal-restoration path.
    }
}

#[cfg(unix)]
fn set_raw_mode(fd: std::os::fd::RawFd, original: &libc::termios) -> Result<(), String> {
    let mut raw = *original;
    unsafe { libc::cfmakeraw(&mut raw) };
    if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &raw) } != 0 {
        return Err(format!(
            "cannot set raw terminal modes: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn query_size(fd: std::os::fd::RawFd) -> Option<TtyFrameSize> {
    let mut size = unsafe { std::mem::zeroed::<libc::winsize>() };
    (unsafe { libc::ioctl(fd, libc::TIOCGWINSZ as _, &mut size) } == 0)
        .then(|| TtyFrameSize::new(u32::from(size.ws_col), u32::from(size.ws_row)))
        .flatten()
}

#[cfg(unix)]
fn read_secondary_tty(
    input: std::fs::File,
    frame_id: u64,
    terminal_id: u64,
    initial_size: TtyFrameSize,
    input_tx: crossbeam_channel::Sender<InputEvent>,
    notifier: Option<WaitNotifier>,
    quit_requested: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
) {
    use std::os::fd::AsRawFd;
    use std::time::Duration;

    let fd = input.as_raw_fd();
    let mut last_size = initial_size;
    while !stop.load(Ordering::Acquire) {
        if paused.load(Ordering::Acquire) {
            std::thread::sleep(Duration::from_millis(50));
            continue;
        }
        if let Some(size) = query_size(fd)
            && size != last_size
        {
            last_size = size;
            if !publish_input(
                &input_tx,
                notifier.as_ref(),
                &stop,
                InputEvent::Resize {
                    width: size.columns(),
                    height: size.rows(),
                    scale_factor: 1.0,
                    emacs_frame_id: frame_id,
                },
            ) {
                return;
            }
        }

        let mut pollfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        let result = unsafe { libc::poll(&mut pollfd, 1, 50) };
        if result < 0 {
            if std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted {
                return;
            }
            continue;
        }
        if result == 0 || pollfd.revents & libc::POLLIN == 0 {
            if pollfd.revents & (libc::POLLHUP | libc::POLLERR) != 0 {
                let _ = publish_input(
                    &input_tx,
                    notifier.as_ref(),
                    &stop,
                    InputEvent::WindowClose {
                        emacs_frame_id: frame_id,
                    },
                );
                return;
            }
            continue;
        }
        let mut bytes = [0u8; 4095];
        let read = unsafe { libc::read(fd, bytes.as_mut_ptr().cast(), bytes.len()) };
        if read == 0 {
            let _ = publish_input(
                &input_tx,
                notifier.as_ref(),
                &stop,
                InputEvent::WindowClose {
                    emacs_frame_id: frame_id,
                },
            );
            return;
        }
        if read < 0 {
            let kind = std::io::Error::last_os_error().kind();
            if matches!(
                kind,
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
            ) {
                continue;
            }
            let _ = publish_input(
                &input_tx,
                notifier.as_ref(),
                &stop,
                InputEvent::WindowClose {
                    emacs_frame_id: frame_id,
                },
            );
            return;
        }
        if stop.load(Ordering::Acquire) {
            continue;
        }
        let bytes = bytes[..read as usize].to_vec();
        if bytes.contains(&0x07) {
            quit_requested.store(true, Ordering::Relaxed);
        }
        if !publish_input(
            &input_tx,
            notifier.as_ref(),
            &stop,
            InputEvent::raw_tty_bytes_for_terminal(bytes, terminal_id),
        ) {
            return;
        }
    }
}

#[cfg(unix)]
fn publish_input(
    input_tx: &crossbeam_channel::Sender<InputEvent>,
    notifier: Option<&WaitNotifier>,
    stop: &AtomicBool,
    mut event: InputEvent,
) -> bool {
    use crossbeam_channel::SendTimeoutError;
    use std::time::Duration;

    loop {
        match input_tx.send_timeout(event, Duration::from_millis(50)) {
            Ok(()) => {
                if let Some(notifier) = notifier {
                    let _ = notifier.notify();
                }
                return true;
            }
            Err(SendTimeoutError::Timeout(returned)) if !stop.load(Ordering::Acquire) => {
                event = returned;
            }
            Err(SendTimeoutError::Timeout(_) | SendTimeoutError::Disconnected(_)) => return false,
        }
    }
}

#[cfg(test)]
#[path = "secondary_tty_test.rs"]
mod secondary_tty_test;
