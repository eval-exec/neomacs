use crate::thread_comm::{ClipboardCommand, ClipboardSelection};
use arboard::Clipboard;
use crossbeam_channel::{Receiver, Sender};
use neomacs_display_protocol::SelectionOwner;
use std::thread::JoinHandle;
use std::time::Duration;
use winit::event_loop::OwnedDisplayHandle;

#[cfg(target_os = "linux")]
use arboard::{ClearExtLinux, GetExtLinux, LinuxClipboardKind, SetExtLinux};
#[cfg(target_os = "linux")]
use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};

const WORKER_SHUTDOWN_TIMEOUT: Duration = Duration::from_millis(250);
const REQUEST_QUEUE_CAPACITY: usize = 32;

trait ClipboardBackend: Send {
    fn set_text(&mut self, selection: ClipboardSelection, text: Option<&str>)
    -> Result<(), String>;

    fn text(&mut self, selection: ClipboardSelection) -> Result<Option<String>, String>;

    fn owner(&mut self, selection: ClipboardSelection) -> Result<SelectionOwner, String>;
}

enum ServiceCommand {
    Request(ClipboardCommand),
    Shutdown { acknowledged: Sender<()> },
}

/// A non-blocking handle to the serialized, display-owned clipboard worker.
///
/// Native clipboard calls never run on Winit's event-loop thread.  In
/// particular, reading from a Wayland selection may wait on another client;
/// isolating that wait keeps rendering and input responsive.
pub(crate) struct ClipboardService {
    commands: Sender<ServiceCommand>,
    worker: Option<JoinHandle<()>>,
}

impl ClipboardService {
    pub(crate) fn for_display(display: OwnedDisplayHandle) -> Result<Self, String> {
        std::cfg_select! {
            target_os = "linux" => {
                let raw_display = display
                    .display_handle()
                    .map_err(|err| format!("failed to access the native display: {err}"))?
                    .as_raw();
                if let RawDisplayHandle::Wayland(raw_display) = raw_display {
                    tracing::info!(
                        "Clipboard service using the native Wayland data-device backend"
                    );
                    // SAFETY: WaylandClipboard owns `display`, which keeps this
                    // wl_display alive until after `clipboard` is dropped.
                    let clipboard = unsafe {
                        smithay_clipboard::Clipboard::new(raw_display.display.as_ptr())
                    };
                    return Self::start(WaylandClipboard {
                        clipboard,
                        _display_owner: display,
                    });
                }
            }
            _ => {
                let _ = display;
            }
        }

        tracing::info!("Clipboard service using the arboard platform backend");
        Self::start(ArboardClipboard::new()?)
    }

    fn start(backend: impl ClipboardBackend + 'static) -> Result<Self, String> {
        let (commands, receiver) = crossbeam_channel::bounded(REQUEST_QUEUE_CAPACITY);
        let worker = std::thread::Builder::new()
            .name("neomacs-clipboard".to_owned())
            .spawn(move || run_worker(Box::new(backend), receiver))
            .map_err(|err| format!("failed to start clipboard worker: {err}"))?;
        Ok(Self {
            commands,
            worker: Some(worker),
        })
    }

    #[cfg(test)]
    fn with_backend(backend: impl ClipboardBackend + 'static) -> Self {
        Self::start(backend).expect("clipboard worker should start")
    }

    pub(crate) fn submit(&self, command: ClipboardCommand) {
        match self.commands.try_send(ServiceCommand::Request(command)) {
            Ok(()) => {}
            Err(crossbeam_channel::TrySendError::Full(ServiceCommand::Request(command))) => {
                reject_command(command, "clipboard worker queue is full".to_owned());
            }
            Err(crossbeam_channel::TrySendError::Disconnected(ServiceCommand::Request(
                command,
            ))) => {
                reject_command(command, "clipboard worker is unavailable".to_owned());
            }
            Err(_) => unreachable!("submit only sends clipboard requests"),
        }
    }
}

impl Drop for ClipboardService {
    fn drop(&mut self) {
        let Some(worker) = self.worker.take() else {
            return;
        };
        let (acknowledged, acknowledgement) = crossbeam_channel::bounded(1);
        match self
            .commands
            .try_send(ServiceCommand::Shutdown { acknowledged })
        {
            Ok(()) => {}
            Err(crossbeam_channel::TrySendError::Full(_)) => {
                // Dropping `commands` disconnects the worker after queued
                // requests finish.  It owns the display lifetime until then.
                tracing::warn!("clipboard worker queue is full during shutdown; detaching it");
                return;
            }
            Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
                if worker.join().is_err() {
                    tracing::warn!("clipboard worker panicked");
                }
                return;
            }
        }

