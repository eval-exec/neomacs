//! Evaluator-thread ownership for Lisp operands retained by native caches.
//! A lease carries no API for reading its values and cannot cross threads.
//! The evaluator owns only weak references, so cancellation and cache eviction
//! release roots without borrowing the evaluator or modifying the specpdl.

use crate::emacs_core::{Context, Value};
use std::cell::RefCell;
use std::rc::{Rc, Weak};

/// Keeps a captured object graph alive until the last evaluator-side owner
/// retires. Owned worker data must never contain this handle.
#[derive(Clone)]
pub struct OwnedRoots {
    values: Rc<[Value]>,
}

impl std::fmt::Debug for OwnedRoots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OwnedRoots")
            .field("len", &self.values.len())
            .finish()
    }
}

type RootLeases = RefCell<Vec<Weak<[Value]>>>;

#[cfg(test)]
thread_local! { static LEASE_PRUNING_INSPECTIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }

#[repr(C)]
#[derive(Default)]
pub(crate) struct OwnedRootRegistry {
    leases: Rc<RootLeases>,
    // Context's pinned repr(C) ABI reserves the former inline RefCell<Vec>
    // slot. Sharing the registry must not move any subsequent Context field.
    _context_abi_reserved: [usize; 3],
}
const _: () =
    assert!(std::mem::size_of::<OwnedRootRegistry>() == std::mem::size_of::<RootLeases>());
const _: () =
    assert!(std::mem::align_of::<OwnedRootRegistry>() == std::mem::align_of::<RootLeases>());

/// Evaluator-local root registration without borrowing the entire Context.
/// This weak factory cannot prolong its evaluator or cross worker threads.
#[derive(Clone, Debug)]
pub struct OwnedRootRetainer {
    leases: Weak<RootLeases>,
}
impl OwnedRootRetainer {
    /// Retain already-live operands, or decline after the original evaluator
    /// has retired. This grants lifetime only, never mutation freshness.
    pub fn retain(&self, values: impl IntoIterator<Item = Value>) -> Option<OwnedRoots> {
        let leases = self.leases.upgrade()?;
        Some(OwnedRootRegistry::retain_in(&leases, values))
    }
}

impl OwnedRootRegistry {
    fn retain(&self, values: impl IntoIterator<Item = Value>) -> OwnedRoots {
        Self::retain_in(&self.leases, values)
    }

    /// Native owners hold strong values; the original evaluator traces only
    /// live weak leases. Registration never transfers evaluator ownership.
    fn retain_in(leases: &RootLeases, values: impl IntoIterator<Item = Value>) -> OwnedRoots {
        let values: Rc<[Value]> = values
            .into_iter()
            .filter(|value| value.is_heap_object())
            .collect();
        if !values.is_empty() {
            let mut leases = leases.borrow_mut();
            // Reclaim an expired suffix cheaply (including the usual single
            // transient lease), preserving prompt cleanup without rescanning
            // live predecessors. Each expired entry is popped at most once.
            while leases.last().is_some_and(|lease| {
                #[cfg(test)]
                LEASE_PRUNING_INSPECTIONS.with(|count| count.set(count.get() + 1));
                lease.strong_count() == 0
            }) {
                drop(leases.pop());
            }
            // Sweep at capacity, not on every registration. If most leases
            // remain live, grow as well: freeing only one dead slot must not
            // cause another full scan on the very next insertion. Either
            // branch leaves at least half a buffer of registration headroom,
            // so total pruning work is amortized linear. GC tracing also
            // removes dead leases; weak entries never keep Lisp graphs live.
            if leases.len() == leases.capacity() {
                leases.retain(|lease| {
                    #[cfg(test)]
                    LEASE_PRUNING_INSPECTIONS.with(|count| count.set(count.get() + 1));
                    lease.strong_count() != 0
                });
                if leases.len() > leases.capacity() / 2 {
                    let additional = leases.capacity();
                    leases.reserve(additional);
                }
            }
            leases.push(Rc::downgrade(&values));
        }
        OwnedRoots { values }
    }

    pub(crate) fn trace_roots(&self, visit: &mut dyn FnMut(Value)) {
        self.leases.borrow_mut().retain(|lease| {
            let Some(values) = lease.upgrade() else {
                return false;
            };
            for &value in values.iter() {
                visit(value);
            }
            true
        });
    }
}

impl Context {
    /// Register roots while other display services borrow the Context. The
    /// returned factory is weak and remains tied to this evaluator's registry.
    pub fn owned_root_retainer(&self) -> OwnedRootRetainer {
        OwnedRootRetainer {
            leases: Rc::downgrade(&self.owned_roots.leases),
        }
    }

    /// Retain already-live operands from this evaluator. This grants object
    /// lifetime only: callers must separately validate mutation and layout
    /// generations before using derived data.
    pub fn retain_gc_roots(&self, values: impl IntoIterator<Item = Value>) -> OwnedRoots {
        self.owned_roots.retain(values)
    }
}

#[cfg(test)]
#[path = "tests/owned_roots_test.rs"]
mod tests;
