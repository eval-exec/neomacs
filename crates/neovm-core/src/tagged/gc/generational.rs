//! Collector state and the per-cycle remembered-set protocol (P3.1 G2).
//! Mutator-owned logs and caches are reached through MutatorGcState.

use super::*;

pub(super) struct GenState {
    pub(super) enabled: bool,
    /// Working R: drained at the stop-all boundary, rooted until termination.
    pub(super) r_seed: Vec<TaggedValue>,
}

impl GenState {
    pub(super) fn new(enabled: bool) -> Self {
        Self {
            enabled,
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

    /// Reset logging facts with all mutators stopped. Young survivors are
    /// promoted by C2.5 before this reset; C2.4 only has permanent/mapped
    /// owners, whose persistent remembered set still seeds today's cycles.
    pub(super) fn reset_generational_remembered_world_stopped(&mut self) {
        if !self.generational.enabled {
            return;
        }
        let owners = std::mem::take(&mut self.generational.r_seed);
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
