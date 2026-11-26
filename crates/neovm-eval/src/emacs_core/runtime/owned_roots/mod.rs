//! Evaluator-thread ownership for Lisp operands retained by native caches.
//! A lease carries no API for reading its values and cannot cross threads.
//! The evaluator owns only weak references, so cancellation and cache eviction
//! release roots without borrowing the evaluator or modifying the specpdl.

use crate::emacs_core::eval::Context;
use neovm_edit::buffer::TextPropertyRoots;
use neovm_edit::buffer::buffer::BufferStorage;
use neovm_value::value::Value;
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

// This payload lives behind the existing single Rc/Weak registry pointer;
// neither Context's pinned slot nor OwnedRootRetainer grows.
#[derive(Default)]
struct RootLeases {
    values: RefCell<Vec<Weak<[Value]>>>,
    text_properties: RefCell<Vec<Weak<TextPropertyLease>>>,
}

#[derive(Debug)]
struct TextPropertyLease {
    table: TextPropertyRoots,
}

/// Keeps only the native registration live; the associated snapshot owns its
/// interval table. Cloning a pending view preserves its registration.
#[derive(Clone, Debug)]
pub struct OwnedTextPropertyRoots {
    _lease: Rc<TextPropertyLease>,
}
static_assertions::assert_not_impl_any!(OwnedTextPropertyRoots: Send, Sync);

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
const _: () = assert!(
    std::mem::size_of::<OwnedRootRegistry>() == std::mem::size_of::<RefCell<Vec<Weak<[Value]>>>>()
);
const _: () = assert!(
    std::mem::align_of::<OwnedRootRegistry>()
        == std::mem::align_of::<RefCell<Vec<Weak<[Value]>>>>()
);

/// Evaluator-local root registration without borrowing the entire Context.
/// This weak factory cannot prolong its evaluator or cross worker threads.
#[derive(Clone, Debug)]
pub struct OwnedRootRetainer {
    leases: Weak<RootLeases>,
}
const _: () = assert!(std::mem::size_of::<OwnedRootRetainer>() == std::mem::size_of::<usize>());
#[cfg(target_pointer_width = "64")]
const _: () = {
    assert!(std::mem::size_of::<OwnedRootRegistry>() == 32);
    assert!(std::mem::size_of::<OwnedRootRetainer>() == 8);
};
impl OwnedRootRetainer {
    /// Register a held immutable property table without walking its operands.
    /// The evaluator traces detached tables only at a collection boundary.
    pub fn retain_text_properties(
        &self,
        table: TextPropertyRoots,
    ) -> Option<OwnedTextPropertyRoots> {
        let registry = self.leases.upgrade()?;
        let lease = Rc::new(TextPropertyLease { table });
        let mut leases = registry.text_properties.borrow_mut();
        while leases.last().is_some_and(|lease| lease.strong_count() == 0) {
            drop(leases.pop());
        }
        if leases.len() == leases.capacity() {
            leases.retain(|lease| lease.strong_count() != 0);
            if leases.len() > leases.capacity() / 2 {
                let additional = leases.capacity();
                leases.reserve(additional);
            }
        }
        leases.push(Rc::downgrade(&lease));
        Some(OwnedTextPropertyRoots { _lease: lease })
    }

    /// Retain already-live operands, or decline after the original evaluator
    /// has retired. This grants lifetime only, never mutation freshness.
    pub fn retain(&self, values: impl IntoIterator<Item = Value>) -> Option<OwnedRoots> {
        let leases = self.leases.upgrade()?;
        Some(OwnedRootRegistry::retain_in(&leases, values))
    }
}

impl OwnedRootRegistry {
    #[doc(hidden)]
    pub fn retain(&self, values: impl IntoIterator<Item = Value>) -> OwnedRoots {
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
            let mut leases = leases.values.borrow_mut();
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

    #[doc(hidden)]
    pub fn trace_roots_with_buffers(&self, buffers: &BufferStorage, visit: &mut dyn FnMut(Value)) {
        self.trace_value_roots(visit);
        self.trace_text_property_roots(buffers, visit);
    }

    fn trace_text_property_roots(&self, buffers: &BufferStorage, visit: &mut dyn FnMut(Value)) {
        let mut leases = self.leases.text_properties.borrow_mut();
        leases.retain(|weak| weak.strong_count() != 0);
        if leases.is_empty() {
            return;
        }
        // Ordinary GC already walks every live buffer. Collect their native
        // table identities once without touching contents, covering a held
        // killed-indirect view even when no lease names its live base.
        let current: rustc_hash::FxHashSet<_> =
            buffers.live_text_property_table_identities().collect();
        let mut seen = rustc_hash::FxHashSet::default();
        for weak in leases.iter() {
            let Some(lease) = weak.upgrade() else {
                continue;
            };
            let identity = lease.table.identity();
            if !current.contains(&identity) && seen.insert(identity) {
                lease.table.trace_roots(visit);
            }
        }
    }

    fn trace_value_roots(&self, visit: &mut dyn FnMut(Value)) {
        self.leases.values.borrow_mut().retain(|lease| {
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
