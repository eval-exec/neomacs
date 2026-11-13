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

    pub fn register_buffer_value(
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

    pub fn window_value(&self, id: u64) -> Option<TaggedValue> {
        self.window_registry.get(&id).copied()
    }

    pub fn register_window_value(&mut self, id: u64, value: TaggedValue) {
        self.window_registry.insert(id, value);
    }

    pub fn frame_value(&self, id: u64) -> Option<TaggedValue> {
        self.frame_registry.get(&id).copied()
    }

    pub fn register_frame_value(&mut self, id: u64, value: TaggedValue) {
        self.frame_registry.insert(id, value);
    }

    pub fn timer_value(&self, id: u64) -> Option<TaggedValue> {
        self.timer_registry.get(&id).copied()
    }

    pub fn register_timer_value(&mut self, id: u64, value: TaggedValue) {
        self.timer_registry.insert(id, value);
    }

    pub fn process_value(&self, id: crate::heap_types::ProcessId) -> Option<TaggedValue> {
        self.process_registry.get(&id).copied()
    }

    pub fn register_process_value(&mut self, id: crate::heap_types::ProcessId, value: TaggedValue) {
        self.process_registry.insert(id, value);
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

    pub fn dirty_owner_count(&self) -> usize {
        self.dirty_owners.len()
    }

    pub fn is_dirty_owner(&self, owner: TaggedValue) -> bool {
        self.dirty_owner_bits.contains(&owner.bits())
    }

    pub fn take_dirty_owners(&mut self) -> Vec<TaggedValue> {
        self.dirty_owner_bits.clear();
        std::mem::take(&mut self.dirty_owners)
    }

    pub fn clear_dirty_owners(&mut self) {
        self.dirty_owners.clear();
        self.dirty_owner_bits.clear();
    }

    pub fn dirty_write_count(&self) -> usize {
        self.dirty_writes.len()
    }

    pub fn dirty_writes(&self) -> &[HeapWriteRecord] {
        &self.dirty_writes
    }

    pub fn take_dirty_writes(&mut self) -> Vec<HeapWriteRecord> {
        std::mem::take(&mut self.dirty_writes)
    }

    pub fn clear_dirty_writes(&mut self) {
        self.dirty_writes.clear();
    }
}
