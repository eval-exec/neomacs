//! Exact `ReadDirectoryChangesW` ownership behind a small safe interface.
//!
//! `notify` deliberately normalizes Windows events and fixes one broad native
//! filter. GNU's low-level `w32notify-*` API exposes every native filter,
//! including last-access time, so this adapter owns the OS request directly.

use super::super::super::delivery::{DeliverySender, PublishOutcome};
use super::super::super::{WatchActivity, WatchId};
use super::{W32Event, codec};
use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::mpsc::{SyncSender, sync_channel};
use std::thread::JoinHandle;
use windows_sys::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_OPERATION_ABORTED, HANDLE, INVALID_HANDLE_VALUE, WAIT_FAILED,
    WAIT_OBJECT_0,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_FLAG_OVERLAPPED,
    FILE_LIST_DIRECTORY, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    ReadDirectoryChangesW,
};
use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows_sys::Win32::System::Threading::{
    CreateEventW, INFINITE, SetEvent, WaitForMultipleObjects,
};

const BUFFER_CAPACITY: usize = 64 * 1024;

pub(super) enum WorkerMessage {
    Event(W32Event),
    Overflow(WatchId),
}

/// Why an already-armed Windows watch stopped.
///
/// GNU exposes initial registration errors synchronously, but an asynchronous
/// worker exit only invalidates the descriptor (`w32notify-valid-p' becomes
/// nil). Keeping expected invalidation distinct from an abnormal worker exit
/// lets the evaluator retire both without turning either into an unrelated
/// Lisp read error, while still logging unexpected failures.
pub(super) enum WorkerTermination {
    Invalidated { watch_id: WatchId },
    Failed { watch_id: WatchId, error: String },
}

enum StartupReport {
    Ready,
    Failed(String),
}

pub(super) struct Worker {
    stop_event: OwnedHandle,
    join: Option<JoinHandle<()>>,
}

