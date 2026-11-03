//! Read-only allocation-domain metadata for one Rows producer, never a GC root.
use super::*;

/// Counts refer to the existing append-only-until-sweep class directories.
/// This record is attempt-local. No epoch, TLS slot or edit-time write is added.
#[derive(Debug)]
pub(crate) struct RowsOwnedDomain {
    start_count: [usize; CHUNK_CLASS_COUNT],
}

impl TaggedHeap {
    pub(crate) fn rows_observation_idle(&self) -> bool {
        if self.concurrent_mark_running
            || self.mark_in_progress
            || self.sweep_in_progress
            || self.generational.major_in_progress
            || self.first_cycle_concurrent
            || !self.gray_queue.is_empty()
            || !self.sweep_noncons_pending.is_null()
            || !self.generational.old_sweep_pending.is_null()
            || self.pending_obarray_scan.is_some()
            || self.concurrent_obarray_start_slots.is_some()
            || self.staged_mapped_cons_scan.is_some()
            || self.staged_mapped_veclikes.is_some()
            || self.concurrent_hash_snapshot().is_some()
            || self.gc_exited.is_some()
            || !self.retired_vector_buffers.is_empty()
            || self.gc_locks_poisoned()
            || self.facade_mark_is_excluded()
            || self._satb_layout_reserve.has_outstanding_young_loans()
        {
            return false;
        }
        self.satb_handoff.is_empty().unwrap_or(false)
            && self
                .mutator_satb_states()
                .all(|owner| owner.satb.is_empty())
    }

    /// Caller checks GEN1, owner and comprehensive idle state before this read.
    /// First scope refuses chunk-map-off and every residual Box dependency.
    pub(crate) fn capture_rows_owned_domain(&self) -> Option<RowsOwnedDomain> {
        self.chunk_map.as_ref()?;
        let mut start_count = [0; CHUNK_CLASS_COUNT];
        start_count[ChunkClass::Cons as usize] = self.cons_blocks.len();
        start_count[ChunkClass::String as usize] = self.string_arena.pages.len();
        start_count[ChunkClass::Vector as usize] = self.vector_arena.pages.len();
        start_count[ChunkClass::Record as usize] = self.record_arena.pages.len();
        Some(RowsOwnedDomain { start_count })
    }

    /// NUMERIC ONLY. Owner/idle/driver/count checks must precede this lookup.
    /// A live foreign allocation at first read cannot overlap any start-owned
    /// block/page. A new current allocation after foreign destruction cannot
    /// acquire a start-owned address without releasing our old storage first.
    /// GEN1 lifecycle checks exclude that release before this numeric test.
    /// Production GEN1 is construction-only: cfg(test) policy toggles do not
    /// satisfy this contract. A GEN1 -> GEN0 -> panic -> GEN1 transition
    /// would invalidate this proof, and must never become a production API.
    /// This proves storage origin, not validity of arbitrary caller-made bits.
    pub(crate) fn rows_dependency_in_start_domain(
        &self,
        domain: &RowsOwnedDomain,
        bits: usize,
    ) -> bool {
        use super::super::value::{TAG_CONS, TAG_MASK, TAG_STRING, TAG_VECLIKE};
        let address = bits & !TAG_MASK;
        if address == 0 {
            return false;
        }
        let Some(map) = self.chunk_map.as_ref() else {
            return false;
        };
        let entry = map.get(address);
        let class = entry.class();
        if entry.index() >= domain.start_count[class as usize] {
            return false;
        }
        let offset = address & (OBJECT_PAGE_ALIGN - 1);
        match class {
            ChunkClass::Cons => {
                (bits & TAG_MASK) == TAG_CONS
                    && ConsBlock::ptr_is_cell_aligned(address as *const ConsCell)
            }
            ChunkClass::String => {
                (bits & TAG_MASK) == TAG_STRING
                    && offset.is_multiple_of(StringObj::SLOT_BYTES)
                    && offset / StringObj::SLOT_BYTES < ObjectPage::<StringObj>::SLOTS
            }
            ChunkClass::Vector => {
                (bits & TAG_MASK) == TAG_VECLIKE
                    && offset.is_multiple_of(VectorObj::SLOT_BYTES)
                    && offset / VectorObj::SLOT_BYTES < ObjectPage::<VectorObj>::SLOTS
            }
            ChunkClass::Record => {
                (bits & TAG_MASK) == TAG_VECLIKE
                    && offset.is_multiple_of(RecordObj::SLOT_BYTES)
                    && offset / RecordObj::SLOT_BYTES < ObjectPage::<RecordObj>::SLOTS
            }
            // None includes residual Boxes, foreign objects and native/static
            // objects. Every other page class is explicitly unsupported.
            ChunkClass::None
            | ChunkClass::Float
            | ChunkClass::ByteCode
            | ChunkClass::Lambda
            | ChunkClass::Macro
            | ChunkClass::SymbolWithPos
            | ChunkClass::Marker
            | ChunkClass::Bignum => false,
        }
    }
}

#[cfg(test)]
#[path = "tests/rows_owned_domain_test.rs"]
mod tests;
