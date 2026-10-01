//! Collector state and the per-cycle remembered-set protocol (P3.1 G2).
//! Mutator-owned logs and caches are reached through MutatorGcState.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GenerationCycle {
    CurrentFull,
    Minor,
}

pub(super) struct GenState {
    pub(super) enabled: bool,
    pub(super) cycle: GenerationCycle,
    /// Headers traced by the stopped-world minor, including weak/finalizer fixpoints.
    pub(super) promo: Vec<*mut GcHeader>,
    /// Collector-owned residual Box old list; links use GcHeader accessors.
    pub(super) old_objects: *mut GcHeader,
    pub(super) old_bytes: usize,
    pub(super) old_cons_count: usize,
    /// Working R: drained at the stop-all boundary, rooted until termination.
    pub(super) r_seed: Vec<TaggedValue>,
}

impl GenState {
    pub(super) fn new(enabled: bool) -> Self {
        Self {
            enabled,
            cycle: GenerationCycle::CurrentFull,
            promo: Vec::new(),
            old_objects: std::ptr::null_mut(),
            old_bytes: 0,
            old_cons_count: 0,
            r_seed: Vec::new(),
        }
    }
}

impl TaggedHeap {
    #[cfg(test)]
    pub(crate) fn disable_generations_for_test(&mut self) {
        self.generational.enabled = false;
        self.publish_barrier_window();
    }

    #[inline]
    pub(crate) fn generational_enabled(&self) -> bool {
        self.generational.enabled
    }

    #[inline]
    pub(super) fn old_cons_trailer(
        &self,
        owner: TaggedValue,
    ) -> Option<(&ConsBlockTrailer, usize)> {
        if !owner.is_cons() || self.owner_is_mapped(owner) {
            return None;
        }
        let ptr = owner.xcons_ptr();
        if !ConsBlock::ptr_is_cell_aligned(ptr) {
            return None;
        }
        let base = ConsBlock::block_base_for_ptr(ptr);
        let block = match self.chunk_map.as_ref() {
            Some(map) => {
                let entry = map.get(ptr as usize);
                entry
                    .is(ChunkClass::Cons)
                    .then(|| &self.cons_blocks[entry.index()])
            }
            None => self
                .cons_block_index_by_base
                .get(&base)
                .map(|&i| &self.cons_blocks[i]),
        }?;
        Some((block.trailer(), ConsBlock::index_of_ptr(ptr)))
    }

    /// Called with every mutator stopped and every barrier append complete.
    /// Mapped dedup is local to each mutator; this drain deduplicates across
    /// all registered mutators. Today the iterator yields exactly one.
    #[cold]
    #[inline(never)]
    pub(super) fn seed_generational_remembered(&mut self) {
        if !self.generational.enabled {
            return;
        }
        debug_assert!(self.generational.r_seed.is_empty());
        let mut owners = Vec::new();
        let mut seen = FxHashSet::default();
        for mutator in self.mutators_mut() {
            mutator.remembered_cache.fill(0);
            for owner in mutator.remset.drain(..) {
                if seen.insert(owner.bits()) {
                    owners.push(owner);
                }
            }
        }
        self.generational.r_seed = owners;
        // No Lisp allocation or safepoint inside enumeration; R_seed is
        // collector-owned and remains rooted throughout the cycle.
        for i in 0..self.generational.r_seed.len() {
            self.push_value_children_to_gray(
                self.generational.r_seed[i],
                "generational-remembered",
            );
        }
    }

