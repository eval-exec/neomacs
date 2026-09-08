use super::*;
use crate::emacs_core::heap_registry::{HeapRegistryHandle, HeapRegistrySlot};

mod delivery;
mod lisp;
mod model;
mod platform;
mod registry;

// Private to the subsystem: every caller is inside `file_notify`, and keeping
// it here means minting a `file-notify-error` stays within the module that
// owns `ErrorDetail`.  Descendants still reach it through `use super::*`.
use lisp::file_notify_error;
use model::{
    Backend as FileNotifyBackend, BackendEvent as FileNotifyEvent, DrainBatch, ErrorDetail,
    FileWatch, RemoveWatchOutcome, TrackedWatch, WatchActivity, WatchId, WatchIdAllocator,
    WatchRegistration, finish_watch_drain,
};
use registry::WatchRegistry;

std::cfg_select! {
    target_os = "linux" => {
        pub(crate) use platform::linux::{inotify_add_watch, inotify_rm_watch, inotify_valid_p};
    }
    target_os = "macos" => {
        pub(crate) use platform::macos::{kqueue_add_watch, kqueue_rm_watch, kqueue_valid_p};
    }
    target_os = "windows" => {
        pub(crate) use platform::windows::{w32notify_add_watch, w32notify_rm_watch, w32notify_valid_p};
    }
    _ => {}
}

// Unsupported native targets still participate in startup registration with
// an empty table; they must not advertise another platform's watch primitives.
mod subrs;

#[cfg(test)]
pub(crate) use self::subrs::SUBRS;
pub(crate) use self::subrs::register_subrs;

#[cfg(all(test, target_os = "linux"))]
#[path = "tests/linux.rs"]
mod linux_test;

#[cfg(all(
    test,
    any(target_os = "linux", target_os = "macos", target_os = "windows")
))]
#[path = "tests/native_runtime.rs"]
mod native_runtime_test;

thread_local! {
    static FILE_NOTIFY_STATE: HeapRegistrySlot<FileNotifyState> = HeapRegistrySlot::new(FileNotifyState::default());
}

#[derive(Default)]
pub(crate) struct FileNotifyState {
    backend: PlatformBackend,
    registry: WatchRegistry,
}

type PlatformBackend = platform::Backend;

pub(crate) fn reset_file_notify_thread_locals() {
    FILE_NOTIFY_STATE.with(|slot| slot.reset(FileNotifyState::default()));
}

pub(crate) type FileNotifyRegistryHandle = HeapRegistryHandle<FileNotifyState>;

pub(crate) fn current_file_notify_registry_handle() -> FileNotifyRegistryHandle {
    FILE_NOTIFY_STATE.with(HeapRegistrySlot::current)
}

pub(crate) fn install_file_notify_registry_handle(handle: &FileNotifyRegistryHandle) {
    FILE_NOTIFY_STATE.with(|slot| slot.install(handle));
}

pub(crate) fn collect_file_notify_registry_gc_roots(
    registry: &FileNotifyRegistryHandle,
    group: &mut Vec<Value>,
) {
    registry.borrow().registry.collect_gc_roots(group);
}

pub(crate) fn has_active_file_notify_watches() -> bool {
    FILE_NOTIFY_STATE.with(|slot| slot.borrow().backend.has_watches())
}

fn prepare_deliveries<Event: FileNotifyEvent>(
    registry: &WatchRegistry,
    batch: DrainBatch<Event>,
) -> (Vec<(Event, WatchRegistration)>, Vec<WatchId>, Option<Flow>) {
    // Capture evaluator-owned registration data before unregistering terminal
    // watches so the final event, when present, is still delivered exactly once.
    let deliverable = batch
        .events
        .into_iter()
        .filter_map(|event| {
            registry
                .registration(event.watch_id())
                .map(|registration| (event, registration))
        })
        .collect();
    (deliverable, batch.terminated, batch.failure)
}

pub(crate) fn drain_file_notify_events(
    ctx: &mut crate::emacs_core::eval::Context,
) -> Result<usize, Flow> {
    let (events, terminated, failure) = FILE_NOTIFY_STATE.with(|slot| {
        let mut state = slot.borrow_mut();
        let batch = state.backend.drain_events()?;
        Ok::<_, Flow>(prepare_deliveries(&state.registry, batch))
    })?;
    let count = events.len();

    for (event, registration) in events {
        let raw_event = event.into_lisp(ctx, registration);
        ctx.queue_special_event(Value::list(vec![
            Value::symbol("file-notify"),
            raw_event,
            registration.callback(),
        ]));
    }

    // Encoding allocates Lisp lists, so terminal registrations must remain in
    // the GC root registry until every final event and callback wrapper has
    // been constructed and queued.
    FILE_NOTIFY_STATE.with(|slot| {
        let mut state = slot.borrow_mut();
        for watch_id in terminated {
            state.registry.unregister(&watch_id);
        }
    });

    match failure {
        Some(error) => Err(error),
        None => Ok(count),
    }
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/gc_tls_ownership.rs"]
mod gc_tls_ownership_tests;
