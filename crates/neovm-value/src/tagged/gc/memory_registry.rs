//! TaggedHeap memory accounting and object-registry surface.
//!
//! Moved out of `tagged/gc.rs` unchanged; a child module of `gc` so it keeps
//! the same view of `TaggedHeap` and the parent's private items
//! (`use super::*`). These are the public/pub(crate) memory-use counters,
//! allocation-byte counters, object-registry registrations and dirty-owner
//! accessors that pushed the root file over its layout ceiling.

use super::*;

impl TaggedHeap {
    #[inline]
    pub(crate) fn add_memory_use_count(&mut self, slot: MemoryUseCountSlot, delta: u64) {
        let index = slot.index();
        let counts = &mut self.current_mutator_gc_mut().memory_use_counts;
        counts[index] = counts[index].wrapping_add(delta);
    }

    /// Zero the three fields `charge_conses` writes onto the post-image shell.
    ///
    /// Between collections those facts live in the published image.
    /// `restore_legacy` returns `ImageError::DestinationOccupied` when
    /// `allocated_count`, the cons `memory_use_counts` slot, or
    /// `bytes_since_gc` is nonzero. This does not bank the bytes, and it
    /// leaves `cons_live_count` and the cons block in place.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn clear_inter_collection_cons_charge_for_test(&mut self) {
        let state = self.current_mutator_gc_mut();
        state.allocated_count = 0;
        state.memory_use_counts[MemoryUseCountSlot::ConsCells.index()] = 0;
        state.bytes_since_gc = 0;
    }

    /// The Lisp-visible allocation counts (`memory-use-counts`), exact: one
    /// per object handed out, the open allocation regions' unused cells
    /// excluded (they were charged when the region was granted).
    #[inline]
    #[doc(hidden)]
    pub fn memory_use_counts_snapshot(&self) -> [u64; MEMORY_USE_COUNT_LEN] {
        let delta = self.allocation_region_deltas();
        self.mutators()
            .map(|state| state.exact_memory_use_counts(delta))
            .fold([0; MEMORY_USE_COUNT_LEN], |mut total, counts| {
                for (sum, count) in total.iter_mut().zip(counts) {
                    *sum = sum.wrapping_add(count);
                }
                total
            })
    }

    /// Bytes allocated since the last collection, as CHARGED: an open
    /// allocation region counts whole. Never below the exact count, and a
    /// region is never granted past the threshold
    /// (`TaggedHeap::region_budget`), so the pacing gates that read this
    /// collect when they did before — at most one region early, never late.
    #[inline(always)]
    pub fn bytes_since_gc(&self) -> usize {
        self.mutators().map(|state| state.bytes_since_gc).sum()
    }

    /// Bytes allocated since the last collection, exact (GNU's `since_gc`):
    /// the open allocation regions' unused cells excluded. What
    /// `garbage-collect-maybe`'s FACTOR test and the memory profiler read.
    #[inline]
    pub fn bytes_since_gc_exact(&self) -> usize {
        let delta = self.allocation_region_deltas();
        self.mutators()
            .map(|state| state.exact_bytes_since_gc(delta))
            .sum()
    }

    /// The one place `bytes_since_gc` returns to zero.
    ///
    /// It banks what is being cleared, so that [`Self::total_allocated_bytes`]
    /// stays exact without the allocation path counting it a second time.
    /// Every collector site resets through here for that reason.
    #[doc(hidden)]
    pub fn reset_bytes_since_gc(&mut self) {
        // Close first, so what is banked is exactly what was handed out.
        self.close_alloc_regions();
        for state in self.mutators_mut() {
            state.bytes_banked_at_resets = state
                .bytes_banked_at_resets
                .saturating_add(state.bytes_since_gc as u64);
            state.bytes_since_gc = 0;
        }
    }

    pub fn live_bytes(&self) -> usize {
        self.live_bytes
    }

    pub fn buffer_value(&self, id: crate::value::buffer_vocab::BufferId) -> Option<TaggedValue> {
        self.buffer_registry.get(id.0 as usize).copied().flatten()
    }

    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn register_buffer_value(
        &mut self,
        id: crate::value::buffer_vocab::BufferId,
        value: TaggedValue,
    ) {
        let idx = id.0 as usize;
        if self.buffer_registry.len() <= idx {
            self.buffer_registry.resize(idx + 1, None);
        }
        self.buffer_registry[idx] = Some(value);
    }

    /// Arm killed-buffer fields as ephemerons for the next full collection.
    #[doc(hidden)]
    pub fn arm_killed_buffer_ephemerons(
        &mut self,
        killed: Vec<(crate::value::buffer_vocab::BufferId, Vec<TaggedValue>)>,
    ) {
        self.ephemeron_records.killed_buffer_ephemerons = killed.into_iter().collect();
    }

    /// Take the buffer ids reclaimed by the last full collection.
    #[doc(hidden)]
    pub fn take_reclaimed_buffer_ids(&mut self) -> Vec<crate::value::buffer_vocab::BufferId> {
        std::mem::take(&mut self.ephemeron_records.reclaimed_buffer_ids)
    }

    pub(crate) fn window_value(&self, id: u64) -> Option<TaggedValue> {
        self.window_registry.get(&id).copied()
    }

    pub(crate) fn register_window_value(&mut self, id: u64, value: TaggedValue) {
        self.window_registry.insert(id, value);
    }

    pub(crate) fn frame_value(&self, id: u64) -> Option<TaggedValue> {
        self.frame_registry.get(&id).copied()
    }

    pub(crate) fn register_frame_value(&mut self, id: u64, value: TaggedValue) {
        self.frame_registry.insert(id, value);
    }

    pub(crate) fn timer_value(&self, id: u64) -> Option<TaggedValue> {
        self.timer_registry.get(&id).copied()
    }

    pub(crate) fn register_timer_value(&mut self, id: u64, value: TaggedValue) {
        self.timer_registry.insert(id, value);
    }

    pub(crate) fn process_value(&self, id: crate::heap_types::ProcessId) -> Option<TaggedValue> {
        self.process_registry.get(&id).copied()
    }

    pub(crate) fn register_process_value(
        &mut self,
        id: crate::heap_types::ProcessId,
        value: TaggedValue,
    ) {
        self.process_registry.insert(id, value);
    }

    /// Arm retired process fields as ephemerons for the next full collection.
    #[doc(hidden)]
    pub fn arm_retired_process_ephemerons(
        &mut self,
        retired: Vec<(crate::heap_types::ProcessId, Vec<TaggedValue>)>,
    ) {
        self.ephemeron_records.retired_process_ephemerons = retired.into_iter().collect();
    }

    /// Take the process ids reclaimed by the last full collection.
    #[doc(hidden)]
    pub fn take_reclaimed_process_ids(&mut self) -> Vec<crate::heap_types::ProcessId> {
        std::mem::take(&mut self.ephemeron_records.reclaimed_process_ids)
    }

    /// True when `make_window` has registered a window object for `id`.
    ///
    /// The window registry is an unconditional root. A deleted frame that
    /// owns any such window stays a root too: GNU `mark_window` marks
    /// `w->frame`.
    #[doc(hidden)]
    pub fn window_object_registered(&self, id: u64) -> bool {
        self.window_registry.contains_key(&id)
    }

    /// Arm deleted-frame fields as ephemerons for the next full collection.
    #[doc(hidden)]
    pub fn arm_deleted_frame_ephemerons(&mut self, deleted: Vec<(u64, Vec<TaggedValue>)>) {
        self.ephemeron_records.deleted_frame_ephemerons = deleted.into_iter().collect();
    }

    /// Take the frame ids reclaimed by the last full collection.
    #[doc(hidden)]
    pub fn take_reclaimed_frame_ids(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.ephemeron_records.reclaimed_frame_ids)
    }

    /// Drop an uncommitted explicit-collection arm.
    ///
    /// A collection that panics before its restore commits must not leave
    /// killed buffers, retired processes, or deleted frames unrooted, and
    /// must not leave reclaimed ids for a later cycle to apply. Registry
    /// entries are left as they are. One function clears every kind so the
    /// three disarms cannot drift.
    #[doc(hidden)]
    pub fn abandon_explicit_ephemerons(&mut self) {
        self.ephemeron_records.retired_process_ephemerons.clear();
        self.ephemeron_records.reclaimed_process_ids.clear();
        self.ephemeron_records.killed_buffer_ephemerons.clear();
        self.ephemeron_records.reclaimed_buffer_ids.clear();
        self.ephemeron_records.deleted_frame_ephemerons.clear();
        self.ephemeron_records.reclaimed_frame_ids.clear();
    }

    /// True when no explicit-collection ephemeron map or reclaimed-id list
    /// is armed. A pure reader: it writes nothing.
    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn explicit_ephemeron_arm_is_clear(&self) -> bool {
        let records = &self.ephemeron_records;
        records.retired_process_ephemerons.is_empty()
            && records.reclaimed_process_ids.is_empty()
            && records.killed_buffer_ephemerons.is_empty()
            && records.reclaimed_buffer_ids.is_empty()
            && records.deleted_frame_ephemerons.is_empty()
            && records.reclaimed_frame_ids.is_empty()
    }

    /// Add one Context's slice to this heap's cycle bank.
    #[doc(hidden)]
    pub fn bank_cycle_elapsed(&mut self, slice: std::time::Duration) {
        self.cycle_time_bank.bank(slice);
    }

    /// Take the bank. The caller publishes it; the next cycle starts at zero.
    #[doc(hidden)]
    pub fn take_cycle_elapsed(&mut self) -> std::time::Duration {
        self.cycle_time_bank.take()
    }

    /// Time banked for the cycle in progress.
    #[doc(hidden)]
    pub fn cycle_elapsed(&self) -> std::time::Duration {
        self.cycle_time_bank.peek()
    }

    /// Wall microseconds of the context-root seed that just ran.
    ///
    /// One store per collection. The clock is the seed's existing total,
    /// not a per-root sample.
    #[doc(hidden)]
    pub fn note_context_seed_us(&mut self, us: u64) {
        self.cycle_time_bank.note_context_seed(us);
    }

    /// Context-root seed of the collection now marking. Eager trace lines
    /// read this. Zero until that seed runs.
    #[doc(hidden)]
    pub fn context_seed_us(&self) -> u64 {
        self.cycle_time_bank.context_seed_us()
    }

    /// Keep the context-root seed with the deferred sweep it belongs to.
    pub(super) fn snapshot_context_seed_for_sweep(&mut self) {
        self.cycle_time_bank.snapshot_context_seed();
    }

    /// Context-root seed frozen when the deferred sweep was armed.
    pub(super) fn sweep_context_seed_us(&self) -> u64 {
        self.cycle_time_bank.sweep_context_seed_us()
    }

    /// Register cons cells whose storage is owned by the loaded pdump image.
    ///
    /// # Safety
    /// `start..start+len` must remain mapped and writable for the lifetime of
    /// this heap.  The range must contain aligned `ConsCell` objects.
    #[doc(hidden)]
    pub unsafe fn register_mapped_cons_range(&mut self, start: *mut ConsCell, len: usize) {
        if len == 0 {
            return;
        }
        debug_assert_eq!(start as usize % std::mem::align_of::<ConsCell>(), 0);
        self.extend_dump_span(start as usize, len.saturating_mul(size_of::<ConsCell>()));
        self.mapped_cons_ranges
            .push(MappedConsRange::new(start, len));
        let state = self.current_mutator_gc_mut();
        state.allocated_count = state.allocated_count.saturating_add(len);
        self.live_bytes = self
            .live_bytes
            .saturating_add(len.saturating_mul(size_of::<ConsCell>()));
    }

    /// Register float objects whose storage is owned by the loaded pdump image.
    ///
    /// # Safety
    /// `start..start+len` must remain mapped and writable for the lifetime of
    /// this heap.  The range must contain aligned `FloatObj` objects.
    #[doc(hidden)]
    pub unsafe fn register_mapped_float_range(&mut self, start: *mut FloatObj, len: usize) {
        if len == 0 {
            return;
        }
        debug_assert_eq!(start as usize % std::mem::align_of::<FloatObj>(), 0);
        self.extend_dump_span(start as usize, len.saturating_mul(size_of::<FloatObj>()));
        self.mapped_float_ranges
            .push(MappedFloatRange::new(start, len));
        let state = self.current_mutator_gc_mut();
        state.allocated_count = state.allocated_count.saturating_add(len);
        self.live_bytes = self
            .live_bytes
            .saturating_add(len.saturating_mul(size_of::<FloatObj>()));
    }

    /// Register a vectorlike object whose storage is owned by the loaded pdump image.
    ///
    /// # Safety
    /// `header` must point at a complete, aligned vectorlike object that remains
    /// mapped and writable for the lifetime of this heap.
    /// Pre-size the mapped-object registries for a load about to register
    /// `veclikes` + `strings` objects (a 12K-entry FxHashMap grown by
    /// rehashing costs several M Ir across a pdump load).
    pub fn reserve_mapped_object_capacity(&mut self, veclikes: usize, strings: usize) {
        self.mapped_veclike_objects.reserve(veclikes);
        self.mapped_veclike_index_by_addr.reserve(veclikes);
        self.mapped_string_objects.reserve(strings);
        self.mapped_string_index_by_addr.reserve(strings);
    }

    #[doc(hidden)]
    pub unsafe fn register_mapped_veclike_object(
        &mut self,
        header: *mut VecLikeHeader,
        byte_len: usize,
    ) {
        if byte_len == 0 {
            return;
        }
        debug_assert_eq!(header as usize % std::mem::align_of::<VecLikeHeader>(), 0);
        self.extend_dump_span(header as usize, byte_len);
        let index = self.mapped_veclike_objects.len();
        let prev = self
            .mapped_veclike_index_by_addr
            .insert(header as usize, index);
        debug_assert!(prev.is_none(), "mapped vectorlike object registered twice");
        self.mapped_veclike_objects
            .push(MappedVecLikeObject::new(header, byte_len));
        let state = self.current_mutator_gc_mut();
        state.allocated_count = state.allocated_count.saturating_add(1);
        self.live_bytes = self.live_bytes.saturating_add(byte_len);
    }

    /// Register a string object whose storage is owned by the loaded pdump image.
    ///
    /// # Safety
    /// `ptr` must point at a complete, aligned string object that remains
    /// mapped and writable for the lifetime of this heap.
    #[doc(hidden)]
    pub unsafe fn register_mapped_string_object(&mut self, ptr: *mut StringObj, byte_len: usize) {
        if byte_len == 0 {
            return;
        }
        debug_assert_eq!(ptr as usize % std::mem::align_of::<StringObj>(), 0);
        self.extend_dump_span(ptr as usize, byte_len);
        let index = self.mapped_string_objects.len();
        let prev = self.mapped_string_index_by_addr.insert(ptr as usize, index);
        debug_assert!(prev.is_none(), "mapped string object registered twice");
        self.mapped_string_objects
            .push(MappedStringObject::new(ptr, byte_len));
        let state = self.current_mutator_gc_mut();
        state.allocated_count = state.allocated_count.saturating_add(1);
        self.live_bytes = self.live_bytes.saturating_add(byte_len);

        let string = unsafe { &(*ptr).data };
        if string.sbytes() == 0 {
            let value = unsafe { TaggedValue::from_string_ptr(ptr) };
            self.canonical_empty_strings
                .install_mapped(string.storage_kind(), value);
        }
    }

    /// Stamp VALUE, a heap object the pdump loader reconstructed from a
    /// dump-image row, as an image resident.
    ///
    /// The first partition cycle's promotion makes exactly these objects
    /// permanent; every other survivor is tenured only. Mapped image objects
    /// need no stamp (their residency is the mapped mark side tables), and
    /// conses have no header: both are ignored, as is anything this heap does
    /// not own.
    ///
    /// The loader calls this once per reconstructed image row before the
    /// image is published, so the byte is written before any mutator or
    /// collector can observe the object.
    #[doc(hidden)]
    pub fn note_image_object(&mut self, value: TaggedValue) -> ImageStamp {
        if value.is_cons() {
            return ImageStamp::NotStamped;
        }
        let Some(addr) = Self::value_heap_addr(value) else {
            return ImageStamp::NotStamped;
        };
        if self.owner_is_mapped(value) || !self.owns_heap_value_object(value, addr) {
            return ImageStamp::NotStamped;
        }
        // SAFETY: the ownership oracle just proved this is a live initialized
        // non-cons heap object, and the loader runs single-threaded before the
        // image is published, so no mark or mutator reads the byte.
        unsafe {
            let header = &mut *(addr as *mut GcHeader);
            header.flags = header.flags.with_image_resident();
        }
        ImageStamp::Stamped
    }

    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn is_dirty_owner(&self, owner: TaggedValue) -> bool {
        self.dirty_owner_bits.contains(&owner.bits())
    }

    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn clear_dirty_owners(&mut self) {
        self.dirty_owners.clear();
        self.dirty_owner_bits.clear();
    }

    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn dirty_writes(&self) -> &[HeapWriteRecord] {
        &self.dirty_writes
    }

    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn clear_dirty_writes(&mut self) {
        self.dirty_writes.clear();
    }
}

