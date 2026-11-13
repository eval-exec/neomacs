//! P1 F1 FILE-NOTIFY FENCE (peer 12:57Z ruling (c)): the test-support entry
//! points the re-homed fixture tests need.
//!
//! A fixture that injects events or drives the thread-local registry below
//! the Lisp layer is a forge, so it goes through these fenced operations
//! (AUTHORITY rows).  The originals stay unwidened: the model items' fields
//! are private, `FILE_NOTIFY_STATE` is a private thread-local, and the
//! production functions keep their `pub(crate)` paths.  The crate-level
//! `neovm_eval::test_support::authority` re-exports these with the signed
//! rows.

use super::{DrainBatch, FILE_NOTIFY_STATE, FileNotifyEvent, WatchId, WatchRegistration};
use neovm_value::error::Flow;
use neovm_value::value::Value;

/// Forge one synthetic drain batch for the fixture tests.
pub fn drain_batch_for_test<Event: FileNotifyEvent>(
    events: Vec<Event>,
    terminated: Vec<WatchId>,
    failure: Option<Flow>,
) -> DrainBatch<Event> {
    DrainBatch {
        events,
        terminated,
        failure,
    }
}

/// Install one registration in the process thread-local registry.
pub fn file_notify_register_for_test(watch_id: WatchId, callback: Value, file_name: Value) {
    FILE_NOTIFY_STATE.with(|slot| {
        slot.borrow_mut()
            .registry
            .register(watch_id, callback, file_name);
    });
}

/// Drop one registration from the process thread-local registry.
pub fn file_notify_unregister_for_test(watch_id: &WatchId) {
    FILE_NOTIFY_STATE.with(|slot| {
        slot.borrow_mut().registry.unregister(watch_id);
    });
}

/// Forge an inotify event with a kernel-supplied native child name and
/// return its evaluator encoding.  The owning platform module constructs the
/// event (its fields stay private); this fence is the only public route.
#[cfg(target_os = "linux")]
pub fn inotify_event_native_name_lisp_for_test(
    ctx: &crate::emacs_core::eval::Context,
    watch_id: WatchId,
    aspects: Vec<&'static str>,
    raw_name: Vec<u8>,
    cookie: u32,
    registration: WatchRegistration,
) -> Value {
    super::platform::linux::inotify_event_native_name_lisp_for_test(
        ctx,
        watch_id,
        aspects,
        raw_name,
        cookie,
        registration,
    )
}

/// Forge the nameless inotify terminal event and return its evaluator
/// encoding (it reuses the registered Lisp filename object).
#[cfg(target_os = "linux")]
pub fn inotify_event_registered_name_lisp_for_test(
    ctx: &crate::emacs_core::eval::Context,
    watch_id: WatchId,
    aspects: Vec<&'static str>,
    cookie: u32,
    registration: WatchRegistration,
) -> Value {
    super::platform::linux::inotify_event_registered_name_lisp_for_test(
        ctx,
        watch_id,
        aspects,
        cookie,
        registration,
    )
}
