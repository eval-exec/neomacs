//! Last-sweep return rows, separate from physical heap inventory.
//!
//! GNU copies gcstat after hooks finish, but the totals describe the sweep
//! before those hooks. An explicit observer requests one cold capture at cycle
//! publication; ordinary allocation and automatic GC gain no inventory walk.

use super::*;

#[derive(Default)]
pub(super) struct GarbageCollectSnapshot {
    requested: bool,
    counts: GcObjectCounts,
}

fn arena_free_slots<T: PagedObject>(arena: &ObjectArena<T>) -> usize {
    arena
        .pages
        .len()
        .saturating_mul(ObjectPage::<T>::SLOTS)
        .saturating_sub(arena.pages.iter().map(|page| page.allocated).sum())
}

fn visit_arena<T: PagedObject>(arena: &ObjectArena<T>, visit: &mut impl FnMut(*const GcHeader)) {
    // Include retired pages: header provenance, not page retirement, decides
    // whether an allocated object belongs to the logical image.
    for page in &arena.pages {
        for word_index in 0..ObjectPage::<T>::ALLOC_WORDS {
            let mut bits = page.alloc_bits[word_index];
            while bits != 0 {
                let bit = bits.trailing_zeros() as usize;
                bits &= bits - 1;
                visit(
                    page.slot_ptr(word_index * usize::BITS as usize + bit)
                        .cast(),
                );
            }
        }
    }
}

#[cfg(test)]
impl TaggedHeap {
    /// #625 structural inventory: physical bytes of every live owned string --
    /// arena slots at the class stride plus each owned payload capacity
    /// (including the NUL), excluding image-resident strings. This is the
    /// string representation's footprint, deliberately separate from GNU's
    /// logical live-object census (32-byte header + STRING_BYTES).
    #[doc(hidden)]
    pub(crate) fn string_physical_bytes_for_test(&self) -> (usize, usize, usize) {
        let mut strings = 0usize;
        let mut slot_bytes = 0usize;
        let mut payload_capacity_bytes = 0usize;
        let mut visit = |header: *const GcHeader| {
            // SAFETY: arena allocation bits establish a live initialized
            // header; the test heap is quiescent and exclusively borrowed.
            unsafe {
                if (*header).flags.image_resident() || (*header).kind != HeapObjectKind::String {
                    return;
                }
                let string = &*header.cast::<StringObj>();
                strings += 1;
                slot_bytes += <StringObj as PagedObject>::SLOT_BYTES;
                payload_capacity_bytes += string.data.storage_capacity();
            }
        };
        visit_arena(&self.string_arena, &mut visit);
        (strings, slot_bytes, payload_capacity_bytes)
    }
}

impl TaggedHeap {
    /// Request rows for the next completed collection, before its finalizers
    /// and post-gc-hook allocate anything. This is independent of byte pacing.
    #[doc(hidden)]
    pub fn request_gc_object_counts(&mut self) {
        self._satb_layout_reserve.gc_rows_mut().requested = true;
    }

    /// Publish all rows at one stopped, post-sweep instant. The evaluator
    /// supplies its last-sweep symbol census, outside the tagged object heap.
    #[doc(hidden)]
    pub fn publish_gc_object_counts_if_requested(&mut self, symbols_used: usize) {
        if !self._satb_layout_reserve.gc_rows().requested {
            return;
        }
        self.close_alloc_regions();
        let counts = self.runtime_gc_object_counts(symbols_used);
        let snapshot = self._satb_layout_reserve.gc_rows_mut();
        snapshot.counts = counts;
        snapshot.requested = false;
    }

    /// Pure read of the captured rows. In particular, return-list and hook
    /// allocations must not change USED or FREE while these rows are built.
    #[doc(hidden)]
    pub fn gc_object_counts(&self) -> GcObjectCounts {
        self._satb_layout_reserve.gc_rows().counts
    }