    /// Reset logging facts with all mutators stopped, after eager minor
    /// promotion. Current full cycles preserve ordinary-old facts until
    /// C2.6 provides their P-all promotion rule.
    #[cold]
    #[inline(never)]
    pub(super) fn reset_generational_remembered_world_stopped(&mut self) {
        if !self.generational.enabled {
            return;
        }
        let owners = std::mem::take(&mut self.generational.r_seed);
        if self.generational.cycle == GenerationCycle::CurrentFull {
            // C2.6 has not supplied P-all yet: current full cycles do not
            // promote their young survivors. Keep each old->young fact for
            // the next minor rather than dropping a still-needed repair.
            self.current_mutator_gc_mut().remset.extend(owners);
            return;
        }
        for owner in owners {
            if self.owner_is_mapped(owner) {
                continue;
            }
            if owner.is_cons() {
                if let Some((trailer, index)) = self.old_cons_trailer(owner) {
                    trailer.set_unlogged(index);
                }
            } else if let Some(addr) = Self::value_heap_addr(owner) {
                // No concurrent claims at this stop-all boundary.
                unsafe { &*(addr as *const GcHeader) }
                    .remembered
                    .store(RememberedState::Unlogged as u8, Ordering::Relaxed);
            }
        }
        for mutator in self.mutators_mut() {
            mutator.r_mapped_seen.clear();
            mutator.remembered_cache.fill(0);
        }
    }

    #[cfg(debug_assertions)]
    pub(super) fn debug_assert_remembered_membership(&self, owner: TaggedValue) {
        if owner.is_cons() || self.owner_is_mapped(owner) {
            return;
        }
        if let Some(addr) = Self::value_heap_addr(owner) {
            let header = unsafe { &*(addr as *const GcHeader) };
            debug_assert!(
                !header.is_remembered()
                    || self.mapped_remembered.contains(&owner.bits())
                    || self.generational.r_seed.contains(&owner)
                    || self.mutators().any(|m| m.remset.contains(&owner)),
                "remembered owner absent from R, R_seed and mapped_remembered: {owner:?}"
            );
        }
    }
}

impl TaggedHeap {
    #[inline]
    pub(crate) fn should_run_minor(&self) -> bool {
        self.generational.enabled && self.should_run_concurrent()
    }

    /// Future global stop-all-mutators handshake belongs before this entry:
    /// join the collector, park every mutator and wait for all barrier appends.
    #[cold]
    #[inline(never)]
    pub(crate) fn begin_minor_collection(&mut self) {
        assert!(self.generational.enabled);
        assert!(!self.concurrent_mark_running);
        self.generational.cycle = GenerationCycle::Minor;
        self.generational.promo.clear();
        self.incremental_mark_us = 0;
        self.pace_mark_start = None;
        self.pace_mark_start_bytes = self.bytes_since_gc();
        self.begin_collection_with(false);
        self.mark_in_progress = true;
    }

    /// Marking and all fixpoints execute on the stopped mutator, then eager
    /// promotion arms the ordinary cooperative sweep. No mutator runs between
    /// tracing a survivor and setting its generation bit.
    #[cold]
    #[inline(never)]
    pub(crate) fn complete_minor_collection(&mut self) {
        assert!(self.generational.cycle == GenerationCycle::Minor);
        self.close_alloc_regions();
        let bytes_before = self.live_bytes;
        self.incremental_drain_all();
        self.incremental_finish(bytes_before, std::time::Instant::now());
    }

    #[inline]
    pub(super) fn is_minor_collection(&self) -> bool {
        self.generational.cycle == GenerationCycle::Minor
    }

    #[inline]
    pub(super) fn note_minor_survivor(&mut self, header: *mut GcHeader) {
        if self.is_minor_collection() {
            self.generational.promo.push(header);
        }
    }

    /// With all mutators stopped and the GC thread joined, promote the complete
    /// traced set before dropping R. Deferred sweep only resets marks/moves Box
    /// nodes; it never decides generation membership.
    #[cold]
    #[inline(never)]
    pub(super) fn promote_minor_survivors_world_stopped(&mut self) {
        if !self.is_minor_collection() {
            return;
        }
        for ptr in self.generational.promo.drain(..) {
            let header = unsafe { &mut *ptr };
            debug_assert!(!header.tenured);
            header.tenured = true;
            self.generational.old_bytes = self
                .generational
                .old_bytes
                .saturating_add(Self::object_bytes_from_header(ptr));
        }
        let promoted: usize = self
            .cons_blocks
            .iter_mut()
            .map(|block| block.promote_marked_world_stopped())
            .sum();
        self.generational.old_cons_count += promoted;
        self.generational.old_bytes = self
            .generational
            .old_bytes
            .saturating_add(promoted * size_of::<ConsCell>());
    }