        if acknowledgement
            .recv_timeout(WORKER_SHUTDOWN_TIMEOUT)
            .is_err()
        {
            // A foreign Wayland selection owner can stall a transfer forever.
            // Detaching is safe: the worker owns its OwnedDisplayHandle, so the
            // native display outlives every clipboard object it may still use.
            tracing::warn!("clipboard worker did not stop promptly; detaching it");
            return;
        }
        if worker.join().is_err() {
            tracing::warn!("clipboard worker panicked");
        }
    }
}

fn run_worker(mut backend: Box<dyn ClipboardBackend>, commands: Receiver<ServiceCommand>) {
    while let Ok(command) = commands.recv() {
        match command {
            ServiceCommand::Request(command) if command.is_expired() => {
                reject_command(
                    command,
                    "clipboard request expired before execution".to_owned(),
                );
            }
            ServiceCommand::Request(command) => execute_command(backend.as_mut(), command),
            ServiceCommand::Shutdown { acknowledged } => {
                // Acknowledgement means the native backend (including
                // smithay-clipboard's internal worker) has fully stopped.
                // If that drop stalls, ClipboardService's bounded wait will
                // detach this thread while its OwnedDisplayHandle stays live.
                drop(backend);
                let _ = acknowledged.send(());
                return;
            }
        }
    }
}

fn execute_command(backend: &mut dyn ClipboardBackend, command: ClipboardCommand) {
    match command {
        ClipboardCommand::SetText {
            selection,
            text,
            reply,
            ..
        } => {
            let result = backend.set_text(selection, text.as_deref());
            if let Err(err) = &result {
                tracing::warn!(?selection, "clipboard set failed: {err}");
            }
            if reply.send(result).is_err() {
                tracing::debug!("clipboard set reply receiver was dropped");
            }
        }
        ClipboardCommand::GetText {
            selection, reply, ..
        } => {
            let result = backend.text(selection);
            if let Err(err) = &result {
                tracing::warn!(?selection, "clipboard read failed: {err}");
            }
            if reply.send(result).is_err() {
                tracing::debug!("clipboard get reply receiver was dropped");
            }
        }
        ClipboardCommand::GetOwnership {
            selection, reply, ..
        } => {
            let result = backend.owner(selection);
            if let Err(err) = &result {
                tracing::warn!(?selection, "clipboard ownership query failed: {err}");
            }
            if reply.send(result).is_err() {
                tracing::debug!("clipboard ownership reply receiver was dropped");
            }
        }
    }
}

pub(crate) fn reject_command(command: ClipboardCommand, error: String) {
    match command {
        ClipboardCommand::SetText { reply, .. } => {
            let _ = reply.send(Err(error));
        }
        ClipboardCommand::GetText { reply, .. } => {
            let _ = reply.send(Err(error));
        }
        ClipboardCommand::GetOwnership { reply, .. } => {
            let _ = reply.send(Err(error));
        }
    }
}

/// Process-local stand-in for a selection the platform clipboard does not
/// expose.
///
/// GNU Emacs never rejects PRIMARY on a non-X platform.  Its w32 port keeps
/// PRIMARY as a Lisp property (lisp/term/w32-win.el:364-367, :417-451), so
/// another process cannot take it.  This typed state reproduces that contract
/// for Neomacs on every platform whose native clipboard API lacks PRIMARY.
///
/// Ledgered macOS divergence: GNU NS maps PRIMARY to a named NSPasteboard
/// (`src/nsselect.m:56,397-466,494-547`) and observes foreign takeover through
/// pasteboard change counts.  Arboard exposes only the conventional system
/// pasteboard, so Neomacs deliberately uses the process-local w32 model rather
/// than aliasing PRIMARY to CLIPBOARD.  Consequently `OtherProcess` cannot
/// arise for this state.  `ns-sent-selection-hooks` is also not run.
#[cfg(not(target_os = "linux"))]
#[derive(Debug, Default, PartialEq, Eq)]
enum PrivateSelection {
    /// Nobody owns the selection: `ns-get-selection` returns nil.
    #[default]
    Vacant,
    /// This process owns the selection with the given text.
    Owned(String),
}

#[cfg(not(target_os = "linux"))]
impl PrivateSelection {
    /// Own the selection with `text`, or disown it with `None`.
    fn store(&mut self, text: Option<&str>) {
        *self = match text {
            Some(text) => Self::Owned(text.to_owned()),
            None => Self::Vacant,
        };
    }

    fn load(&self) -> Option<String> {
        match self {
            Self::Owned(text) => Some(text.clone()),
            Self::Vacant => None,
        }
    }

    fn owner(&self) -> SelectionOwner {
        match self {
            Self::Owned(_) => SelectionOwner::ThisProcess,
            Self::Vacant => SelectionOwner::None,
        }
    }
}

struct ArboardClipboard {
    clipboard: Clipboard,
    /// PRIMARY on platforms whose clipboard API has no such selection.
    #[cfg(not(target_os = "linux"))]
    primary: PrivateSelection,
}