    fn runtime_gc_object_counts(&self, symbols_used: usize) -> GcObjectCounts {
        let bumped: usize = self
            .cons_blocks
            .iter()
            .map(|block| block.next_index as usize)
            .sum();
        let free: usize = self
            .cons_allocator
            .free_counts()
            .into_iter()
            .map(|(_, count)| count)
            .sum();
        let conses_used = bumped.saturating_sub(free);
        let intervals_slot = MemoryUseCountSlot::Intervals.index();
        // Preserve the allocation-minus-drop definition and its existing low
        // read with other owners' undrained counts; capture it with all rows.
        let intervals_allocated = self.memory_use_counts_snapshot()[intervals_slot]
            .wrapping_add(crate::value::thread_local_allocation_counts()[intervals_slot]);
        let mut counts = GcObjectCounts {
            conses_used,
            conses_free: self
                .cons_blocks
                .len()
                .saturating_mul(CONS_BLOCK_SIZE)
                .saturating_sub(conses_used),
            symbols_used,
            strings_free: arena_free_slots(&self.string_arena),
            floats_free: arena_free_slots(&self.float_arena),
            intervals_used: usize::try_from(crate::value::text_props::intervals_live_from(
                intervals_allocated,
            ))
            .unwrap_or(usize::MAX),
            buffers_used: self
                .buffer_registry
                .iter()
                .filter(|entry| entry.is_some())
                .count(),
            ..GcObjectCounts::default()
        };
        let mut visit = |header| unsafe { self.note_runtime_gc_object(header, &mut counts) };
        visit_arena(&self.string_arena, &mut visit);
        visit_arena(&self.float_arena, &mut visit);
        visit_arena(&self.vector_arena, &mut visit);
        visit_arena(&self.bytecode_arena, &mut visit);
        visit_arena(&self.lambda_arena, &mut visit);
        visit_arena(&self.macro_arena, &mut visit);
        visit_arena(&self.record_arena, &mut visit);
        visit_arena(&self.symbol_with_pos_arena, &mut visit);
        visit_arena(&self.marker_arena, &mut visit);
        visit_arena(&self.bignum_arena, &mut visit);
        for &address in &self.non_cons_object_addrs {
            visit(address as *const GcHeader);
        }
        counts
    }

    /// # Safety
    /// `header` must point to a live, initialized GC object header.
    unsafe fn note_runtime_gc_object(&self, header: *const GcHeader, counts: &mut GcObjectCounts) {
        // SAFETY: arena allocation bits or exact Box ownership established a
        // live initialized header. Capture runs at stopped post-sweep
        // publication or before activation publishes this heap; no mutator
        // or callback advances the heap during the cold capture.
        unsafe {
            if (*header).flags.image_resident() {
                return;
            }
            match (*header).kind {
                HeapObjectKind::String => {
                    counts.strings_used += 1;
                    // GNU sweep_strings adds STRING_BYTES, excluding the NUL
                    // and allocator alignment (alloc.c:1862-1863).
                    counts.string_bytes += (*header.cast::<StringObj>()).data.byte_len();
                }
                HeapObjectKind::Float => counts.floats_used += 1,
                HeapObjectKind::VecLike => {
                    counts.vectors_used += 1;
                    let vector = header.cast::<VecLikeHeader>();
                    let word = size_of::<TaggedValue>();
                    let slots = match (*vector).type_tag {
                        VecLikeType::Vector => {
                            let data = &(*vector.cast::<VectorObj>()).data;
                            counts.vector_slots_free +=
                                data.owned_capacity().saturating_sub(data.len());
                            // GNU's ordinary vector has a one-word header in
                            // addition to its Lisp cells (alloc.c:3323-3326).
                            data.len().saturating_add(1)
                        }
                        // Hash backing is accounted separately, never as
                        // vector slots. Keep this row's footprint inline.
                        VecLikeType::HashTable => size_of::<HashTableObj>().div_ceil(word),
                        _ => self.collector_object_bytes(header).div_ceil(word),
                    };
                    counts.vector_slots_used += slots;
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "tests/owned_gc_rows_test.rs"]
mod tests;