    #[inline]
    pub(super) fn cons_block_live_count(&self, block: &ConsBlock) -> usize {
        if self.generational.enabled {
            block.count_live_generational()
        } else {
            block.count_marked()
        }
    }

    #[inline]
    pub(super) fn old_noncons_bytes(&self) -> usize {
        self.generational
            .old_bytes
            .saturating_sub(self.generational.old_cons_count * size_of::<ConsCell>())
    }

    /// Add a promoted Box survivor to the old list under collector exclusion.
    #[inline]
    pub(super) unsafe fn link_old_object_world_stopped(&mut self, ptr: *mut GcHeader) {
        let header = unsafe { &mut *ptr };
        debug_assert!(header.tenured && !header.generation.permanent());
        header.marked.store(UNMARKED_AT_REST, Ordering::Relaxed);
        header.set_gc_link_world_stopped(self.generational.old_objects);
        self.generational.old_objects = ptr;
    }

    /// Today's full cycle skips non-cons tenured objects. Until C2.6 can trace
    /// and reclaim ordinary old objects, enumerate their children conservatively
    /// on full cycles. This also preserves the symbol-mark repair that skipping
    /// old owners would otherwise remove. No Lisp allocation occurs in this walk.
    #[cold]
    #[inline(never)]
    pub(super) fn seed_old_children_for_current_full(&mut self) {
        if !self.generational.enabled || self.is_minor_collection() {
            return;
        }
        let mut headers = Vec::new();
        let mut visit = |ptr: *mut GcHeader| {
            let header = unsafe { &*ptr };
            if header.tenured && !header.generation.permanent() {
                headers.push(ptr);
            }
        };
        self.float_arena.for_each_allocated_slot(&mut visit);
        self.string_arena.for_each_allocated_slot(&mut visit);
        self.vector_arena.for_each_allocated_slot(&mut visit);
        self.bytecode_arena.for_each_allocated_slot(&mut visit);
        self.lambda_arena.for_each_allocated_slot(&mut visit);
        self.macro_arena.for_each_allocated_slot(&mut visit);
        self.record_arena.for_each_allocated_slot(&mut visit);
        self.symbol_with_pos_arena
            .for_each_allocated_slot(&mut visit);
        self.marker_arena.for_each_allocated_slot(&mut visit);
        self.bignum_arena.for_each_allocated_slot(&mut visit);
        let mut ptr = self.generational.old_objects;
        while !ptr.is_null() {
            headers.push(ptr);
            ptr = unsafe { (*ptr).gc_link() };
        }
        for ptr in headers {
            let owner = unsafe {
                match (*ptr).kind {
                    HeapObjectKind::String => TaggedValue::from_string_ptr(ptr.cast()),
                    HeapObjectKind::Float => TaggedValue::from_float_ptr(ptr.cast()),
                    HeapObjectKind::VecLike => TaggedValue::from_veclike_ptr(ptr.cast()),
                }
            };
            self.push_value_children_to_gray(owner, "current-full-old-children");
        }
    }

    #[cfg(test)]
    pub(crate) fn value_is_old_for_test(&self, value: TaggedValue) -> bool {
        if let Some((trailer, index)) = self.old_cons_trailer(value) {
            trailer.is_old(index)
        } else {
            self.value_is_tenured(value)
        }
    }

    #[cfg(test)]
    pub(crate) fn remembered_log_len_for_test(&self) -> usize {
        self.mutators().map(|m| m.remset.len()).sum()
    }
}