impl ArboardClipboard {
    fn new() -> Result<Self, String> {
        Clipboard::new()
            .map(|clipboard| Self {
                clipboard,
                #[cfg(not(target_os = "linux"))]
                primary: PrivateSelection::default(),
            })
            .map_err(|err| format!("failed to initialize the system clipboard: {err}"))
    }

    fn text_result(result: Result<String, arboard::Error>) -> Result<Option<String>, String> {
        match result {
            Ok(text) => Ok(Some(text)),
            Err(arboard::Error::ContentNotAvailable) => Ok(None),
            Err(err) => Err(err.to_string()),
        }
    }
}

impl ClipboardBackend for ArboardClipboard {
    fn set_text(
        &mut self,
        selection: ClipboardSelection,
        text: Option<&str>,
    ) -> Result<(), String> {
        std::cfg_select! {
            target_os = "linux" => {
                let selection = match selection {
                    ClipboardSelection::Clipboard => LinuxClipboardKind::Clipboard,
                    ClipboardSelection::Primary => LinuxClipboardKind::Primary,
                };
                match text {
                    Some(text) => self
                        .clipboard
                        .set()
                        .clipboard(selection)
                        .text(text.to_owned()),
                    None => self.clipboard.clear_with().clipboard(selection),
                }
                .map_err(|err| err.to_string())
            }
            _ => {
                match selection {
                    ClipboardSelection::Clipboard => match text {
                        Some(text) => self.clipboard.set_text(text.to_owned()),
                        None => self.clipboard.clear(),
                    }
                    .map_err(|err| err.to_string()),
                    ClipboardSelection::Primary => {
                        self.primary.store(text);
                        Ok(())
                    }
                }
            }
        }
    }

    fn text(&mut self, selection: ClipboardSelection) -> Result<Option<String>, String> {
        std::cfg_select! {
            target_os = "linux" => {
                let selection = match selection {
                    ClipboardSelection::Clipboard => LinuxClipboardKind::Clipboard,
                    ClipboardSelection::Primary => LinuxClipboardKind::Primary,
                };
                Self::text_result(self.clipboard.get().clipboard(selection).text())
            }
            _ => {
                match selection {
                    ClipboardSelection::Clipboard => Self::text_result(self.clipboard.get_text()),
                    ClipboardSelection::Primary => Ok(self.primary.load()),
                }
            }
        }
    }

    fn owner(&mut self, selection: ClipboardSelection) -> Result<SelectionOwner, String> {
        std::cfg_select! {
            target_os = "linux" => {
                let _ = selection;
                Ok(SelectionOwner::Unknown)
            }
            _ => {
                Ok(match selection {
                    ClipboardSelection::Clipboard => SelectionOwner::Unknown,
                    ClipboardSelection::Primary => self.primary.owner(),
                })
            }
        }
    }
}

#[cfg(target_os = "linux")]
struct WaylandClipboard {
    // Field order is significant: drop the protocol worker before its display.
    clipboard: smithay_clipboard::Clipboard,
    _display_owner: OwnedDisplayHandle,
}

#[cfg(target_os = "linux")]
impl ClipboardBackend for WaylandClipboard {
    fn set_text(
        &mut self,
        selection: ClipboardSelection,
        text: Option<&str>,
    ) -> Result<(), String> {
        let Some(text) = text else {
            return Err(
                "disowning a selection is not supported by the native Wayland clipboard backend"
                    .to_owned(),
            );
        };
        match selection {
            ClipboardSelection::Clipboard => self.clipboard.store(text.to_owned()),
            ClipboardSelection::Primary => self.clipboard.store_primary(text.to_owned()),
        }
        // smithay-clipboard queues ownership requests to its protocol worker;
        // success here means accepted by that API, not compositor confirmation.
        Ok(())
    }

    fn text(&mut self, selection: ClipboardSelection) -> Result<Option<String>, String> {
        let result = match selection {
            ClipboardSelection::Clipboard => self.clipboard.load(),
            ClipboardSelection::Primary => self.clipboard.load_primary(),
        };
        match result {
            Ok(text) => Ok(Some(text)),
            // smithay-clipboard currently exposes an untyped io::Error for an
            // unowned selection, including this stable error message.
            Err(err)
                if err.kind() == std::io::ErrorKind::NotFound
                    || err.to_string() == "selection is empty" =>
            {
                Ok(None)
            }
            Err(err) => Err(err.to_string()),
        }
    }

    fn owner(&mut self, _selection: ClipboardSelection) -> Result<SelectionOwner, String> {
        Ok(SelectionOwner::Unknown)
    }
}

#[cfg(test)]
#[path = "clipboard/tests/clipboard_test.rs"]
mod tests;
