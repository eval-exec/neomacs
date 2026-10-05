//! Optional collector cold state, carried in the existing census pointer slot.
//! No U35 field widens the inline heap or mutator allocation/barrier state.

use super::*;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

static NEXT_MUTATOR_ID: AtomicUsize = AtomicUsize::new(1);

thread_local! {
    // Created only by an enabled U35 accessor. A process-unique registration
    // token survives heap moves and cannot alias a recycled address/thread.
    static CONCURRENT_HASH_MUTATOR_ID: std::cell::Cell<Option<NonZeroUsize>> = const {
        std::cell::Cell::new(None)
    };
}

fn current_mutator_id() -> NonZeroUsize {
    CONCURRENT_HASH_MUTATOR_ID.with(|slot| {
        if let Some(id) = slot.get() {
            return id;
        }
        let raw = NEXT_MUTATOR_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                next.checked_add(1)
            })
            .expect("concurrent hash mutator registration IDs exhausted");
        let id = NonZeroUsize::new(raw).expect("mutator registration ID is nonzero");
        slot.set(Some(id));
        id
    })
}

/// U35 logs belong to a registered mutator, independently of the hot inline
/// MutatorGcState. All access is protected by this entry's mutex.
pub(super) struct ConcurrentHashMutatorState {
    pub(super) retired_hash_buffers: Vec<Vec<Option<crate::emacs_core::value::HashTableEntry>>>,
    pub(super) written_hash_owners: Vec<TaggedValue>,
}

impl ConcurrentHashMutatorState {
    pub(super) fn new() -> Self {
        Self {
            retired_hash_buffers: Vec::new(),
            written_hash_owners: Vec::new(),
        }
    }
}

pub(super) struct ConcurrentClaimsState {
    pub(super) leaf_claimed: Arc<AtomicUsize>,
    pub(super) last_leaf_claimed: usize,
    pub(super) hash_snapshot: Option<Arc<concurrent_hash::HashTableScanSnapshot>>,
    pub(super) hash_claimed: Arc<AtomicUsize>,
    pub(super) last_hash_claimed: usize,
    pub(super) scan_policy: concurrent_hash::HashTableScanPolicy,
    // Never hold this map lock while locking an entry/table descriptor.
    mutators: Mutex<FxHashMap<NonZeroUsize, Arc<Mutex<ConcurrentHashMutatorState>>>>,
}

impl ConcurrentClaimsState {
    fn new() -> Self {
        Self {
            leaf_claimed: Arc::new(AtomicUsize::new(0)),
            last_leaf_claimed: 0,
            hash_snapshot: None,
            hash_claimed: Arc::new(AtomicUsize::new(0)),
            last_hash_claimed: 0,
            scan_policy: knobs::concurrent_hash_scan_policy(),
            mutators: Mutex::new(FxHashMap::default()),
        }
    }

    pub(super) fn current_mutator(&self) -> Arc<Mutex<ConcurrentHashMutatorState>> {
        let id = current_mutator_id();
        let mut registry = self.mutators.lock().unwrap();
        registry
            .entry(id)
            .or_insert_with(|| Arc::new(Mutex::new(ConcurrentHashMutatorState::new())))
            .clone()
    }

    /// Called with all mutators stopped for capture, join and release.
    /// Thread exit must not discard cycle-retained logs/buffers. Entries stay
    /// heap-owned through its lifetime; a future unregister can prune them
    /// only after stopped-world termination has drained their obligations.
    pub(super) fn mutators_world_stopped(&self) -> Vec<Arc<Mutex<ConcurrentHashMutatorState>>> {
        self.mutators.lock().unwrap().values().cloned().collect()
    }

    pub(super) fn locks_poisoned(&self) -> bool {
        self.mutators.is_poisoned()
            || self
                .mutators_world_stopped()
                .iter()
                .any(|entry| entry.is_poisoned())
    }
}

/// Keeps the old optional-box pointer/niche carrier in TaggedHeap. Census
/// take/restore affects only `census`, so active U35 state never disappears.
pub(super) struct ColdGcState {
    pub(super) census: Option<Box<GenCensus>>,
    pub(super) concurrent: Option<ConcurrentClaimsState>,
}

