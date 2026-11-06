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

use super::{DrainBatch, FILE_NOTIFY_STATE, FileNotifyEvent, WatchId};
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
