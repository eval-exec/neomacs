use super::*;
use neovm_value::heap_registry::{HeapRegistryHandle, HeapRegistrySlot};

mod delivery;
mod lisp;
mod model;
mod platform;
// The compiled-in backend's primitive names, re-exported from the private
// selection module so tests can ask the build instead of naming an OS. The
// feature arm is the cross-crate test-support route; the other arm covers
// production and this crate's own cfg(test) build, so every build has
// exactly one binding.
#[cfg(feature = "test-support")]
pub use platform::{BACKEND_SUBRS, REMOVE_WATCH_SEMANTICS, RemoveWatchSemantics};
#[cfg(not(feature = "test-support"))]
pub(crate) use platform::{BACKEND_SUBRS, REMOVE_WATCH_SEMANTICS, RemoveWatchSemantics};
#[cfg(any(test, feature = "test-support"))]
pub(crate) mod authority;
mod registry;

// Private to the subsystem: every caller is inside `file_notify`, and keeping
// it here means minting a `file-notify-error` stays within the module that
// owns `ErrorDetail`.  Descendants still reach it through `use super::*`.
use lisp::file_notify_error;
// P1 F1: the re-homed file-notify fixture tests name these test-support
// types.  Exactly one binding per name: the feature arm is the cross-crate
// route, the other arm keeps production and this crate's cfg(test) build
// private.
use model::{
    Backend as FileNotifyBackend, ErrorDetail, FileWatch, RemoveWatchOutcome, TrackedWatch,
    WatchActivity, WatchIdAllocator, finish_watch_drain,
};
#[cfg(feature = "test-support")]
#[doc(hidden)]
pub use model::{BackendEvent as FileNotifyEvent, DrainBatch, WatchId, WatchRegistration};
#[cfg(not(feature = "test-support"))]
use model::{BackendEvent as FileNotifyEvent, DrainBatch, WatchId, WatchRegistration};
#[cfg(feature = "test-support")]
#[doc(hidden)]
pub use registry::WatchRegistry;
#[cfg(not(feature = "test-support"))]
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

std::cfg_select! {
    any(target_os = "linux", target_os = "macos", target_os = "windows") => {
        mod subrs;

        #[cfg(any(test, feature = "test-support"))]
        #[cfg_attr(feature = "test-support", visibility::make(pub))]
        pub(crate) use self::subrs::SUBRS;
        #[doc(hidden)]
        pub use self::subrs::register_subrs;
    }
    _ => {}
}

#[cfg(all(
    test,
    any(target_os = "linux", target_os = "macos", target_os = "windows")
))]
#[path = "tests/native_runtime_test.rs"]
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

/// Carrier park: forget the installed file-notify registry handle (a plain
/// store). The World's registry stays alive through its Contexts.
pub(crate) fn park_file_notify_thread_locals() {
    FILE_NOTIFY_STATE.with(HeapRegistrySlot::park);
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

#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn prepare_deliveries<Event: FileNotifyEvent>(
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

/// Observe-only read of the process thread-local registry for the re-homed
/// fixture tests (peer 14:47Z item 2): `prepare_deliveries` runs against the
/// BORROWED registry (no borrow_mut, no write), so this is a VIS accessor,
/// not an authority fence.  The synthetic batch it consumes can only be
/// constructed by the `drain_batch_for_test` authority forge.
#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn prepare_deliveries_for_test<Event: FileNotifyEvent>(
    batch: DrainBatch<Event>,
) -> (Vec<(Event, WatchRegistration)>, Vec<WatchId>, Option<Flow>) {
    FILE_NOTIFY_STATE.with(|slot| prepare_deliveries(&slot.borrow().registry, batch))
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
#[path = "tests/gc_tls_ownership_test.rs"]
mod gc_tls_ownership_tests;

/// Debug-only carrier TLS fingerprint for this module's classified records.
#[cfg(debug_assertions)]
pub(crate) fn carrier_tls_fingerprint(h: &mut u64) {
    neovm_value::tls_fingerprint::mix(
        h,
        FILE_NOTIFY_STATE.with(|slot| slot.installed_heap_identity().unwrap_or(0) as u64),
    );
}