impl ColdGcState {
    pub(super) fn from_knobs(claims: bool) -> Option<Box<Self>> {
        let census = GenCensus::from_knob();
        if !claims && census.is_none() {
            return None;
        }
        Some(Box::new(Self {
            census,
            concurrent: claims.then(ConcurrentClaimsState::new),
        }))
    }
}

impl TaggedHeap {
    #[inline]
    pub(super) fn census_state(&self) -> Option<&GenCensus> {
        self.census
            .as_deref()
            .and_then(|cold| cold.census.as_deref())
    }

    pub(super) fn census_state_mut(&mut self) -> Option<&mut GenCensus> {
        self.census
            .as_deref_mut()
            .and_then(|cold| cold.census.as_deref_mut())
    }

    #[inline]
    pub(super) fn concurrent_claims_state(&self) -> Option<&ConcurrentClaimsState> {
        self.census
            .as_deref()
            .and_then(|cold| cold.concurrent.as_ref())
    }

    pub(super) fn concurrent_claims_state_mut(&mut self) -> Option<&mut ConcurrentClaimsState> {
        self.census
            .as_deref_mut()
            .and_then(|cold| cold.concurrent.as_mut())
    }

    #[inline]
    pub(super) fn concurrent_claims(&self) -> bool {
        self.concurrent_claims_state().is_some()
    }

    pub(super) fn concurrent_hash_snapshot(
        &self,
    ) -> Option<&Arc<concurrent_hash::HashTableScanSnapshot>> {
        self.concurrent_claims_state()
            .and_then(|state| state.hash_snapshot.as_ref())
    }

    pub(super) fn set_concurrent_hash_snapshot(
        &mut self,
        snapshot: Option<Arc<concurrent_hash::HashTableScanSnapshot>>,
    ) {
        if let Some(state) = self.concurrent_claims_state_mut() {
            state.hash_snapshot = snapshot;
        } else {
            assert!(
                snapshot.is_none(),
                "Tier-H snapshot installed with claims disabled"
            );
        }
    }

    pub(super) fn concurrent_leaf_claimed(&self) -> Option<&Arc<AtomicUsize>> {
        self.concurrent_claims_state()
            .map(|state| &state.leaf_claimed)
    }

    pub(super) fn concurrent_hash_claimed(&self) -> Option<&Arc<AtomicUsize>> {
        self.concurrent_claims_state()
            .map(|state| &state.hash_claimed)
    }

    pub(super) fn last_concurrent_leaf_claimed(&self) -> usize {
        self.concurrent_claims_state()
            .map_or(0, |state| state.last_leaf_claimed)
    }

    pub(super) fn last_concurrent_hash_claimed(&self) -> usize {
        self.concurrent_claims_state()
            .map_or(0, |state| state.last_hash_claimed)
    }
}

// The normal x86-64 shipping type has no cfg(test) mapped_veclike_traces word.
// Main 2e4c571469's compiled DWARF supplies this baseline. These assertions
// reject silent hot-layout drift during ordinary compilation, independently
// of the runtime knob. The complete staged DWARF table remains the final gate.
#[cfg(all(target_arch = "x86_64", target_pointer_width = "64", not(test)))]
const _: () = {
    use std::mem::{align_of, offset_of, size_of};
    assert!(size_of::<TaggedHeap>() == 4528);
    assert!(align_of::<TaggedHeap>() == 8);
    assert!(offset_of!(TaggedHeap, mutator_gc) == 24);
    assert!(offset_of!(TaggedHeap, region_book) == 840);
    assert!(offset_of!(TaggedHeap, generational) == 928);
    assert!(offset_of!(TaggedHeap, cons_blocks) == 1048);
    assert!(offset_of!(TaggedHeap, float_arena) == 1264);
    assert!(offset_of!(TaggedHeap, string_arena) == 1368);
    assert!(offset_of!(TaggedHeap, vector_arena) == 1472);
    assert!(offset_of!(TaggedHeap, non_cons_object_addrs) == 3048);
    assert!(offset_of!(TaggedHeap, satb_shared) == 3432);
    assert!(offset_of!(TaggedHeap, jit) == 3608);
    assert!(jit_state::HEAP_JIT_CONS_CUR == 3608);
    assert!(jit_state::HEAP_JIT_CONS_LIM == 3616);
    assert!(jit_state::HEAP_JIT_FLOAT_CUR == 3624);
    assert!(jit_state::HEAP_JIT_FLOAT_LIM == 3632);
    assert!(jit_state::HEAP_JIT_BARRIER_LO == 3640);
    assert!(jit_state::HEAP_JIT_BARRIER_LEN == 3648);
    assert!(offset_of!(TaggedHeap, region_stats) == 3656);
    assert!(offset_of!(TaggedHeap, chunk_map) == 3752);
    assert!(offset_of!(TaggedHeap, all_objects) == 3768);
    assert!(offset_of!(TaggedHeap, gc_threshold) == 3784);
    assert!(offset_of!(TaggedHeap, live_bytes) == 3792);
    assert!(offset_of!(TaggedHeap, cons_free_list) == 3840);
    assert!(offset_of!(TaggedHeap, cons_live_count) == 3848);
    assert!(offset_of!(TaggedHeap, census) == 4504);
    assert!(offset_of!(TaggedHeap, mark_parity) == 4520);
    assert!(offset_of!(TaggedHeap, concurrent_mark_running) == 4522);
    assert!(offset_of!(TaggedHeap, sweep_in_progress) == 4523);
};