#[cfg(test)]
pub(crate) mod b13_test_hooks {
    use super::*;
    impl TaggedHeap {
        pub(crate) fn dirty_owner_count(&self) -> usize {
            self.dirty_owners.len()
        }

        pub(crate) fn dirty_write_count(&self) -> usize {
            self.dirty_writes.len()
        }
    }
}

/// Retired-process, killed-buffer, and deleted-frame records armed as
/// ephemerons for the next explicit full collection, keyed on the process,
/// buffer, or frame object, plus the ids that collection did not reach.
///
/// There is no durable set of reclaimed frame ids. Frame creation mints the
/// object, and a later missing `frame_registry` entry means reclaimed or
/// never created.
#[derive(Default)]
pub(crate) struct EphemeronRecords {
    /// Retired process fields armed for the next stop-the-world full cycle.
    pub(crate) retired_process_ephemerons:
        FxHashMap<crate::heap_types::ProcessId, Vec<TaggedValue>>,
    /// Process ids whose ephemeron keys were unreachable in the last full cycle.
    pub(crate) reclaimed_process_ids: Vec<crate::heap_types::ProcessId>,
    /// Killed-buffer fields armed for the next stop-the-world full cycle.
    pub(crate) killed_buffer_ephemerons:
        FxHashMap<crate::value::buffer_vocab::BufferId, Vec<TaggedValue>>,
    /// Buffer ids whose ephemeron keys were unreachable in the last full cycle.
    pub(crate) reclaimed_buffer_ids: Vec<crate::value::buffer_vocab::BufferId>,
    /// Deleted-frame fields armed for the next stop-the-world full cycle.
    pub(crate) deleted_frame_ephemerons: FxHashMap<u64, Vec<TaggedValue>>,
    /// Frame ids whose ephemeron keys were unreachable in the last full cycle.
    pub(crate) reclaimed_frame_ids: Vec<u64>,
}