impl Worker {
    pub(super) fn start(
        path: &Path,
        recursive: bool,
        native_filter: u32,
        watch_id: WatchId,
        activity: WatchActivity,
        events: DeliverySender<WorkerMessage, WorkerTermination>,
    ) -> Result<Self, String> {
        let (directory, watched_name) = if path.is_dir() {
            (path.to_path_buf(), None)
        } else {
            let parent = path
                .parent()
                .ok_or_else(|| "watched file has no parent directory".to_owned())?;
            (parent.to_path_buf(), path.file_name().map(PathBuf::from))
        };
        let directory_handle = open_directory(&directory)?;
        let io_event = create_event()?;
        let stop_event = create_event()?;
        let worker_stop_event = stop_event.try_clone().map_err(|error| error.to_string())?;
        let (startup_tx, startup_rx) = sync_channel(1);

        let mut join = Some(
            std::thread::Builder::new()
                .name("neomacs-w32notify".to_owned())
                .spawn(move || {
                    run(
                        directory_handle,
                        io_event,
                        worker_stop_event,
                        watched_name,
                        recursive,
                        native_filter,
                        watch_id,
                        activity,
                        events,
                        startup_tx,
                    );
                })
                .map_err(|error| error.to_string())?,
        );
        match startup_rx.recv() {
            Ok(StartupReport::Ready) => Ok(Self { stop_event, join }),
            Ok(StartupReport::Failed(error)) => {
                let _ = join.take().expect("worker thread was started").join();
                Err(error)
            }
            Err(_) => {
                let _ = join.take().expect("worker thread was started").join();
                Err("Windows file-notification worker exited during startup".to_owned())
            }
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        // SAFETY: this controller owns the event handle. The worker owns a
        // duplicate referring to the same event object until `join` completes.
        unsafe {
            SetEvent(self.stop_event.as_raw_handle() as HANDLE);
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn run(
    directory: OwnedHandle,
    io_event: OwnedHandle,
    stop_event: OwnedHandle,
    watched_name: Option<PathBuf>,
    recursive: bool,
    native_filter: u32,
    watch_id: WatchId,
    activity: WatchActivity,
    events: DeliverySender<WorkerMessage, WorkerTermination>,
    startup: SyncSender<StartupReport>,
) {
    let directory_raw = directory.as_raw_handle() as HANDLE;
    let io_event_raw = io_event.as_raw_handle() as HANDLE;
    let stop_event_raw = stop_event.as_raw_handle() as HANDLE;
    let handles = [stop_event_raw, io_event_raw];
    let mut buffer = vec![0_u8; BUFFER_CAPACITY];
    let mut startup = Some(startup);

    loop {
        let mut overlapped = OVERLAPPED {
            hEvent: io_event_raw,
            ..OVERLAPPED::default()
        };
        let mut pending = match PendingDirectoryRead::start(
            directory_raw,
            &mut buffer,
            &mut overlapped,
            recursive,
            native_filter,
        ) {
            Ok(pending) => pending,
            Err(error) => {
                if let Some(startup) = startup.take() {
                    let _ = startup.send(StartupReport::Failed(error.to_string()));
                    return;
                }
                finish_after_io_error(&activity, events, watch_id, error);
                return;
            }
        };
        if let Some(startup) = startup.take()
            && startup.send(StartupReport::Ready).is_err()
        {
            return;
        }

        // SAFETY: both event handles stay owned for the full wait.
        let ready = unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, INFINITE) };
        if ready == WAIT_OBJECT_0 {
            // `pending` cancels and joins the I/O operation before its
            // borrowed OVERLAPPED and buffer can leave scope.
            return;
        }
        if ready == WAIT_FAILED || ready != WAIT_OBJECT_0 + 1 {
            let error = if ready == WAIT_FAILED {
                std::io::Error::last_os_error().to_string()
            } else {
                format!("unexpected wait result {ready}")
            };
            finish_after_worker_failure(&activity, events, watch_id, error);
            return;
        }

        let bytes = match pending.complete() {
            Ok(bytes) => bytes,
            Err(error) => {
                if error.raw_os_error() == Some(ERROR_OPERATION_ABORTED as i32) {
                    return;
                }
                finish_after_io_error(&activity, events, watch_id, error);
                return;
            }
        };
        if bytes == 0 {
            if events.publish(WorkerMessage::Overflow(watch_id.clone())) == PublishOutcome::Closed {
                return;
            }
            continue;
        }

        let decoded = match codec::decode(pending.completed_bytes(bytes)) {
            Ok(decoded) => decoded,
            Err(error) => {
                tracing::warn!(%error, "malformed Windows file-notification batch; rescanning");
                if events.publish(WorkerMessage::Overflow(watch_id.clone()))
                    == PublishOutcome::Closed
                {
                    return;
                }
                continue;
            }
        };
        for (action, path) in decoded {
            if watched_name.as_ref().is_some_and(|name| *name != path) {
                continue;
            }
            if events.publish(WorkerMessage::Event(W32Event {
                watch_id: watch_id.clone(),
                action,
                path,
            })) == PublishOutcome::Closed
            {
                return;
            }
        }
    }
}

/// Owns Rust's borrows for one outstanding `ReadDirectoryChangesW` request.
///
/// The kernel may access both buffers until `GetOverlappedResult` observes
/// completion. Drop cancels and waits, making early returns and future edits
/// unable to free those buffers while the operation is still pending.
struct PendingDirectoryRead<'a> {
    directory: HANDLE,
    overlapped: &'a mut OVERLAPPED,
    buffer: &'a mut [u8],
    pending: bool,
}

impl<'a> PendingDirectoryRead<'a> {
    fn start(
        directory: HANDLE,
        buffer: &'a mut [u8],
        overlapped: &'a mut OVERLAPPED,
        recursive: bool,
        native_filter: u32,
    ) -> std::io::Result<Self> {
        // SAFETY: the returned guard borrows both writable buffers and waits
        // for cancellation/completion before releasing either borrow.
        let started = unsafe {
            ReadDirectoryChangesW(
                directory,
                buffer.as_mut_ptr().cast::<c_void>(),
                buffer.len() as u32,
                if recursive { 1 } else { 0 },
                native_filter,
                ptr::null_mut(),
                overlapped,
                None,
            )
        };
        if started == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Self {
            directory,
            overlapped,
            buffer,
            pending: true,
        })
    }

