use crate::thread_comm::{ClipboardCommand, ClipboardSelection};
use arboard::Clipboard;
use crossbeam_channel::{Receiver, Sender};
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
    }
}

/// Process-local stand-in for a selection the platform clipboard does not
/// expose.
///
/// GNU Emacs never rejects PRIMARY on a non-X platform.  The NS port maps it
/// to a private pasteboard named "Selection" (emacs-31.0.90 src/nsselect.m:56
/// and :547, `symbol_to_nsstring` / `nxatoms_of_nsselect`), which
/// `ns-own-selection-internal` (:397-446) writes and `ns-get-selection`
/// (:514-541) reads back, and the w32 port keeps it as a Lisp property
/// (lisp/term/w32-win.el:364-367, :417-447).  Both are only visible to this
/// Emacs, which is what this store reproduces.
///
/// Ledgered divergence: GNU NS also declares `ns_send_types` on the
/// pasteboard and compares change counts to detect a foreign owner, since a
/// named NSPasteboard is reachable by other processes that know its name.
/// Neomacs does not expose that pasteboard, so a foreign owner cannot arise
/// and the value lives in this process only.
#[cfg(not(target_os = "linux"))]
#[derive(Debug, Default)]
struct PrivatePasteboard {
    text: Option<String>,
}

#[cfg(not(target_os = "linux"))]
impl PrivatePasteboard {
    /// Own the selection with `text`, or disown it with `None`
    /// (`ns-disown-selection-internal`, nsselect.m:449-466).
    fn store(&mut self, text: Option<&str>) {
        self.text = text.map(str::to_owned);
    }

    fn load(&self) -> Option<String> {
        self.text.clone()
    }
}

struct ArboardClipboard {
    clipboard: Clipboard,
    /// PRIMARY on platforms whose clipboard API has no such selection.
    #[cfg(not(target_os = "linux"))]
    primary: PrivatePasteboard,
}