/// `(garbage-collect)`'s per-kind live/free counts (#775).
///
/// GNU reports the `gcstat` totals its sweep leaves behind: USED is the live
/// count for that cycle and FREE the free-list count
/// (`src/alloc.c:310-317, 6038-6063`). Neo's arenas have no per-kind free
/// list, so FREE is the page capacity that currently holds no live object
/// (for vector slots, unused payload capacity). Mapped image objects are
/// added where they are countable. Closes open allocation regions first, so
/// every cell is either live or free (the `layout_stats` contract); the scan
/// is O(pages) and runs only for the `garbage-collect` row build.
#[doc(hidden)]
impl TaggedHeap {
    pub fn gc_object_counts(&mut self) -> GcObjectCounts {
        self.close_alloc_regions();
        let stats = self.layout_stats();
        let arena = |class: &str| {
            stats
                .arenas
                .iter()
                .find(|arena| arena.class == class)
                .cloned()
                .unwrap_or_default()
        };
        let string = arena("string");
        let vector = arena("vector");
        let float = arena("float");
        let capacity = |arena: &ArenaLayoutStats| arena.pages.saturating_mul(arena.slots_per_page);
        let word = std::mem::size_of::<TaggedValue>();
        let mapped_string_bytes: usize = self
            .mapped_string_objects
            .iter()
            .map(|object| object.byte_len)
            .sum();
        GcObjectCounts {
            conses_used: stats.cons.live_slots.saturating_add(stats.mapped.conses),
            conses_free: stats
                .cons
                .capacity_slots
                .saturating_sub(stats.cons.live_slots),
            symbols_used: crate::intern::symbol_span_total(),
            strings_used: string.allocated_slots.saturating_add(stats.mapped.strings),
            strings_free: capacity(&string).saturating_sub(string.allocated_slots),
            string_bytes: string
                .payload_logical_bytes
                .saturating_add(mapped_string_bytes),
            vectors_used: vector.allocated_slots.saturating_add(stats.mapped.veclikes),
            vector_slots_used: vector.payload_logical_bytes / word,
            vector_slots_free: vector
                .payload_capacity_bytes
                .saturating_sub(vector.payload_logical_bytes)
                / word,
            floats_used: float.allocated_slots.saturating_add(stats.mapped.floats),
            floats_free: capacity(&float).saturating_sub(float.allocated_slots),
            intervals_used: usize::try_from(crate::value::text_props::intervals_live())
                .unwrap_or(usize::MAX),
            buffers_used: self
                .buffer_registry
                .iter()
                .filter(|entry| entry.is_some())
                .count(),
        }
    }
}

#[cfg(test)]
#[path = "tests/ephemeron_cross_kind_test.rs"]
mod ephemeron_cross_kind_tests;
