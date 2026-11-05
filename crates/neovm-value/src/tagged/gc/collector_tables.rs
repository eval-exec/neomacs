//! Collector table access for serial collections and the scoped proof-2 job.
//!
//! The parallel bridge retains the *real* AllStopped reference. Its erased
//! address lives only in the cold registry's lazy Cell, never TLS or the
//! hot heap layout. The installation guard clears it on return and unwind.
//! Raw collector table pointers still require liveness/stop-all admission;
//! this module does not turn arbitrary pointer callers into safe authorities.

use super::mutator_registration::{AllStopped, KeeperTableMut, RendezvousError};
use super::*;
use crate::value::LispHashTable;
use std::rc::Rc;

/// Copying this private designator does not extend its installed stop scope.
/// Every dereference is unsafe and requires the installation to remain live.
#[derive(Clone, Copy)]
pub(super) struct CollectorTables {
    stopped: *const (),
}

impl CollectorTables {
    /// # Safety
    /// `table` is live, and the heap's stopped-table installation (if any)
    /// still exists. The callback must not free this table or resume owners.
    /// The HRTB prevents any payload borrow from escaping the callback.
    pub(super) unsafe fn inspect<R>(
        self,
        table: *mut HashTableObj,
        f: impl for<'table> FnOnce(&'table LispHashTable) -> R,
    ) -> R {
        if self.stopped.is_null() {
            // SAFETY: serial collector admission and the caller's liveness.
            return unsafe { f(&(*table).table) };
        }
        // SAFETY: install retained this actual guard until its RAII cleanup;
        // only the pointer's lifetime was erased, not the stop-all authority.
        let stopped: &AllStopped<'_> = unsafe { &*self.stopped.cast() };
        let value = unsafe { TaggedValue::from_veclike_ptr(table.cast()) };
        KeeperTableMut::enter(stopped, value)
            .expect("collector pointer names a live hash table")
            .inspect(f)
    }

    /// # Safety
    /// Same scope/liveness contract as inspect, plus exclusive collector access
    /// to this table for the callback. No callbacks into Lisp or safepoints.
    pub(super) unsafe fn edit<R>(
        self,
        table: *mut HashTableObj,
        f: impl for<'table> FnOnce(&'table mut LispHashTable) -> R,
    ) -> R {
        if self.stopped.is_null() {
            // SAFETY: exclusive serial collector admission.
            return unsafe { f(&mut (*table).table) };
        }
        // SAFETY: same real-guard reborrow as inspect, scoped to this call.
        let stopped: &AllStopped<'_> = unsafe { &*self.stopped.cast() };
        let value = unsafe { TaggedValue::from_veclike_ptr(table.cast()) };
        KeeperTableMut::enter(stopped, value)
            .expect("collector pointer names a live hash table")
            .edit_storage(f)
    }
}

/// Installs the address of one real guard, retaining its borrow until cleanup.
/// The optional cold slot is allocated only for proof collections. An Rc clone
/// retains its Cell independently of any later heap/registry reborrow, so Drop
/// never dereferences a raw pointer invalidated by mutable registry access.
struct CollectorTableScope<'stop, 'era> {
    slot: Rc<Cell<*const ()>>,
    stopped: &'stop AllStopped<'era>,
}

/// Run all phases while retaining the actual stop guard. The RAII installation
/// stays private: callers cannot forget it and leave an erased pointer behind.
pub(super) fn with_stopped_tables<R>(
    heap: &mut TaggedHeap,
    stopped: &AllStopped<'_>,
    body: impl FnOnce(&mut TaggedHeap) -> R,
) -> Result<R, RendezvousError> {
    stopped.check_world_roots(heap.heap_identity())?;
    let slot = heap._satb_layout_reserve.collector_tables_slot();
    if !slot.get().is_null() {
        return Err(RendezvousError::Busy);
    }
    slot.set(std::ptr::from_ref(stopped).cast());
    let _scope = CollectorTableScope { slot, stopped };
    Ok(body(heap))
}

/// Cold collector trace entry. The job callback has not run when installation
/// refuses; callback errors remain a distinct inner Result at the call site.
pub(super) fn with_stopped_tables_for_trace<R>(
    heap: &mut TaggedHeap,
    stopped: &AllStopped<'_>,
    body: impl FnOnce(&mut TaggedHeap) -> R,
) -> Result<R, RendezvousError> {
    with_stopped_tables_for_trace_observing_busy(heap, stopped, body, || {})
}

