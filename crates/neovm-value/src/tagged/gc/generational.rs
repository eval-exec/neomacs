//! Collector state and the per-cycle remembered-set protocol (P3.1 G2).
//! Mutator-owned logs and caches are reached through MutatorGcState.

use super::*;
use smallvec::SmallVec;

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
        let ptr = owner.gc_cons_ptr();
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
        // collector-owned and remains rooted throughout the cycle. Owners in
        // cons blocks and arena pages are traced through their dirty cards;
        // Box and mapped owners keep the per-owner path.
        for i in 0..self.generational.r_seed.len() {
            let owner = self.generational.r_seed[i];
            if !self.owner_is_card_scanned(owner) {
                self.push_value_children_to_gray(owner, "generational-remembered");
            }
        }
        self.trace_dirty_card_owners_world_stopped();
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
        // Cons stores mark cards without logging their owner; every dirty
        // chunk's slot was logged when its summary went dirty, so cleaning
        // the logged slots cleans every card. Only conses have cards; slot
        // owners leave through their logged headers below.
        self.scan_dirty_card_slots_world_stopped();
        let slots = self
            .card_slots_mut()
            .map(CardSlots::take_dirty)
            .unwrap_or_default();
        if let Some(cards) = self.card_table() {
            for slot in slots {
                cards.clear_slot(slot);
            }
        }
        let mut owners = std::mem::take(&mut self.generational.r_seed);
        for mutator in self.mutators_mut() {
            owners.append(&mut mutator.remset);
            mutator.remembered_cache.fill(0);
        }
        for owner in owners {
            if self.owner_is_mapped(owner) {
                continue;
            }
            // Owned conses are remembered by their cards, never logged.
            debug_assert!(!owner.is_cons(), "an owned cons in the remembered set");
            if let Some(addr) = Self::value_heap_addr(owner) {
                // All appenders are stopped, and no header has been freed.
                unsafe { &*(addr as *const GcHeader) }
                    .remembered
                    .store(RememberedState::Unlogged as u8, Ordering::Relaxed);
            }
        }
    }

    #[cfg(debug_assertions)]
    pub(super) fn debug_assert_remembered_membership(&self, owner: TaggedValue) {
        debug_assert!(
            self.remembered_membership_is_valid(owner),
            "remembered owner absent from R, R_seed and mapped_remembered: {owner:?}"
        );
    }

    /// Exercise the same invariant in tests even when debug assertions are off.
    #[cfg(test)]
    pub(super) fn assert_remembered_membership_for_test(&self, owner: TaggedValue) {
        assert!(
            self.remembered_membership_is_valid(owner),
            "remembered owner absent from R, R_seed and mapped_remembered: {owner:?}"
        );
    }

    #[cfg(any(test, debug_assertions))]
    fn remembered_membership_is_valid(&self, owner: TaggedValue) -> bool {
        if owner.is_cons() || self.owner_is_mapped(owner) {
            return true;
        }
        if let Some(addr) = Self::value_heap_addr(owner) {
            let header = unsafe { &*(addr as *const GcHeader) };
            !header.is_remembered()
                || self.mapped_remembered.contains(&owner.bits())
                || self
                    .generational
                    .r_seed
                    .iter()
                    .any(|value| value.bits() == owner.bits())
                || self
                    .mutators()
                    .any(|m| m.remset.iter().any(|value| value.bits() == owner.bits()))
        } else {
            true
        }
    }
}

/// Minor admission belongs to the serialized, stopped heap owner. Residual
/// major marking must finish before a new cycle may reset any collector state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum MinorCollectionAdmissionError {
    #[error("the heap has unfinished collector work")]
    CollectorBusy,
    #[error("minor collection requires the generational collector")]
    GenerationalDisabled,
}

static_assertions::assert_impl_all!(MinorCollectionAdmissionError: Send, Sync, std::fmt::Debug);

