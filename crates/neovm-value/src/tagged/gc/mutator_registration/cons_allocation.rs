//! Rooted single-cons allocation for admitted consumers; no Lisp or heap TLS.
use super::super::cons_storage::CatalogSource;
use super::super::*;
use super::*;
use crate::tagged::transport::{ConsRootSeal, RootedConsInputs, RootedSlot, SharedRoot};

/// Only a completely initialized exclusive cell can create this handoff.
/// The run loan remains active until the transport bridge installs its root.
pub(crate) struct InitializedCons<'run> {
    heap: HeapIdentity,
    value: TaggedValue,
    _run: PhantomData<&'run mut ()>,
    _thread: PhantomData<Rc<()>>,
}
impl InitializedCons<'_> {
    pub(crate) fn into_root_parts(self, _: ConsRootSeal) -> (HeapIdentity, TaggedValue) {
        (self.heap, self.value)
    }
}
static_assertions::assert_not_impl_any!(InitializedCons<'static>: Send, Sync, Clone, Copy);

impl RunAccess<'_, '_> {
    pub fn heap_identity(&self) -> HeapIdentity {
        self.owner.registry.heap
    }

    /// Allocate exactly one initialized cons and install its SharedRoot before
    /// returning. Operands remain rooted throughout; this invokes no Lisp,
    /// collection, Context accessor, payload reader or installed-heap lookup.
    #[cold]
    #[inline(never)]
    pub fn cons(
        &mut self,
        car: &SharedRoot,
        cdr: &SharedRoot,
    ) -> Result<SharedRoot, RegistrationError> {
        SharedRoot::cons_from_run(self, car, cdr)
    }

    /// The lasting single-cons primitive for rooted VM/frame or transport homes.
    /// All three homes are validated before any reservation or mutation.
    #[inline]
    pub fn cons_into<C: RootedSlot, D: RootedSlot, R: RootedSlot>(
        &mut self,
        car: &C,
        cdr: &D,
        result: &mut R,
    ) -> Result<(), RegistrationError> {
        crate::tagged::transport::cons_into(self, car, cdr, result)
    }

    #[inline]
    pub(crate) fn allocate_rooted_cons(
        &mut self,
        inputs: &RootedConsInputs<'_>,
    ) -> Result<InitializedCons<'_>, RegistrationError> {
        if inputs.heap_identity() != self.heap_identity() {
            return Err(RegistrationError::WrongHeap);
        }
        let heap = self.heap_identity();
        let mut state = self.cons_state();
        state.check_boundary()?;
        if state.region.as_ref().is_none_or(|region| region.empty()) {
            state.refill()?;
        }
        let region = state.region.as_mut().expect("reserved region");
        let (mut cursor, _) = region.slice_bounds(&state.registry.cons_catalog, state.entry.key)?;
        let (car, cdr) = inputs.words();
        // SAFETY: the same finite-region primitive used by RunSlice. Borrowed
        // rooted inputs remain live through the transport's immediate store.
        let value = unsafe { initialize_next_cons(&mut cursor, car, cdr) };
        unsafe { region.commit_slice_cursor(cursor) };

        Ok(InitializedCons {
            heap,
            value,
            _run: PhantomData,
            _thread: PhantomData,
        })
    }

    /// Close the canonical region before dropping the run or publishing.
    #[cold]
    #[inline(never)]
    pub fn close_cons_region(&mut self) -> Result<(), RegistrationError> {
        self.cons_state().close()
    }

    pub(super) fn cons_state(&mut self) -> ConsRunState<'_> {
        let local = self.owner.local.as_mut().expect("resident owner state");
        ConsRunState {
            registry: self.owner.registry,
            entry: &self.owner.entry,
            thread: self.owner.thread,
            hot: &mut local.hot,
            cold: &mut local.cold,
            stats: &mut local.region_stats,
            region: &mut local.catalog_region,
            tails: &mut local.cons_tails,
        }
    }

    #[cfg(test)]
    pub(crate) fn cons_region_for_test(&self) -> Option<(usize, usize, usize)> {
        self.owner
            .local
            .as_ref()?
            .catalog_region
            .as_ref()
            .map(|r| r.bounds_for_test())
    }

    /// The front carving tail's `(cursor, limit)` cell indexes, if the owner
    /// currently holds an unexhausted tail.
    #[cfg(test)]
    pub(crate) fn cons_tail_bounds_for_test(&self) -> Option<(u16, u16)> {
        self.owner
            .local
            .as_ref()?
            .cons_tails
            .iter()
            .find(|tail| !tail.exhausted())
            .map(|tail| tail.bounds())
    }

    /// Bases of the blocks recorded in this owner's canonical young log.
    #[cfg(test)]
    pub(crate) fn cons_young_blocks_for_test(&self) -> Vec<usize> {
        self.owner
            .local
            .as_ref()
            .expect("resident owner state")
            .cold
            .young
            .resident()
            .expect("resident young log")
            .blocks()
            .iter()
            .map(|base| base.addr())
            .collect()
    }

    /// One cold batch grant attempt, for protocol tests.
    #[cfg(test)]
    pub(crate) fn cons_probe_tail_batch_for_test(&mut self) -> Result<(), RegistrationError> {
        let state = self.cons_state();
        state.check_boundary()?;
        let mut tails = state.registry.cons_catalog.grant_tail_batch(
            state.entry.key,
            state.cold.young.resident_mut()?,
            super::super::cons_storage::TAIL_BATCH_CELLS,
        )?;
        state.tails.append(&mut tails);
        Ok(())
    }

    /// Drop every held tail without publishing: the failure path a lost or
    /// panicked owner takes. The catalog must refuse to serve afterwards.
    #[cfg(test)]
    pub(crate) fn cons_drop_tails_for_test(&mut self) {
        let local = self.owner.local.as_mut().expect("resident owner state");
        local.cons_tails.clear();
    }

    /// Per-owner refill accounting for the A8 contention rows.
    #[cfg(test)]
    pub(crate) fn cons_stats_for_test(&self) -> super::super::alloc_region::RegionStats {
        self.owner
            .local
            .as_ref()
            .expect("resident owner state")
            .region_stats
            .clone()
    }
}