/// The observer exposes only a refused pre-job attempt for the deterministic
/// restore-contention regression. Production supplies the no-op above.
pub(super) fn with_stopped_tables_for_trace_observing_busy<R>(
    heap: &mut TaggedHeap,
    stopped: &AllStopped<'_>,
    body: impl FnOnce(&mut TaggedHeap) -> R,
    mut on_busy: impl FnMut(),
) -> Result<R, RendezvousError> {
    // The unique keeper enters without control, entry or root-table guards.
    // A live scope is reentry, not another owner's transient gate contention.
    if !heap._satb_layout_reserve.collector_tables().is_null() {
        return Err(RendezvousError::Busy);
    }
    let mut body = Some(body);
    loop {
        // Outer errors precede the callback. A callback's own Err(Busy) is
        // inside R and must still make AllStopped::trace poison the era.
        match with_stopped_tables(heap, stopped, |heap| {
            body.take().expect("collector job runs once")(heap)
        }) {
            Err(RendezvousError::Busy) => {
                // A keeper callback retaining its own gate cannot make
                // progress by yielding. Preserve immediate refusal there.
                if stopped.gate_is_held_by_current_thread() {
                    return Err(RendezvousError::Busy);
                }
                debug_assert!(body.is_some(), "refused scope never invokes the job");
                on_busy();
                std::thread::yield_now();
            }
            result => return result,
        }
    }
}

impl Drop for CollectorTableScope<'_, '_> {
    fn drop(&mut self) {
        let installed = self.slot.replace(std::ptr::null());
        debug_assert_eq!(installed, std::ptr::from_ref(self.stopped).cast());
    }
}

impl TaggedHeap {
    /// Internal raw designator: use only synchronously within the current
    /// admitted collector operation. Its unsafe visits retain that obligation.
    pub(super) fn collector_tables(&self) -> CollectorTables {
        CollectorTables {
            stopped: self._satb_layout_reserve.collector_tables(),
        }
    }

    /// Collector accounting must inspect table backing under the same seal as
    /// mark/sweep. Allocation retains its existing serial static helper.
    pub(super) fn collector_object_bytes(&self, header: *const GcHeader) -> usize {
        // SAFETY: callers already require a live initialized object header.
        unsafe {
            if (*header).kind == HeapObjectKind::VecLike
                && (*(header.cast::<VecLikeHeader>())).type_tag == VecLikeType::HashTable
            {
                self.collector_tables()
                    .inspect(header.cast_mut().cast(), |table| {
                        size_of::<HashTableObj>().saturating_add(table.data.known_storage_bytes())
                    })
            } else {
                Self::object_bytes_from_header(header)
            }
        }
    }
}

static_assertions::assert_not_impl_any!(CollectorTableScope<'static, 'static>: Send, Sync, Clone, Copy);

#[cfg(test)]
mod tests {
    /// The raw adapter is deliberately narrow: the admitted collector modules
    /// may not acquire another payload reference outside its lease callback.
    #[test]
    fn keeper_collector_reaches_guard_test() {
        for (name, source) in [
            ("incremental", include_str!("incremental.rs")),
            ("mark_sweep", include_str!("mark_sweep.rs")),
            ("memory_inventory", include_str!("memory_inventory.rs")),
            ("old_sweep", include_str!("old_sweep.rs")),
            ("generational", include_str!("generational.rs")),
            ("census", include_str!("census.rs")),
            (
                "collection",
                include_str!("mutator_registration/collection.rs"),
            ),
            (
                "weak sweep policy",
                include_str!("mutator_registration/weak_policy.rs"),
            ),
            // Tier-H's mutation preparation remains excluded from the proof-2
            // extent; its serial snapshot/retirement protocol is unchanged.
            (
                "concurrent mirrors",
                include_str!("concurrent.rs")
                    .split("pub(crate) fn prepare_concurrent_hash_write(")
                    .next()
                    .unwrap(),
            ),
        ] {
            let code = source
                .lines()
                .map(|line| line.split("//").next().unwrap())
                .flat_map(str::chars)
                .filter(|ch| !ch.is_whitespace())
                .collect::<String>();
            assert!(
                !code.contains(").table"),
                "unsealed collector table reference in {name}"
            );
            assert!(
                !code.contains("object_bytes_from_header("),
                "unsealed collector table accounting in {name}"
            );
        }
    }
}
