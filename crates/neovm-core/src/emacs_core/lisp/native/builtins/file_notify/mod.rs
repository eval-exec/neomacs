use super::*;
use std::cell::RefCell;

mod delivery;
mod lisp;
mod model;
mod platform;
mod registry;

pub(super) use lisp::file_notify_error;
use model::{
    Backend as FileNotifyBackend, BackendEvent as FileNotifyEvent, DrainBatch, FileWatch,
    RemoveWatchOutcome, TrackedWatch, WatchActivity, WatchId, WatchIdAllocator, WatchRegistration,
    finish_watch_drain,
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

std::cfg_select! {
    any(target_os = "linux", target_os = "macos", target_os = "windows") => {
        mod subrs;

        #[cfg(test)]
        pub(crate) use self::subrs::SUBRS;
        pub(crate) use self::subrs::register_subrs;
    }
    _ => {}
}

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
    static FILE_NOTIFY_STATE: RefCell<FileNotifyState> = RefCell::new(FileNotifyState::default());
}

struct FileNotifyState {
    backend: PlatformBackend,
    registry: WatchRegistry,
}

type PlatformBackend = platform::Backend;

impl Default for FileNotifyState {
    fn default() -> Self {
        Self {
            backend: PlatformBackend::default(),
            registry: WatchRegistry::default(),
        }
    }
}

pub(crate) fn reset_file_notify_thread_locals() {
    FILE_NOTIFY_STATE.with(|slot| *slot.borrow_mut() = FileNotifyState::default());
}

pub(crate) fn collect_file_notify_gc_roots(group: &mut Vec<Value>) {
    FILE_NOTIFY_STATE.with(|slot| {
        slot.borrow().registry.collect_gc_roots(group);
    });
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
