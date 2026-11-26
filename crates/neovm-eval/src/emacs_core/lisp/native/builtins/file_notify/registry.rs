//! Evaluator-owned Lisp watch registry.
//!
//! Native watcher threads never hold Lisp values.  Callback lookup and every
//! Lisp object that native events may need to reuse stay GC-rooted on the
//! evaluator thread behind this small ownership boundary.

use super::{WatchId, WatchRegistration};
use hashbrown::HashMap;
use neovm_value::value::Value;
use std::sync::atomic::{AtomicBool, Ordering};

/// Set once any watch is registered in the process so `drain_file_notify_events`
/// can short-circuit in one atomic load on sessions that never use file-notify.
pub(super) static FILE_NOTIFY_MAY_BE_ACTIVE: AtomicBool = AtomicBool::new(false);

#[derive(Default)]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) struct WatchRegistry {
    registrations: HashMap<WatchId, WatchRegistration>,
}

impl WatchRegistry {
    #[doc(hidden)]
    pub fn register(&mut self, watch_id: WatchId, callback: Value, file_name: Value) {
        FILE_NOTIFY_MAY_BE_ACTIVE.store(true, Ordering::Release);
        let registration = WatchRegistration::new(callback, file_name);
        let replaced = self.registrations.insert(watch_id, registration);
        debug_assert!(replaced.is_none(), "native backend reused a live watch id");
    }

    #[doc(hidden)]
    pub fn unregister(&mut self, watch_id: &WatchId) {
        self.registrations.remove(watch_id);
    }

    #[doc(hidden)]
    pub fn registration(&self, watch_id: &WatchId) -> Option<WatchRegistration> {
        self.registrations.get(watch_id).copied()
    }

    #[doc(hidden)]
    pub fn collect_gc_roots(&self, roots: &mut Vec<Value>) {
        for registration in self.registrations.values() {
            registration.collect_gc_roots(roots);
        }
    }
}