/// Parallel capture failures remain separate from the legacy admission enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(super) enum ParallelMinorAdmissionError {
    #[error(transparent)]
    Admission(#[from] MinorCollectionAdmissionError),
    #[error(transparent)]
    Capture(#[from] super::young_registry::YoungRegistryError),
}

impl TaggedHeap {
    /// Future global stop-all-mutators handshake belongs before this entry:
    /// join the collector, park every mutator and wait for all barrier appends.
    /// Minors and an active major mark are mutually exclusive, including the
    /// residual major after a worker joins or an unpublished capture drops.
    #[cold]
    #[inline(never)]
    pub(crate) fn begin_minor_collection(&mut self) -> Result<(), MinorCollectionAdmissionError> {
        // Admission precedes EVERY mutation, even for a GEN0 heap. Joining a
        // worker alone does not finish its major's residual mark or sweep.
        self.check_minor_admission()?;
        debug_assert!(
            !self.concurrent_mark_running
                && !self.mark_in_progress
                && !self.generational.major_in_progress,
            "minor stop-all excludes concurrent and unfinished major marking"
        );
        self.close_alloc_regions();
        // SAFETY: the existing collector caller retains stopped ownership;
        // no callbacks, allocation safepoints or mutator resumes occur here.
        unsafe { scan_contract::SingleMutatorWorld::from_heap(self) }
            .with_stopped_heap(|heap, proof| {
                // SAFETY: admission proved collector quiescence before any
                // mutation. Every mutator is stopped and its regions are closed;
                // minor marking/promotion stays in this same stopped extent.
                let mut logs = super::young_registry::StoppedYoungLogs::new(
                    &mut heap._satb_layout_reserve,
                    proof,
                )
                .expect("minor admission returned every young-only loan");
                let mut stopped = unsafe { heap.cons_allocator.stopped_world() };
                if heap.generational.verify {
                    stopped
                        .verify_young_at_begin(&heap.cons_blocks, &mut logs)
                        .expect("minor verifier uses this heap canonical registry");
                }
                // Freeze young logs before mark clearing; promotion hands this set to sweep.
                let _ = stopped
                    .capture_minor(&mut logs)
                    .expect("minor capture uses this heap canonical registry");
            })
            .expect("minor admission returned every young-only loan");
        self.begin_captured_minor_marking();
        Ok(())
    }

    #[inline(always)]
    pub(super) fn check_minor_admission(&mut self) -> Result<(), MinorCollectionAdmissionError> {
        facade_mark::CollectorQuiescence::check(self)
            .map_err(|_| MinorCollectionAdmissionError::CollectorBusy)?;
        if !self.generational.enabled {
            return Err(MinorCollectionAdmissionError::GenerationalDisabled);
        }
        Ok(())
    }

    /// The real parallel job already captured all published young payloads.
    /// Never enter the serial registry again: it cannot resolve peer IDs.
    #[cold]
    pub(super) fn begin_parallel_minor_collection(
        &mut self,
        capture: super::cons_allocator::CapturedParallelYoung,
        _proof: &scan_contract::ParallelStoppedMutatorsProof<'_>,
    ) -> Result<(), ParallelMinorAdmissionError> {
        self.check_minor_admission()?;
        capture.check_heap(self.heap_identity())?;
        if self.generational.verify {
            // SAFETY: the matching real parallel proof remains borrowed by
            // this job; all regions are closed, and capture already drained
            // EVERY peer payload into allocator-owned mark work. This bridge
            // only verifies the now-canonical work, never captures peer logs.
            unsafe { scan_contract::SingleMutatorWorld::from_heap(self) }.with_stopped_heap(
                |heap, proof| {
                    let logs = super::young_registry::StoppedYoungLogs::new(
                        &mut heap._satb_layout_reserve,
                        proof,
                    )?;
                    // SAFETY: the same complete stopped extent and unique
                    // heap loan exclude all allocators throughout this check.
                    let allocator = unsafe { heap.cons_allocator.stopped_world() };
                    allocator.verify_young_at_begin(&heap.cons_blocks, &logs)
                },
            )??;
        }
        self.begin_captured_minor_marking();
        Ok(())
    }

    #[inline(always)]
    fn begin_captured_minor_marking(&mut self) {
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
        // Refuse before moving promotion buffers or changing any old bits.
        // SAFETY: the caller retains the existing stopped-owner contract.
        unsafe { scan_contract::SingleMutatorWorld::from_heap(self) }
            .ensure_young_logs_stopped()
            .expect("promotion requires every young-only loan returned");
        if self.is_minor_collection() && self.generational.verify {
            // Every mutator is stopped and the weak/finalizer fixpoints are
            // complete. Check before promotion could hide a missing mark and
            // before any sweep can reclaim the missed young child.
            // An unmarked young child first: that is the missed barrier
            // itself. A marked one behind a clean card was reached another
            // way, but the next minor would miss it.
            self.verify_dump_partition();
            self.verify_old_cons_edge_cards_world_stopped();
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
        // The first partition's promotion walk owns non-cons classification,
        // including survivors of a late-loaded image.
        if !partition_first {
            for ptr in headers {
                let header = unsafe { &mut *ptr };
                if !header.tenured {
                    debug_assert!(header.is_marked_at(self.mark_parity));
                    header.promote_to_old_generation();
                    let bytes = self.collector_object_bytes(ptr);
                    self.generational.old_bytes = self.generational.old_bytes.saturating_add(bytes);
                    newly_promoted_header_bytes = newly_promoted_header_bytes.saturating_add(bytes);
                }
            }
        }
        let major = self.generational.major_in_progress;
        let mut promoted = 0;
        // SAFETY: the existing collector caller retains stopped ownership;
        // no callbacks, allocation safepoints or mutator resumes occur here.
        unsafe { scan_contract::SingleMutatorWorld::from_heap(self) }
            .with_stopped_heap(|heap, proof| {
                let old_count;
                // SAFETY: all mutators remain parked, regions closed, and any
                // major worker joined through promotion and flag/work handoff.
                let mut logs = super::young_registry::StoppedYoungLogs::new(
                    &mut heap._satb_layout_reserve,
                    proof,
                )
                .expect("promotion requires every young-only loan returned");
                let mut stopped = unsafe { heap.cons_allocator.stopped_world() };
                if major {
                    let mut total = 0;
                    for block in &heap.cons_blocks {
                        promoted += block.promote_major_world_stopped();
                        let block_old = block.count_old();
                        stopped.record_major_old_count(ConsBlockBase::of(block), block_old);
                        total += block_old;
                    }
                    old_count = total;
                } else {
                    // The cloned frozen bases cost O(young); unlike count_old,
                    // this does no mark/old-bitmap work in untouched old blocks.
                    let work = stopped.capture_work_snapshot();
                    for base in work {
                        let index = heap.cons_block_index_by_base[&base.addr()];
                        let newly_old = heap.cons_blocks[index].promote_marked_world_stopped();
                        stopped.record_minor_promoted(base, newly_old);
                        promoted += newly_old;
                    }
                    old_count = heap.generational.old_cons_count + promoted;
                }
                heap.generational.old_cons_count = old_count;
                stopped
                    .finish_promotion(
                        heap.generational.cycle,
                        &heap.cons_blocks,
                        old_count,
                        &mut logs,
                    )
                    .expect("promotion uses this heap canonical registry");
                if heap.generational.verify {
                    stopped
                        .verify_after_promotion(&heap.cons_blocks, &mut logs)
                        .expect("promotion verifier uses this heap canonical registry");
                }
            })
            .expect("promotion requires every young-only loan returned");
        if self.generational.verify {
            self.verify_age_mirror_world_stopped();
        }
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

    /// Whether VALUE, an owned cons, reads old in the age mirror that the
    /// write barrier tests.
    #[cfg(test)]
    pub(crate) fn cons_age_is_old_for_test(&self, value: TaggedValue) -> bool {
        let (trailer, index) = self.old_cons_trailer(value).expect("owned cons");
        trailer.age_is_old(index)
    }

    /// Whether OWNER's GEN1 card is dirty (false without a card table).
    #[cfg(test)]
    pub(crate) fn owner_card_is_dirty_for_test(&self, owner: TaggedValue) -> bool {
        Self::owner_card_addr(owner).is_some_and(|addr| self.owner_card_is_dirty(addr))
    }

    #[cfg(test)]
    pub(crate) fn remembered_log_len_for_test(&self) -> usize {
        self.mutators().map(|m| m.remset.len()).sum()
    }
}

impl TaggedHeap {
    /// The address whose card covers OWNER: a cons cell or an object header.
    /// Mapped owners stay on the window path and the persistent mapped set.
    #[inline]
    pub(super) fn owner_card_addr(owner: TaggedValue) -> Option<usize> {
        if owner.is_cons() {
            Some(owner.gc_cons_ptr() as usize)
        } else {
            Self::value_heap_addr(owner)
        }
    }

    /// The GEN1 card table, carried with the heap's other optional cold
    /// state so that it never widens the inline heap layout.
    #[inline]
    pub(super) fn card_table(&self) -> Option<&CardTable> {
        self.census
            .as_deref()
            .and_then(|carrier| carrier.cards.as_ref())
    }

    /// Mark the card of OWNER_ADDR. Mutator side, GEN1 only: two Relaxed
    /// byte stores into the table `install_card_table` built.
    #[inline]
    pub(super) fn mark_owner_card(&self, owner_addr: usize) {
        self.card_table()
            .expect("a GEN1 heap installs its card table")
            .mark(owner_addr);
    }

    /// Re-register the surviving cons blocks under their card chunk slots
    /// after a release. GEN1 only.
    pub(super) fn rebuild_card_slot_blocks(&mut self) {
        if !self.generational.enabled {
            return;
        }
        let Some(census) = self.census.as_deref_mut() else {
            return;
        };
        census
            .card_slots
            .rebuild(self.cons_blocks.iter().map(ConsBlock::base_addr));
    }

    /// Find the dirty chunk slots by scanning the summaries. With every
    /// mutator stopped.
    pub(super) fn scan_dirty_card_slots_world_stopped(&mut self) {
        let Some(census) = self.census.as_deref_mut() else {
            return;
        };
        if let Some(cards) = census.cards.as_ref() {
            census.card_slots.scan(cards);
        }
    }

    /// The collector's card chunk slot index (GEN1 heaps).
    #[inline]
    pub(super) fn card_slots(&self) -> Option<&CardSlots> {
        self.census.as_deref().map(|carrier| &carrier.card_slots)
    }

    #[inline]
    pub(super) fn card_slots_mut(&mut self) -> Option<&mut CardSlots> {
        self.census
            .as_deref_mut()
            .map(|carrier| &mut carrier.card_slots)
    }

    /// Turn generations on or off. A GEN1 heap builds its card table here,
    /// before any leaf can compile a card mark against it.
    #[cfg(test)]
    pub(crate) fn set_generational_enabled(&mut self, enabled: bool) {
        self.generational.enabled = enabled;
        if enabled {
            self.install_card_table();
        }
    }

    /// Build the GEN1 card table, carried with the heap's other optional
    /// cold state, and publish its fixed address where compiled cons stores
    /// read it. Idempotent; the table lives as long as the heap.
    pub(super) fn install_card_table(&mut self) {
        let base = self
            .census
            .get_or_insert_with(|| Box::new(GenCensus::disabled()))
            .cards
            .get_or_insert_with(CardTable::new)
            .base_addr();
        let bytes = base.to_ne_bytes();
        self.card_table_base_0.set(bytes[0]);
        self.card_table_base_1.set(bytes[1]);
        self.card_table_base_2.set(bytes[2]);
        self.card_table_base_3.set(bytes[3]);
        self.card_table_base_4.set(bytes[4]);
        self.card_table_base_5.set(bytes[5]);
        self.card_table_base_6.set(bytes[6]);
        self.card_table_base_7.set(bytes[7]);
    }

    #[inline]
    pub(super) fn owner_card_is_dirty(&self, owner_addr: usize) -> bool {
        self.card_table()
            .is_some_and(|cards| cards.is_dirty(owner_addr))
    }

    /// Whether a minor reaches logged OWNER's children through its dirty
    /// card: an old cons of an owned block. Vector-like, string and Box
    /// owners stay on the remembered set, which names exactly the logged
    /// owners: one of them can own thousands of out-of-line children, and
    /// a card would drag in every neighbour that shares it.
    #[inline]
    pub(super) fn owner_is_card_scanned(&self, owner: TaggedValue) -> bool {
        if !owner.is_cons() || self.owner_is_mapped(owner) {
            return false;
        }
        debug_assert!(self.old_cons_trailer(owner).is_some());
        true
    }

    /// Trace the old cells that start in a dirty card, straight onto the
    /// gray stack: for every dirty chunk slot with a dirty card, each cons
    /// block of that slot (more than one only 4 GiB apart), 32 cells per
    /// card, two children each. The work follows the dirty chunks, not the
    /// heap; blocks are re-borrowed per cell, so nothing is collected first.
    /// With every mutator stopped; an aliased card only adds owners.
    #[cold]
    #[inline(never)]
    fn trace_dirty_card_owners_world_stopped(&mut self) {
        if self.card_table().is_none() {
            return;
        }
        self.scan_dirty_card_slots_world_stopped();
        let slots = self.card_slots().map_or(0, |slots| slots.dirty().len());
        for slot_index in 0..slots {
            let Some((chunk, blocks)) = self.dirty_slot_blocks(slot_index) else {
                continue;
            };
            for block in blocks {
                self.trace_dirty_cards_of_block(block, chunk);
            }
        }
    }

    /// The cards of the SLOT_INDEXth dirty slot when one is dirty, and the
    /// indices of the owned cons blocks that use the slot.
    fn dirty_slot_blocks(&self, slot_index: usize) -> Option<(ChunkCards, SmallVec<[usize; 1]>)> {
        let slots = self.card_slots()?;
        let slot = slots.dirty()[slot_index];
        let chunk = self.card_table()?.dirty_cards(slot)?;
        let blocks = slots
            .blocks(slot)
            .iter()
            .filter_map(|base| self.cons_block_index_by_base.get(base).copied())
            .collect();
        Some((chunk, blocks))
    }

    #[inline]
    fn trace_dirty_cards_of_block(&mut self, block: usize, chunk: ChunkCards) {
        for offset in chunk.dirty_offsets() {
            let (first, mut old) = self.cons_blocks[block].old_cells_in_card(offset);
            while old != 0 {
                let index = first + old.trailing_zeros() as usize;
                old &= old - 1;
                // SAFETY: an old cell of an owned block is a live cons.
                let children = unsafe {
                    let cell = self.cons_blocks[block].cells_ptr().add(index);
                    [(*cell).load_car(), (*cell).load_cdr()]
                };
                for child in children {
                    // An old cell is black in a minor: skip it unpushed.
                    if !self.cons_blocks[block].holds_old_cell(child) {
                        self.mark_or_push_child(child, "generational-card");
                    }
                }
            }
        }
    }

    /// Every edge from an old owned cons to a young owned object has a dirty
    /// card: the minor reaches such an edge only through the card, and only
    /// a store after the last promotion can create one. Runs in GEN1 verify
    /// mode with every mutator stopped, before promotion and the card reset.
    #[cold]
    #[inline(never)]
    pub(super) fn verify_old_cons_edge_cards_world_stopped(&self) {
        for block in &self.cons_blocks {
            for word in 0..cons_mark_words(block.next_index as usize) {
                let mut old = block.trailer().old_word(word);
                while old != 0 {
                    let index = word * CONS_MARK_BITS_PER_WORD + old.trailing_zeros() as usize;
                    old &= old - 1;
                    // SAFETY: an old cell of an owned block is a live cons.
                    let cell = unsafe { block.cells_ptr().add(index) };
                    let children = unsafe { [(*cell).load_car(), (*cell).load_cdr()] };
                    for child in children {
                        assert!(
                            !self.value_is_young_owned(child)
                                || self.owner_card_is_dirty(cell as usize),
                            "GC verify: old cons {cell:p} has a young child {child:?} and a clean card"
                        );
                    }
                }
            }
        }
    }

    /// Every cons block's age bytes equal its old bits, the mirror compiled
    /// stores test. Runs in GEN1 verify mode right after every minor and
    /// major promotion, the only writers of either copy.
    #[cold]
    #[inline(never)]
    fn verify_age_mirror_world_stopped(&self) {
        for block in &self.cons_blocks {
            assert!(
                block.trailer().ages_match_old_bits(),
                "GC verify: cons block {:#x} age bytes differ from its old bits",
                block.base_addr()
            );
        }
    }

    /// Whether VALUE is an owned heap object of the young generation.
    fn value_is_young_owned(&self, value: TaggedValue) -> bool {
        if let Some((trailer, index)) = self.old_cons_trailer(value) {
            return !trailer.is_old(index);
        }
        if value.is_cons() || self.owner_is_mapped(value) {
            return false;
        }
        Self::value_heap_addr(value).is_some_and(|addr| {
            // SAFETY: a non-cons heap value points at a GcHeader-prefixed
            // object; ownership is checked before the header decides.
            self.owns_heap_value_object(value, addr)
                && !unsafe { &*(addr as *const GcHeader) }.tenured
        })
    }
}

#[cfg(test)]
#[path = "tests/minor_admission_test.rs"]
mod minor_admission_tests;
