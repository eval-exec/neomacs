//! GC state written only by the mutator.
//!
//! The current runtime has one mutator. Keeping its allocation accounting and
//! barrier state in one inline value gives collector consumers the same
//! accessor surface a future mutator registry will expose, without adding a
//! pointer indirection to allocation or a new thread-local cache.

use super::*;

pub(super) struct MutatorGcState {
    /// Charged objects, including the unused tail of an open P0.8 region.
    pub(super) allocated_count: usize,
    pub(super) memory_use_counts: [u64; MEMORY_USE_COUNT_LEN],
    pub(super) bytes_since_gc: usize,
    /// Bytes charged before the most recent accounting reset.
    pub(super) bytes_banked_at_resets: u64,
    /// Ordinary old owners recorded since the previous collection began.
    pub(super) remset: Vec<TaggedValue>,
    /// Mapped owners whose children must be scanned in the next cycle.
    pub(super) r_mapped_seen: FxHashSet<usize>,
    /// Non-cons objects born black during a concurrent major (C2.6).
    pub(super) black_born: Vec<*mut GcHeader>,
    pub(super) black_born_regions: Vec<birth_logs::BlackBornRegion>,
    pub(super) black_cons_region_start: Option<usize>,
    pub(super) black_float_region_start: Option<usize>,
    /// Symbol side-table preimages and inserted symbols belong to this mutator.
    pub(super) major_symbol_preimages: Vec<SymId>,
    /// Cons owners written during a concurrent major. Their current children
    /// are traced once per owner after the stopped-world join; old insertions
    /// are not necessarily allocate-black births. No free precedes this drain.
    pub(super) major_cons_writes: Vec<TaggedValue>,
    /// Direct-mapped repeat-owner reject, cleared at every cycle begin.
    pub(super) remembered_cache: [usize; BARRIER_CACHE_SLOTS],
    /// Generation pacing obligations owned by this mutator, summed at poll.
    pub(super) pacing: pacing::GenerationPacingCounters,
}

/// Unused region capacity, derived from the cursors shared with compiled code.
/// Deriving this at a read keeps exact accounting out of the allocation path.
#[derive(Clone, Copy)]
pub(super) struct AllocationRegionDeltas {
    pub(super) conses: usize,
    pub(super) floats: usize,
}

impl AllocationRegionDeltas {
    #[inline]
    fn objects(self) -> usize {
        self.conses + self.floats
    }

    #[inline]
    fn bytes(self) -> usize {
        self.conses * size_of::<ConsCell>() + self.floats * size_of::<FloatObj>()
    }
}

impl MutatorGcState {
    pub(super) fn new() -> Self {
        Self {
            allocated_count: 0,
            memory_use_counts: [0; MEMORY_USE_COUNT_LEN],
            bytes_since_gc: 0,
            bytes_banked_at_resets: 0,
            remset: Vec::new(),
            r_mapped_seen: FxHashSet::default(),
            black_born: Vec::new(),
            black_born_regions: Vec::new(),
            black_cons_region_start: None,
            black_float_region_start: None,
            major_symbol_preimages: Vec::new(),
            major_cons_writes: Vec::new(),
            remembered_cache: [0; BARRIER_CACHE_SLOTS],
            pacing: pacing::GenerationPacingCounters::default(),
        }
    }

    #[inline]
    pub(super) fn exact_allocated_count(&self, delta: AllocationRegionDeltas) -> usize {
        self.allocated_count - delta.objects()
    }

    #[inline]
    pub(super) fn exact_bytes_since_gc(&self, delta: AllocationRegionDeltas) -> usize {
        self.bytes_since_gc - delta.bytes()
    }

    #[inline]
    pub(super) fn exact_memory_use_counts(
        &self,
        delta: AllocationRegionDeltas,
    ) -> [u64; MEMORY_USE_COUNT_LEN] {
        let mut counts = self.memory_use_counts;
        let conses = MemoryUseCountSlot::ConsCells.index();
        counts[conses] = counts[conses].wrapping_sub(delta.conses as u64);
        let floats = MemoryUseCountSlot::Floats.index();
        counts[floats] = counts[floats].wrapping_sub(delta.floats as u64);
        counts
    }
}

impl TaggedHeap {
    #[inline]
    pub(super) fn current_mutator_gc(&self) -> &MutatorGcState {
        &self.mutator_gc
    }

    #[inline]
    pub(super) fn current_mutator_gc_mut(&mut self) -> &mut MutatorGcState {
        &mut self.mutator_gc
    }

    #[inline]
    pub(super) fn mutators(&self) -> impl Iterator<Item = &MutatorGcState> {
        std::iter::once(self.current_mutator_gc())
    }

    #[inline]
    pub(super) fn mutators_mut(&mut self) -> impl Iterator<Item = &mut MutatorGcState> {
        std::iter::once(self.current_mutator_gc_mut())
    }

    #[inline]
    pub(super) fn allocation_region_deltas(&self) -> AllocationRegionDeltas {
        AllocationRegionDeltas {
            conses: self.open_cons_unused(),
            floats: self.open_float_unused(),
        }
    }
}
