//! Collector state and the per-cycle remembered-set protocol (P3.1 G2).
//! Mutator-owned logs and caches are reached through MutatorGcState.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GenerationCycle {
    Major,
    Minor,
}

pub(super) struct GenState {
    pub(super) enabled: bool,
    /// Constructor-frozen verification; inactive even if requested with GEN0.
    pub(super) verify: bool,
    pub(super) cycle: GenerationCycle,
    /// Full scope lasts until every deferred sweep cursor completes.
    pub(super) major_in_progress: bool,
    pub(super) old_sweep_pending: *mut GcHeader,
    /// Newly traced young headers, including weak/finalizer fixpoints.
    pub(super) promo: Vec<*mut GcHeader>,
    /// Collector-owned residual Box old list; links use GcHeader accessors.
    pub(super) old_objects: *mut GcHeader,
    pub(super) old_bytes: usize,
    /// Constructor-read limits and exact ordinary-old bytes after a major.
    pub(super) pacing_knobs: knobs::GenerationalPacingKnobs,
    pub(super) old_bytes_after_major: usize,
    pub(super) old_cons_count: usize,
    /// Working R: drained at the stop-all boundary, rooted until termination.
    pub(super) r_seed: Vec<TaggedValue>,
}

impl GenState {
    pub(super) fn new(enabled: bool) -> Self {
        Self {
            enabled,
            verify: enabled && std::env::var("NEOVM_GC_VERIFY_GENERATIONAL").as_deref() == Ok("1"),
            cycle: GenerationCycle::Major,
            major_in_progress: false,
            old_sweep_pending: std::ptr::null_mut(),
            promo: Vec::new(),
            old_objects: std::ptr::null_mut(),
            old_bytes: 0,
            pacing_knobs: knobs::generational_pacing_knobs(enabled),
            old_bytes_after_major: 0,
            old_cons_count: 0,
            r_seed: Vec::new(),
        }
    }
}

impl TaggedHeap {
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
        self.publish_persistent_remembered_world_stopped();
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

    /// Merge current-mutator persistent additions only under collector
    /// exclusion. A future stop-all handshake must precede this drain.
    #[cold]
    #[inline(never)]
    pub(super) fn publish_persistent_remembered_world_stopped(&mut self) {
        if !self.generational.enabled {
            return;
        }
        let mut additions = Vec::new();
        for mutator in self.mutators_mut() {
            additions.extend(mutator.r_mapped_seen.drain());
        }
        self.mapped_remembered.extend(additions);
    }

    /// Reset claims before a major traces old objects, and at termination
    /// after P-all. No owner is freed until this drain is complete. Facts
    /// appended during the later cooperative sweep belong to the next cycle.
    #[cold]
    #[inline(never)]
    pub(super) fn reset_generational_remembered_world_stopped(&mut self) {
        if !self.generational.enabled {
            return;
        }
        self.publish_persistent_remembered_world_stopped();
        let mut owners = std::mem::take(&mut self.generational.r_seed);
        for mutator in self.mutators_mut() {
            owners.append(&mut mutator.remset);
            mutator.remembered_cache.fill(0);
        }
        for owner in owners {
            if self.owner_is_mapped(owner) {
                continue;
            }
            if owner.is_cons() {
                if let Some((trailer, index)) = self.old_cons_trailer(owner)
                    && trailer.is_old(index)
                {
                    trailer.set_unlogged(index);
                }
            } else if let Some(addr) = Self::value_heap_addr(owner) {
                // All appenders are stopped, and no header has been freed.
                unsafe { &*(addr as *const GcHeader) }
                    .remembered
                    .store(RememberedState::Unlogged as u8, Ordering::Relaxed);
            }
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
                    || self
                        .generational
                        .r_seed
                        .iter()
                        .any(|value| value.bits() == owner.bits())
                    || self
                        .mutators()
                        .any(|m| m.remset.iter().any(|value| value.bits() == owner.bits())),
                "remembered owner absent from R, R_seed and mapped_remembered: {owner:?}"
            );
        }
    }
}

impl TaggedHeap {
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
        self.incremental_finish(bytes_before, neomacs_host_runtime::time::Instant::now());
    }

    #[inline]
    pub(super) fn is_minor_collection(&self) -> bool {
        self.generational.cycle == GenerationCycle::Minor
    }

    #[inline]
    pub(super) fn note_minor_survivor(&mut self, header: *mut GcHeader) {
        if self.generational.enabled && !unsafe { (*header).tenured } {
            self.generational.promo.push(header);
        }
    }

    #[inline]
    pub(super) fn is_generational_major_marking(&self) -> bool {
        self.generational.major_in_progress && self.mark_in_progress
    }

    /// P-all: traced headers plus every mutator's black births. The marker
    /// has joined; no mutator can publish a store until this finishes.
    #[cold]
    #[inline(never)]
    pub(super) fn promote_survivors_world_stopped(&mut self) {
        if !self.generational.enabled {
            return;
        }
        if self.is_minor_collection() && self.generational.verify {
            // Every mutator is stopped and the weak/finalizer fixpoints are
            // complete. Check before promotion could hide a missing mark and
            // before any sweep can reclaim the missed young child.
            self.verify_dump_partition();
        }
        let partition_first = self.partition_dump && !self.dump_blackened;
        let mut headers = std::mem::take(&mut self.generational.promo);
        for mutator in self.mutators() {
            headers.extend(mutator.black_born.iter().copied());
            for region in &mutator.black_born_regions {
                headers.extend(region.float_headers());
            }
        }
        let mut newly_promoted_header_bytes = 0usize;
        // The first partition's existing permanent splice owns non-cons
        // promotion, including survivors of a late-loaded image.
        if !partition_first {
            for ptr in headers {
                let header = unsafe { &mut *ptr };
                if !header.tenured {
                    debug_assert!(header.is_marked_at(self.mark_parity));
                    header.tenured = true;
                    let bytes = Self::object_bytes_from_header(ptr);
                    self.generational.old_bytes = self.generational.old_bytes.saturating_add(bytes);
                    newly_promoted_header_bytes = newly_promoted_header_bytes.saturating_add(bytes);
                }
            }
        }
        let major = self.generational.major_in_progress;
        let mut promoted = 0;
        let mut old_count = 0;
        for block in &self.cons_blocks {
            promoted += if major {
                block.promote_major_world_stopped()
            } else {
                block.promote_marked_world_stopped()
            };
            old_count += block.count_old();
        }
        self.generational.old_cons_count = old_count;
        let newly_promoted_cons_bytes = promoted.saturating_mul(size_of::<ConsCell>());
        self.generational.old_bytes = self
            .generational
            .old_bytes
            .saturating_add(newly_promoted_cons_bytes);
        self.record_generation_promoted_bytes(
            newly_promoted_header_bytes.saturating_add(newly_promoted_cons_bytes),
        );
        #[cfg(feature = "gc-memory-telemetry")]
        memory_telemetry::note_promotion(
            self,
            newly_promoted_header_bytes.saturating_add(newly_promoted_cons_bytes),
        );
        if !partition_first {
            self.clear_black_births_world_stopped();
        }
    }

    /// Clear only after ordinary P-all or the first permanent splice. GEN-4
    /// prevents a new collection until deferred first-partition promotion ends.
    #[cold]
    #[inline(never)]
    pub(super) fn clear_black_births_world_stopped(&mut self) {
        for mutator in self.mutators_mut() {
            debug_assert!(mutator.black_cons_region_start.is_none());
            debug_assert!(mutator.black_float_region_start.is_none());
            mutator.black_born.clear();
            mutator.black_born_regions.clear();
        }
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
