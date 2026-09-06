//! Platform-neutral types at the file-notification boundary.
//!
//! A backend chooses its own request and event types through associated types.
//! That keeps platform-specific flag vocabularies out of the common state
//! machine and makes cross-platform request mismatches a compile-time error.

use crate::emacs_core::error::Flow;
use crate::emacs_core::process::WaitNotifier;
use crate::emacs_core::value::Value;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Evaluator-owned Lisp state for one native watch.
///
/// Every platform registers the exact public filename object for its low-level
/// API (the original inotify argument, or the normalized kqueue/w32 name).
/// Keeping this required avoids a representable "backend needs a filename but
/// none was rooted" state and keeps all Lisp values off native worker threads.
#[derive(Clone, Copy, Debug)]
pub(super) struct WatchRegistration {
    callback: Value,
    registered_file_name: Value,
}

impl WatchRegistration {
    pub(super) fn new(callback: Value, registered_file_name: Value) -> Self {
        Self {
            callback,
            registered_file_name,
        }
    }

    pub(super) fn callback(self) -> Value {
        self.callback
    }

    pub(super) fn registered_file_name(self) -> Value {
        self.registered_file_name
    }

    pub(super) fn collect_gc_roots(self, roots: &mut Vec<Value>) {
        roots.extend([self.callback, self.registered_file_name]);
    }
}

/// Stable identity for one native watch registration.
///
/// The generation is part of the identity even on backends whose current Lisp
/// representation omits it.  This prevents a future descriptor reuse policy
/// from accidentally delivering a stale event to a newer registration.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct WatchId {
    slot: i64,
    generation: i64,
}

impl WatchId {
    pub(super) fn new(slot: i64, generation: i64) -> Self {
        debug_assert!(slot >= 0);
        debug_assert!(generation >= 0);
        Self { slot, generation }
    }

    pub(super) fn to_inotify_lisp(&self) -> Value {
        Value::cons(Value::fixnum(self.slot()), Value::fixnum(self.generation()))
    }

    pub(super) fn slot(&self) -> i64 {
        self.slot
    }

    pub(super) fn generation(&self) -> i64 {
        self.generation
    }
}

/// Monotonic logical identity source shared by every native backend.
///
/// Native descriptors may be reused; logical slots are never reused within an
/// evaluator lifetime, so stale events cannot alias a later registration.
#[derive(Default)]
pub(super) struct WatchIdAllocator {
    next_slot: i64,
}

impl WatchIdAllocator {
    pub(super) fn allocate(&mut self) -> WatchId {
        let slot = self.next_slot;
        self.next_slot = self
            .next_slot
            .checked_add(1)
            .expect("file notification descriptor space exhausted");
        WatchId::new(slot, 0)
    }
}

/// Shared monotonic native-watch lifecycle.
///
/// A worker flips this token while publishing a lossless terminal control
/// record, so `valid-p` changes immediately. Registry retirement is driven by
/// consuming that control record rather than by polling this atomic alone.
#[derive(Clone, Debug)]
pub(super) struct WatchActivity(Arc<AtomicBool>);

impl WatchActivity {
    pub(super) fn active() -> Self {
        Self(Arc::new(AtomicBool::new(true)))
    }

    pub(super) fn terminate(&self) {
        self.0.store(false, Ordering::Release);
    }

    pub(super) fn is_active(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    pub(super) fn same_registration(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

#[derive(Clone, Debug)]
pub(super) struct FileWatch<Request> {
    pub(super) id: WatchId,
    pub(super) request: Request,
}

/// Native-watch state needed by the shared drain lifecycle.
pub(super) trait TrackedWatch {
    fn watch_id(&self) -> &WatchId;
}

/// Run overflow recovery while every affected registration is still present,
/// then retire watches acknowledged by lossless control records.
///
/// Keeping this ordering in one cross-platform operation prevents a terminal
/// event burst from losing callbacks during conservative recovery. Atomic
/// activity is intentionally not consulted here: only a consumed control
/// record is a safe cross-thread retirement acknowledgment.
pub(super) fn finish_watch_drain<Watch: TrackedWatch>(
    watches: &mut Vec<Watch>,
    terminated: &[WatchId],
    before_retire: impl FnOnce(&mut [Watch]),
) {
    before_retire(watches);
    let terminated = terminated.iter().collect::<hashbrown::HashSet<_>>();
    watches.retain(|watch| !terminated.contains(watch.watch_id()));
}

pub(super) trait BackendEvent {
    fn watch_id(&self) -> &WatchId;
    fn into_lisp(
        self,
        ctx: &crate::emacs_core::eval::Context,
        registration: WatchRegistration,
    ) -> Value;
}

/// One atomic handoff from a native backend to the evaluator.
///
/// Watch termination is control-plane state, not an attribute of a
/// Lisp-visible event.  Keeping it separate ensures callback roots are
/// released even when a terminal native notification produces no event for
/// the watch's requested action set.
pub(super) struct DrainBatch<Event> {
    pub(super) events: Vec<Event>,
    pub(super) terminated: Vec<WatchId>,
    /// An asynchronous backend failure observed in the same drain.
    /// Lifecycle reconciliation and already-published events still happen
    /// before this error is returned to the evaluator.
    pub(super) failure: Option<Flow>,
}

/// Result of removing one logical watch.
///
/// Some native APIs can fail after the public descriptor has already been
/// retired (GNU inotify deliberately has this behavior). Keeping that state in
/// the type forces Lisp adapters to release roots before propagating the error.
pub(super) enum RemoveWatchOutcome {
    NotFound,
    Removed,
    RemovedWithError(Flow),
}

pub(super) trait Backend {
    type Request;
    type Event: BackendEvent;

    fn add_watch(
        &mut self,
        path: &Path,
        request: Self::Request,
        notifier: Option<WaitNotifier>,
    ) -> Result<WatchId, Flow>;
    fn remove_watch(&mut self, watch_id: &WatchId) -> RemoveWatchOutcome;
    fn valid_p(&self, watch_id: &WatchId) -> bool;
    fn drain_events(&mut self) -> Result<DrainBatch<Self::Event>, Flow>;
    fn has_watches(&self) -> bool;
}

#[cfg(test)]
#[path = "tests/model.rs"]
mod tests;