#[cfg(test)]
mod tests {
    use super::*;

    fn heap(claims: bool, census: knobs::CensusMode) -> TaggedHeap {
        knobs::set_concurrent_claims_for_test(Some(claims));
        knobs::set_census_mode_for_test(Some(census));
        let heap = TaggedHeap::new();
        knobs::set_concurrent_claims_for_test(None);
        knobs::set_census_mode_for_test(None);
        heap
    }

    #[test]
    fn u35_cold_carrier_is_absent_off_and_separates_census_from_claims() {
        let off = heap(false, knobs::CensusMode::Off);
        assert!(off.census.is_none());
        assert!(!off.concurrent_claims());
        assert!(off.concurrent_hash_mutators().next().is_none());
        assert!(off.concurrent_leaf_claimed().is_none());
        let census = heap(false, knobs::CensusMode::Survivors);
        assert!(census.census_state().is_some());
        assert!(!census.concurrent_claims());
        assert!(census.concurrent_hash_mutators().next().is_none());
        let claims = heap(true, knobs::CensusMode::Off);
        assert!(claims.census_state().is_none());
        assert!(claims.concurrent_claims());
        assert!(claims.concurrent_hash_mutators().next().is_none());
    }

    #[test]
    #[cfg(all(target_arch = "x86_64", target_pointer_width = "64"))]
    fn u35_hot_mutator_offsets_and_cold_carrier_match_main() {
        use std::mem::{align_of, offset_of, size_of};
        // Main 2e4c571469's 14-field inline state. This catches any future
        // cold log accidentally being reintroduced into allocation/barriers.
        assert_eq!(size_of::<MutatorGcState>(), 800);
        assert_eq!(align_of::<MutatorGcState>(), 8);
        assert_eq!(
            [
                offset_of!(MutatorGcState, black_cons_region_start),
                offset_of!(MutatorGcState, black_float_region_start),
                offset_of!(MutatorGcState, remset),
                offset_of!(MutatorGcState, black_born),
                offset_of!(MutatorGcState, black_born_regions),
                offset_of!(MutatorGcState, major_symbol_preimages),
                offset_of!(MutatorGcState, major_cons_writes),
                offset_of!(MutatorGcState, r_mapped_seen),
                offset_of!(MutatorGcState, allocated_count),
                offset_of!(MutatorGcState, memory_use_counts),
                offset_of!(MutatorGcState, bytes_since_gc),
                offset_of!(MutatorGcState, bytes_banked_at_resets),
                offset_of!(MutatorGcState, remembered_cache),
                offset_of!(MutatorGcState, pacing),
            ],
            [
                0, 16, 32, 56, 80, 104, 128, 152, 184, 192, 248, 256, 264, 776
            ],
        );
        assert_eq!(
            size_of::<Option<Box<ColdGcState>>>(),
            size_of::<Option<Box<GenCensus>>>()
        );
        assert_eq!(
            align_of::<Option<Box<ColdGcState>>>(),
            align_of::<Option<Box<GenCensus>>>()
        );
        assert_eq!(size_of::<Option<Box<ColdGcState>>>(), size_of::<usize>());
        assert_eq!(size_of::<JitHeapState>(), 48);
        let mut off = heap(false, knobs::CensusMode::Off);
        assert!(!off.concurrent_claims());
        let census = heap(false, knobs::CensusMode::Survivors);
        assert!(!census.concurrent_claims());
        let mut on = heap(true, knobs::CensusMode::Off);
        let owner = on.alloc_hash_table(crate::emacs_core::value::LispHashTable::new(
            crate::emacs_core::value::HashTableTest::Eq,
        ));
        assert!(
            on.non_cons_object_addrs
                .contains(&(owner.as_veclike_ptr().unwrap() as usize))
        );
        assert!(on.concurrent_hash_mutators().next().is_none());
        // Installing cold state cannot relocate any already-open region or
        // alter accounting layout; the OFF allocation uses the original path.
        let cons = off.alloc_cons(TaggedValue::fixnum(5), TaggedValue::NIL);
        assert!(cons.is_cons());
        assert!(off.census.is_none());
    }