    fn complete(&mut self) -> std::io::Result<usize> {
        let mut bytes = 0;
        // SAFETY: this guard exclusively borrows the exact OVERLAPPED and
        // buffer supplied when the operation started.
        let completed =
            unsafe { GetOverlappedResult(self.directory, self.overlapped, &mut bytes, 0) };
        if completed == 0 {
            return Err(std::io::Error::last_os_error());
        }
        self.pending = false;
        Ok(bytes as usize)
    }

    fn completed_bytes(&self, bytes: usize) -> &[u8] {
        assert!(!self.pending, "file-notify I/O is still pending");
        &self.buffer[..bytes]
    }
}

impl Drop for PendingDirectoryRead<'_> {
    fn drop(&mut self) {
        if !self.pending {
            return;
        }
        // SAFETY: this guard owns the outstanding operation's borrows. Waiting
        // here keeps both allocations alive until the kernel has stopped using
        // them, whether cancellation succeeds or races with completion.
        unsafe {
            CancelIoEx(self.directory, self.overlapped);
            let mut ignored = 0;
            GetOverlappedResult(self.directory, self.overlapped, &mut ignored, 1);
        }
        self.pending = false;
    }
}

fn finish_after_io_error(
    activity: &WatchActivity,
    events: DeliverySender<WorkerMessage, WorkerTermination>,
    watch_id: WatchId,
    error: std::io::Error,
) {
    let termination = if error.raw_os_error() == Some(ERROR_ACCESS_DENIED as i32) {
        WorkerTermination::Invalidated { watch_id }
    } else {
        WorkerTermination::Failed {
            watch_id,
            error: error.to_string(),
        }
    };
    finish(activity, events, termination);
}

fn finish_after_worker_failure(
    activity: &WatchActivity,
    events: DeliverySender<WorkerMessage, WorkerTermination>,
    watch_id: WatchId,
    error: String,
) {
    finish(
        activity,
        events,
        WorkerTermination::Failed { watch_id, error },
    );
}

fn finish(
    activity: &WatchActivity,
    events: DeliverySender<WorkerMessage, WorkerTermination>,
    termination: WorkerTermination,
) {
    events.finish_with(termination, || activity.terminate());
}

fn open_directory(path: &Path) -> Result<OwnedHandle, String> {
    let wide = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    // SAFETY: `wide` is a live NUL-terminated UTF-16 path; no security or
    // template pointers are supplied. Ownership transfers to OwnedHandle.
    let raw = unsafe {
        CreateFileW(
            wide.as_ptr(),
            FILE_LIST_DIRECTORY,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OVERLAPPED | FILE_FLAG_OPEN_REPARSE_POINT,
            ptr::null_mut(),
        )
    };
    if raw == INVALID_HANDLE_VALUE {
        return Err(std::io::Error::last_os_error().to_string());
    }
    // SAFETY: CreateFileW returned a new, valid, uniquely owned handle.
    Ok(unsafe { OwnedHandle::from_raw_handle(raw) })
}

fn create_event() -> Result<OwnedHandle, String> {
    // SAFETY: null attributes/name create an unnamed auto-reset event.
    let raw = unsafe { CreateEventW(ptr::null(), 0, 0, ptr::null()) };
    if raw.is_null() {
        return Err(std::io::Error::last_os_error().to_string());
    }
    // SAFETY: CreateEventW returned a new, valid, uniquely owned handle.
    Ok(unsafe { OwnedHandle::from_raw_handle(raw) })
}