impl Mutator<'_> {
    /// Hand every held cons tail suffix back under the catalog gate before an
    /// image publication. Only publication paths call this, after their loan
    /// checks proved no run borrow can hold a tail; the steady single-thread
    /// state holds none and takes no lock at all.
    pub(super) fn publish_cons_tails(&mut self) -> Result<(), RegistrationError> {
        let Some(local) = self.local.as_mut() else {
            return Err(RegistrationError::Publication);
        };
        if local.cons_tails.is_empty() {
            return Ok(());
        }
        let tails = std::mem::take(&mut local.cons_tails);
        self.registry
            .cons_catalog
            .publish_tails(self.entry.key, tails)?;
        Ok(())
    }
}

/// Disjoint allocator loans: cold refill never reborrows the root bank or a
/// whole OwnerGcState/heap while branded root pointers are live.
pub(super) struct ConsRunState<'run> {
    pub(super) registry: &'run Registry,
    pub(super) entry: &'run MutatorEntry,
    pub(super) thread: ThreadId,
    pub(super) hot: &'run mut super::super::mutator_gc::MutatorGcState,
    pub(super) cold: &'run mut super::super::mutator_gc::SatbMutatorState,
    pub(super) stats: &'run mut super::super::alloc_region::RegionStats,
    pub(super) region: &'run mut Option<super::super::cons_storage::CatalogRegion>,
    pub(super) tails: &'run mut Vec<super::super::cons_storage::OwnedConsTail>,
}
impl ConsRunState<'_> {
    pub(super) fn check_boundary(&self) -> Result<(), RegistrationError> {
        if self.thread != std::thread::current().id() {
            return Err(RegistrationError::WrongThread);
        }
        if self.registry.park.poisoned.load(Ordering::Acquire) {
            return Err(RegistrationError::Poisoned);
        }
        if !self.entry.admitted() {
            return Err(RegistrationError::OutstandingLoan);
        }
        self.registry.cons_catalog.check_live()
    }

    #[cold]
    #[inline(never)]
    pub(super) fn refill(&mut self) -> Result<(), RegistrationError> {
        self.close()?;
        const MAX: usize = super::super::alloc_region::CONS_REGION_MAX_CELLS;
        self.hot
            .allocated_count
            .checked_add(MAX)
            .ok_or(RegistrationError::Capacity)?;
        self.hot
            .bytes_since_gc
            .checked_add(MAX * size_of::<ConsCell>())
            .ok_or(RegistrationError::Capacity)?;
        if self.cold.young.id() != self.entry.young {
            return Err(RegistrationError::StaleRegistration);
        }
        // Spent tails stay in the list until publication: only the ACK-side
        // gate may decrement the catalog's lent-range charge, so dropping one
        // early would strand active_regions above zero forever. Carving just
        // skips them; one list drains fully at the next publish.
        if self
            .tails
            .iter()
            .all(super::super::cons_storage::OwnedConsTail::exhausted)
        {
            let granted = self.registry.cons_catalog.grant_tail_batch(
                self.entry.key,
                self.cold.young.resident_mut()?,
                super::super::cons_storage::TAIL_BATCH_CELLS,
            )?;
            self.tails.extend(granted);
        }
        let tail = self
            .tails
            .iter_mut()
            .find(|tail| !tail.exhausted())
            .expect("granted tail");
        let region = tail.carve(self.entry.key, MAX);
        let n = region.total();
        self.hot.allocated_count += n;
        let slot = MemoryUseCountSlot::ConsCells.index();
        self.hot.memory_use_counts[slot] = self.hot.memory_use_counts[slot].wrapping_add(n as u64);
        self.hot.bytes_since_gc += n * size_of::<ConsCell>();
        self.stats.cons_refills += 1;
        self.stats.cons_cells_granted += n as u64;
        self.stats.cons_short_runs += u64::from(n < 8);
        match region.source() {
            CatalogSource::FreeList => self.stats.cons_from_free_list += 1,
            CatalogSource::BlockTail => self.stats.cons_from_block_tail += 1,
            CatalogSource::FreshBlock => self.stats.cons_from_fresh_block += 1,
        }
        *self.region = Some(region);
        Ok(())
    }

    #[cold]
    #[inline(never)]
    pub(super) fn close(&mut self) -> Result<(), RegistrationError> {
        self.check_boundary()?;
        let Some(mut region) = self.region.take() else {
            return Ok(());
        };
        // Fast local path: the region was carved from one of this owner's
        // exclusive tails, so the rewind only moves that tail's cursor.
        for tail in self.tails.iter_mut() {
            if let Some(unused) = tail.rewind_region(&region) {
                region.retire();
                self.refund(unused);
                return Ok(());
            }
        }
        // A region no owned tail can claim is a protocol violation: the
        // gated close fails loudly (and poisons) instead of refunding.
        let unused = match self.registry.cons_catalog.close(self.entry.key, region) {
            Ok(unused) => unused,
            Err((error, region)) => {
                *self.region = Some(region);
                self.registry.poison();
                return Err(error);
            }
        };
        self.refund(unused);
        Ok(())
    }

    fn refund(&mut self, unused: usize) {
        self.hot.allocated_count -= unused;
        self.hot.bytes_since_gc -= unused * size_of::<ConsCell>();
        let slot = MemoryUseCountSlot::ConsCells.index();
        self.hot.memory_use_counts[slot] =
            self.hot.memory_use_counts[slot].wrapping_sub(unused as u64);
        self.stats.cons_tail_closes += u64::from(unused != 0);
    }
}

/// The one admitted initialization/bump primitive. The caller owns a nonempty
/// region and keeps both operand homes rooted until the immediate result store.
#[inline(always)]
pub(super) unsafe fn initialize_next_cons(
    cursor: &mut *mut ConsCell,
    car: usize,
    cdr: usize,
) -> TaggedValue {
    let cell = *cursor;
    // SAFETY: caller proves the finite exclusive range has another cell.
    unsafe {
        cell.write(ConsCell {
            car: crate::tagged::header::ConsWord::new(TaggedValue::from_bits(car)),
            cdr_or_next: crate::tagged::header::ConsCdrWord::new(TaggedValue::from_bits(cdr)),
        });
        *cursor = cell.add(1);
        TaggedValue::from_cons_ptr(cell)
    }
}