    #[test]
    fn u35_census_inner_take_restore_preserves_snapshot_counters_and_mutator_logs() {
        let mut heap = heap(true, knobs::CensusMode::Survivors);
        let snapshot = Arc::new(concurrent_hash::HashTableScanSnapshot::new());
        heap.set_concurrent_hash_snapshot(Some(snapshot.clone()));
        let entry = heap.current_concurrent_hash_mutator();
        entry
            .lock()
            .unwrap()
            .written_hash_owners
            .push(TaggedValue::fixnum(3));
        heap.concurrent_leaf_claimed()
            .unwrap()
            .store(7, Ordering::Relaxed);
        let cold_address = &**heap.census.as_ref().unwrap() as *const ColdGcState;
        heap.census_at_termination(CensusCycleKind::StopTheWorld, 0);
        assert_eq!(
            &**heap.census.as_ref().unwrap() as *const ColdGcState,
            cold_address
        );
        assert!(heap.last_census_for_test().is_some());
        assert!(Arc::ptr_eq(
            heap.concurrent_hash_snapshot().unwrap(),
            &snapshot
        ));
        assert_eq!(
            heap.concurrent_leaf_claimed()
                .unwrap()
                .load(Ordering::Relaxed),
            7
        );
        assert!(Arc::ptr_eq(&heap.current_concurrent_hash_mutator(), &entry));
        assert_eq!(
            entry.lock().unwrap().written_hash_owners,
            [TaggedValue::fixnum(3)]
        );
    }

    #[test]
    fn u35_cold_registry_has_stable_distinct_mutators_and_retains_all_thread_exit_logs() {
        let state = Arc::new(ConcurrentClaimsState::new());
        let workers: Vec<_> = (0..3)
            .map(|i| {
                let state = state.clone();
                std::thread::spawn(move || {
                    let entry = state.current_mutator();
                    assert!(Arc::ptr_eq(&entry, &state.current_mutator()));
                    let mut log = entry.lock().unwrap();
                    log.written_hash_owners.push(TaggedValue::fixnum(i as i64));
                    log.retired_hash_buffers.push(vec![Some(
                        crate::emacs_core::value::HashTableEntry {
                            key: TaggedValue::fixnum(i as i64),
                            value: TaggedValue::fixnum(99),
                        },
                    )]);
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap();
        }
        let entries = state.mutators_world_stopped();
        assert_eq!(entries.len(), 3);
        let mut owners = Vec::new();
        for entry in &entries {
            let mut log = entry.lock().unwrap();
            assert_eq!(log.retired_hash_buffers.len(), 1);
            owners.append(&mut log.written_hash_owners);
        }
        owners.sort_by_key(|owner| owner.bits());
        assert_eq!(owners, (0..3).map(TaggedValue::fixnum).collect::<Vec<_>>());
        // A vector of copied handles holds no registry lock; the coordinator
        // can enumerate again while those same entry locks are held.
        let locked = entries[0].lock().unwrap();
        assert_eq!(state.mutators_world_stopped().len(), 3);
        drop(locked);
        for entry in state.mutators_world_stopped() {
            let mut log = entry.lock().unwrap();
            log.retired_hash_buffers.clear();
            assert!(log.written_hash_owners.is_empty());
        }
    }
}