impl ArboardClipboard {
    fn new() -> Result<Self, String> {
        Clipboard::new()
            .map(|clipboard| Self {
                clipboard,
                #[cfg(not(target_os = "linux"))]
                primary: PrivatePasteboard::default(),
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct MemoryClipboard {
        selections: HashMap<ClipboardSelection, String>,
    }

    struct BlockingClipboard {
        started: Sender<()>,
        release: Receiver<()>,
    }

    struct BlockingDropClipboard {
        drop_started: Sender<()>,
        release_drop: Receiver<()>,
    }

    struct ExpiringClipboard {
        read_started: Sender<()>,
        release_read: Receiver<()>,
        writes: Sender<Option<String>>,
    }

    impl ClipboardBackend for MemoryClipboard {
        fn set_text(
            &mut self,
            selection: ClipboardSelection,
            text: Option<&str>,
        ) -> Result<(), String> {
            if let Some(text) = text {
                self.selections.insert(selection, text.to_owned());
            } else {
                self.selections.remove(&selection);
            }
            Ok(())
        }

        fn text(&mut self, selection: ClipboardSelection) -> Result<Option<String>, String> {
            Ok(self.selections.get(&selection).cloned())
        }
    }

    impl ClipboardBackend for BlockingClipboard {
        fn set_text(
            &mut self,
            _selection: ClipboardSelection,
            _text: Option<&str>,
        ) -> Result<(), String> {
            Ok(())
        }

        fn text(&mut self, _selection: ClipboardSelection) -> Result<Option<String>, String> {
            self.started.send(()).unwrap();
            self.release.recv().unwrap();
            Ok(Some("released".to_owned()))
        }
    }

    impl ClipboardBackend for BlockingDropClipboard {
        fn set_text(
            &mut self,
            _selection: ClipboardSelection,
            _text: Option<&str>,
        ) -> Result<(), String> {
            Ok(())
        }

        fn text(&mut self, _selection: ClipboardSelection) -> Result<Option<String>, String> {
            Ok(None)
        }
    }

    impl Drop for BlockingDropClipboard {
        fn drop(&mut self) {
            self.drop_started.send(()).unwrap();
            self.release_drop.recv().unwrap();
        }
    }

    impl ClipboardBackend for ExpiringClipboard {
        fn set_text(
            &mut self,
            _selection: ClipboardSelection,
            text: Option<&str>,
        ) -> Result<(), String> {
            self.writes.send(text.map(str::to_owned)).unwrap();
            Ok(())
        }

        fn text(&mut self, _selection: ClipboardSelection) -> Result<Option<String>, String> {
            self.read_started.send(()).unwrap();
            self.release_read.recv().unwrap();
            Ok(None)
        }
    }

    fn set_text(
        service: &ClipboardService,
        selection: ClipboardSelection,
        text: Option<&str>,
    ) -> Result<(), String> {
        let (reply, result) = crossbeam_channel::bounded(1);
        service.submit(ClipboardCommand::SetText {
            selection,
            text: text.map(str::to_owned),
            expires_at: std::time::Instant::now() + Duration::from_secs(5),
            reply,
        });
        result.recv().unwrap()
    }

    fn text(
        service: &ClipboardService,
        selection: ClipboardSelection,
    ) -> Result<Option<String>, String> {
        let (reply, result) = crossbeam_channel::bounded(1);
        service.submit(ClipboardCommand::GetText {
            selection,
            expires_at: std::time::Instant::now() + Duration::from_secs(5),
            reply,
        });
        result.recv().unwrap()
    }

    #[test]
    fn service_keeps_clipboard_and_primary_distinct_and_can_clear_them() {
        let service = ClipboardService::with_backend(MemoryClipboard::default());

        set_text(&service, ClipboardSelection::Clipboard, Some("copied")).unwrap();
        set_text(&service, ClipboardSelection::Primary, Some("selected")).unwrap();
        assert_eq!(
            text(&service, ClipboardSelection::Clipboard).unwrap(),
            Some("copied".to_owned())
        );
        assert_eq!(
            text(&service, ClipboardSelection::Primary).unwrap(),
            Some("selected".to_owned())
        );

        set_text(&service, ClipboardSelection::Clipboard, None).unwrap();
        assert_eq!(text(&service, ClipboardSelection::Clipboard).unwrap(), None);
        assert_eq!(
            text(&service, ClipboardSelection::Primary).unwrap(),
            Some("selected".to_owned())
        );
    }

    #[test]
    fn submitting_a_slow_native_read_never_blocks_the_caller() {
        let (started, has_started) = crossbeam_channel::bounded(1);
        let (release, may_finish) = crossbeam_channel::bounded(1);
        let service = ClipboardService::with_backend(BlockingClipboard {
            started,
            release: may_finish,
        });
        let (reply, result) = crossbeam_channel::bounded(1);

        service.submit(ClipboardCommand::GetText {
            selection: ClipboardSelection::Clipboard,
            expires_at: std::time::Instant::now() + Duration::from_secs(5),
            reply,
        });

        has_started.recv().unwrap();
        assert_eq!(
            result.try_recv(),
            Err(crossbeam_channel::TryRecvError::Empty)
        );
        release.send(()).unwrap();
        assert_eq!(result.recv().unwrap(), Ok(Some("released".to_owned())));
    }

    #[test]
    fn service_shutdown_is_bounded_even_when_native_backend_drop_stalls() {
        let (drop_started, has_started) = crossbeam_channel::bounded(1);
        let (release_drop, may_finish) = crossbeam_channel::bounded(1);
        let service = ClipboardService::with_backend(BlockingDropClipboard {
            drop_started,
            release_drop: may_finish,
        });

        let started = std::time::Instant::now();
        drop(service);

        assert!(
            started.elapsed() < Duration::from_secs(2),
            "clipboard shutdown must not hang the display event loop"
        );
        has_started.recv().unwrap();
        release_drop.send(()).unwrap();
    }

    #[test]
    fn mutation_that_expires_behind_a_slow_read_is_never_executed() {
        let (read_started, has_started) = crossbeam_channel::bounded(1);
        let (release_read, may_finish) = crossbeam_channel::bounded(1);
        let (writes, recorded_writes) = crossbeam_channel::unbounded();
        let service = ClipboardService::with_backend(ExpiringClipboard {
            read_started,
            release_read: may_finish,
            writes,
        });
        let (read_reply, read_result) = crossbeam_channel::bounded(1);
        service.submit(ClipboardCommand::GetText {
            selection: ClipboardSelection::Clipboard,
            expires_at: std::time::Instant::now() + Duration::from_secs(5),
            reply: read_reply,
        });
        has_started.recv().unwrap();

        let (set_reply, set_result) = crossbeam_channel::bounded(1);
        service.submit(ClipboardCommand::SetText {
            selection: ClipboardSelection::Clipboard,
            text: Some("must not be published".to_owned()),
            expires_at: std::time::Instant::now(),
            reply: set_reply,
        });
        release_read.send(()).unwrap();

        assert_eq!(read_result.recv().unwrap(), Ok(None));
        assert_eq!(
            set_result.recv().unwrap(),
            Err("clipboard request expired before execution".to_owned())
        );
        assert_eq!(
            recorded_writes.try_recv(),
            Err(crossbeam_channel::TryRecvError::Empty)
        );
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn private_pasteboard_round_trips_store_load_and_clear() {
        let mut pasteboard = PrivatePasteboard::default();
        assert_eq!(pasteboard.load(), None);

        pasteboard.store(Some("selected"));
        assert_eq!(pasteboard.load(), Some("selected".to_owned()));

        pasteboard.store(Some("reselected"));
        assert_eq!(pasteboard.load(), Some("reselected".to_owned()));

        pasteboard.store(None);
        assert_eq!(pasteboard.load(), None);
    }

    /// GNU's NS port keeps PRIMARY in a private pasteboard instead of
    /// rejecting it (emacs-31.0.90 src/nsselect.m:56, :547), so every
    /// region deactivation under `select-active-regions` must succeed here.
    /// Only PRIMARY is touched: the system CLIPBOARD is left alone.
    #[cfg(not(target_os = "linux"))]
    #[test]
    fn arboard_backend_keeps_primary_in_a_private_pasteboard() {
        let mut backend = ArboardClipboard::new().expect("system clipboard should open");

        backend
            .set_text(ClipboardSelection::Primary, Some("selected"))
            .expect("PRIMARY store must not fail on this platform");
        assert_eq!(
            backend.text(ClipboardSelection::Primary),
            Ok(Some("selected".to_owned()))
        );

        backend
            .set_text(ClipboardSelection::Primary, None)
            .expect("PRIMARY disown must not fail on this platform");
        assert_eq!(backend.text(ClipboardSelection::Primary), Ok(None));
    }
}
