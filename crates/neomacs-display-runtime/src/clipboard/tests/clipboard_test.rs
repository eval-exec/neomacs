use super::*;
use neomacs_display_protocol::SelectionOwner;
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

    fn owner(&mut self, selection: ClipboardSelection) -> Result<SelectionOwner, String> {
        Ok(if self.selections.contains_key(&selection) {
            SelectionOwner::ThisProcess
        } else {
            SelectionOwner::None
        })
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

    fn owner(&mut self, _selection: ClipboardSelection) -> Result<SelectionOwner, String> {
        Ok(SelectionOwner::Unknown)
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

    fn owner(&mut self, _selection: ClipboardSelection) -> Result<SelectionOwner, String> {
        Ok(SelectionOwner::Unknown)
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

    fn owner(&mut self, _selection: ClipboardSelection) -> Result<SelectionOwner, String> {
        Ok(SelectionOwner::Unknown)
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

fn owner(
    service: &ClipboardService,
    selection: ClipboardSelection,
) -> Result<SelectionOwner, String> {
    let (reply, result) = crossbeam_channel::bounded(1);
    service.submit(ClipboardCommand::GetOwnership {
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
fn service_reports_empty_primary_as_owned_until_it_is_disowned() {
    let service = ClipboardService::with_backend(MemoryClipboard::default());

    assert_eq!(
        owner(&service, ClipboardSelection::Primary).unwrap(),
        SelectionOwner::None
    );

    set_text(&service, ClipboardSelection::Primary, Some("")).unwrap();
    assert_eq!(
        text(&service, ClipboardSelection::Primary).unwrap(),
        Some(String::new())
    );
    assert_eq!(
        owner(&service, ClipboardSelection::Primary).unwrap(),
        SelectionOwner::ThisProcess
    );

    set_text(&service, ClipboardSelection::Primary, None).unwrap();
    assert_eq!(
        owner(&service, ClipboardSelection::Primary).unwrap(),
        SelectionOwner::None
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
fn private_selection_round_trips_owned_and_vacant_states() {
    let mut selection = PrivateSelection::default();
    assert_eq!(selection.load(), None);

    selection.store(Some("selected"));
    assert_eq!(selection.load(), Some("selected".to_owned()));

    selection.store(Some("reselected"));
    assert_eq!(selection.load(), Some("reselected".to_owned()));

    selection.store(None);
    assert_eq!(selection.load(), None);
}

/// GNU's NS port keeps PRIMARY in a private pasteboard instead of
/// rejecting it (emacs-31.0.90 src/nsselect.m:56, :547), so every
/// region deactivation under `select-active-regions` must succeed here.
/// Only PRIMARY is touched: the system CLIPBOARD is left alone.
#[cfg(not(target_os = "linux"))]
#[test]
fn arboard_backend_keeps_primary_in_process_local_state() {
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
