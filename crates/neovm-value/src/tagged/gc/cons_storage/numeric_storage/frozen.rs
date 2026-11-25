//! Frozen numeric ownership never forms an arena, Vec, slice, or page
//! reference. A cold Box owns the complete stopped bank at a stable raw
//! designator. Only the checked zero-view transition reconstructs that owner.

use super::super::CatalogIdentity;
use super::retained::{RetainedPage, refresh_retained};
use super::runs::{BignumRun, FloatRun, RawPage, RunCore};
use super::{HeapIdentity, MutableNumericStorage, MutatorKey, NumericRetention, RegistrationError};
use crate::tagged::gc::arena_pages::{ObjectPage, PagedObject};
use crate::tagged::header::{BignumObj, FloatObj, MarkParity};
use rustc_hash::FxHashSet;
use std::mem::ManuallyDrop;
use std::sync::Arc;
use std::sync::atomic::Ordering;

enum PageState {
    Available {
        frontier: usize,
    },
    Claimed {
        owner: MutatorKey,
        claim: u64,
        start: usize,
    },
}

/// Token metadata, not an arena descriptor or a reference into its Vec.
/// Pool metadata may be examined under the gate while raw page views exist.
struct PoolPage {
    raw: RawPage,
    state: PageState,
}

/// The descriptor Vecs live only behind `backing`, an owned raw Box pointer.
/// `&mut FrozenNumericStorage` therefore never reborrows those Vecs. This
/// type deliberately has no indexing/iteration/growth/restore/drop API for
/// backing; its only exit is the checked zero-view transition below.
pub(in crate::tagged::gc) struct FrozenNumericStorage {
    backing: *mut MutableNumericStorage,
    heap: HeapIdentity,
    identity: Arc<CatalogIdentity>,
    generation: u64,
    parity: MarkParity,
    floats: Vec<PoolPage>,
    bignums: Vec<PoolPage>,
    claims: usize,
    next_claim: u64,
}

impl MutableNumericStorage {
    #[cold]
    pub(super) fn freeze(
        mut self,
        float_pages: usize,
        bignum_pages: usize,
        retained: &NumericRetention,
        heap: HeapIdentity,
        identity: Arc<CatalogIdentity>,
        generation: u64,
        parity: MarkParity,
    ) -> Result<FrozenNumericStorage, (RegistrationError, Self)> {
        if identity.abandoned.load(Ordering::Acquire) {
            return Err((RegistrationError::Poisoned, self));
        }
        let prepared_floats = match refresh_retained(&self.floats, &retained.floats) {
            Ok(pages) => pages,
            Err(error) => return Err((error, self)),
        };
        let prepared_bignums = match refresh_retained(&self.bignums, &retained.bignums) {
            Ok(pages) => pages,
            Err(error) => return Err((error, self)),
        };
        let Some(float_total) = float_pages.checked_add(prepared_floats.len()) else {
            return Err((RegistrationError::Capacity, self));
        };
        let Some(bignum_total) = bignum_pages.checked_add(prepared_bignums.len()) else {
            return Err((RegistrationError::Capacity, self));
        };
        let mut floats = Vec::new();
        let mut bignums = Vec::new();
        let mut float_indices = Vec::new();
        let mut bignum_indices = Vec::new();
        let mut retained_floats = FxHashSet::default();
        let mut retained_bignums = FxHashSet::default();
        // Every recoverable metadata reservation precedes arena mutation.
        if floats.try_reserve_exact(float_total).is_err()
            || bignums.try_reserve_exact(bignum_total).is_err()
            || float_indices.try_reserve_exact(float_total).is_err()
            || bignum_indices.try_reserve_exact(bignum_total).is_err()
            || retained_floats.try_reserve(prepared_floats.len()).is_err()
            || retained_bignums
                .try_reserve(prepared_bignums.len())
                .is_err()
        {
            return Err((RegistrationError::Capacity, self));
        }
        for page in &prepared_floats {
            let index = self
                .floats
                .recover_numeric_owner_page(page.base, &retained_floats);
            let inserted = retained_floats.insert(index);
            debug_assert!(inserted);
            float_indices.push((index, page.frontier));
        }
        for page in &prepared_bignums {
            let index = self
                .bignums
                .recover_numeric_owner_page(page.base, &retained_bignums);
            let inserted = retained_bignums.insert(index);
            debug_assert!(inserted);
            bignum_indices.push((index, page.frontier));
        }
        let mut first_index = 0;
        for _ in 0..float_pages {
            let index = self
                .floats
                .mint_uninit_numeric_owner_page_excluding(first_index, &retained_floats);
            first_index = index + 1;
            float_indices.push((index, 0));
        }
        first_index = 0;
        for _ in 0..bignum_pages {
            let index = self
                .bignums
                .mint_uninit_numeric_owner_page_excluding(first_index, &retained_bignums);
            first_index = index + 1;
            bignum_indices.push((index, 0));
        }

        // One cold bank allocation per freeze, not a serial bitmap
        // indirection. All page-Vec/map growth is now complete. Put ownership
        // at its final raw designator BEFORE deriving any descriptor views.
        let backing = Box::into_raw(Box::new(self));
        // SAFETY: no view exists yet. These field-only reborrows derive each
        // descriptor buffer's raw base once. No later Frozen method forms any
        // reference to the arena, Vec owner, or ObjectPage elements.
        let (float_base, bignum_base) = unsafe {
            (
                (&mut *std::ptr::addr_of_mut!((*backing).floats.pages)).as_mut_ptr(),
                (&mut *std::ptr::addr_of_mut!((*backing).bignums.pages)).as_mut_ptr(),
            )
        };
        for (page, frontier) in float_indices {
            // SAFETY: the final buffer contains the minted index, and no
            // reference to it has been created since its raw base was derived.
            let raw = unsafe { mint_raw_page::<FloatObj>(float_base, page) };
            floats.push(PoolPage {
                raw,
                state: PageState::Available { frontier },
            });
        }
        for (page, frontier) in bignum_indices {
            // SAFETY: as above, for the separately allocated bignum buffer.
            let raw = unsafe { mint_raw_page::<BignumObj>(bignum_base, page) };
            bignums.push(PoolPage {
                raw,
                state: PageState::Available { frontier },
            });
        }
        Ok(FrozenNumericStorage {
            backing,
            heap,
            identity,
            generation,
            parity,
            floats,
            bignums,
            claims: 0,
            next_claim: 1,
        })
    }
}

/// Derive raw addresses of disjoint initialized descriptor fields, rather
/// than materializing `&ObjectPage` and invalidating another field's view.
///
/// # Safety
/// `base` points to the final, stable descriptor buffer and `index` is in
/// bounds. No owner view exists yet. The backing bank remains allocated and
/// no arena, Vec-owner, or descriptor reference may be formed after these
/// raw field pointers are derived until the checked zero-view thaw retires
/// every view.
unsafe fn mint_raw_page<T: PagedObject>(base: *mut ObjectPage<T>, index: usize) -> RawPage {
    // SAFETY: the caller proves this index is in the final descriptor buffer.
    unsafe {
        let descriptor = base.add(index);
        let (payload, bits, occupancy) = ObjectPage::raw_frozen_parts(descriptor);
        RawPage {
            page: index,
            payload,
            bits,
            occupancy,
            slots: ObjectPage::<T>::SLOTS,
        }
    }
}

impl FrozenNumericStorage {
    fn check(&self, owner: MutatorKey, generation: u64) -> Result<(), RegistrationError> {
        if self.identity.abandoned.load(Ordering::Acquire) {
            return Err(RegistrationError::Poisoned);
        }
        if owner.heap_identity() != self.heap {
            return Err(RegistrationError::WrongHeap);
        }
        if generation != self.generation {
            return Err(RegistrationError::StaleRegistration);
        }
        Ok(())
    }

    /// The catalog mutex transfers every bitmap word and the occupancy
    /// field at close/reclaim. There are no concurrent sub-page runs, so the
    /// next owner may use a word that also contains the old frontier prefix.
    fn assert_unique_claim(&self, selected: RawPage) {
        let live = self
            .floats
            .iter()
            .chain(self.bignums.iter())
            .filter(|page| matches!(page.state, PageState::Claimed { .. }))
            .count();
        assert_eq!(
            live, self.claims,
            "numeric claim metadata must retain every live page view"
        );
        assert!(
            self.floats
                .iter()
                .chain(self.bignums.iter())
                .all(|page| !matches!(page.state, PageState::Claimed { .. })
                    || page.raw.payload != selected.payload),
            "one live claim per physical page: catalog mutex handoff replaces concurrent sub-page word alignment"
        );
    }

    pub(super) fn claim_float(
        &mut self,
        owner: MutatorKey,
        generation: u64,
    ) -> Result<Option<FloatRun>, RegistrationError> {
        self.check(owner, generation)?;
        let Some(pool_index) = self
            .floats
            .iter()
            .position(|page| matches!(page.state, PageState::Available { frontier } if frontier < page.raw.slots))
        else {
            return Ok(None);
        };
        let claim = self.next_claim;
        let next = claim.checked_add(1).ok_or(RegistrationError::Capacity)?;
        self.assert_unique_claim(self.floats[pool_index].raw);
        let page = &mut self.floats[pool_index];
        let PageState::Available { frontier: start } = page.state else {
            unreachable!("selected an available partial page")
        };
        page.state = PageState::Claimed {
            owner,
            claim,
            start,
        };
        self.next_claim = next;
        self.claims += 1;
        Ok(Some(FloatRun {
            core: RunCore::new(
                page.raw,
                Arc::clone(&self.identity),
                owner,
                generation,
                claim,
                pool_index,
                self.parity,
                start,
            ),
        }))
    }

    pub(super) fn claim_bignum(
        &mut self,
        owner: MutatorKey,
        generation: u64,
    ) -> Result<Option<BignumRun>, RegistrationError> {
        self.check(owner, generation)?;
        let Some(pool_index) = self
            .bignums
            .iter()
            .position(|page| matches!(page.state, PageState::Available { frontier } if frontier < page.raw.slots))
        else {
            return Ok(None);
        };
        let claim = self.next_claim;
        let next = claim.checked_add(1).ok_or(RegistrationError::Capacity)?;
        self.assert_unique_claim(self.bignums[pool_index].raw);
        let page = &mut self.bignums[pool_index];
        let PageState::Available { frontier: start } = page.state else {
            unreachable!("selected an available partial page")
        };
        page.state = PageState::Claimed {
            owner,
            claim,
            start,
        };
        self.next_claim = next;
        self.claims += 1;
        Ok(Some(BignumRun {
            core: RunCore::new(
                page.raw,
                Arc::clone(&self.identity),
                owner,
                generation,
                claim,
                pool_index,
                self.parity,
                start,
            ),
        }))
    }

    fn check_return(
        &self,
        owner: MutatorKey,
        core: &RunCore,
        pool: &[PoolPage],
    ) -> Result<(), RegistrationError> {
        self.check(owner, core.generation)?;
        if owner.owner() != core.owner.owner() {
            return Err(RegistrationError::WrongOwner);
        }
        if owner != core.owner || !Arc::ptr_eq(&core.identity, &self.identity) {
            return Err(RegistrationError::StaleRegistration);
        }
        let page = pool
            .get(core.pool_index)
            .ok_or(RegistrationError::StaleRegistration)?;
        if !matches!(page.state, PageState::Claimed { owner: bound, claim, start }
            if bound == owner && claim == core.claim && start == core.start)
            || page.raw.page != core.page.page
            || page.raw.payload != core.page.payload
            || page.raw.bits != core.page.bits
            || page.raw.occupancy != core.page.occupancy
            || page.raw.slots != core.page.slots
            || core.start > core.used
            || core.used > core.page.slots
        {
            return Err(RegistrationError::StaleRegistration);
        }
        Ok(())
    }

    pub(super) fn settle_float(
        &mut self,
        owner: MutatorKey,
        mut run: FloatRun,
    ) -> Result<(), (RegistrationError, FloatRun)> {
        if let Err(error) = self.check_return(owner, &run.core, &self.floats) {
            return Err((error, run));
        }
        self.floats[run.core.pool_index].state = PageState::Available {
            frontier: run.core.used,
        };
        self.claims -= 1;
        run.core.retire();
        Ok(())
    }

    pub(super) fn settle_bignum(
        &mut self,
        owner: MutatorKey,
        mut run: BignumRun,
    ) -> Result<(), (RegistrationError, BignumRun)> {
        if let Err(error) = self.check_return(owner, &run.core, &self.bignums) {
            return Err((error, run));
        }
        self.bignums[run.core.pool_index].state = PageState::Available {
            frontier: run.core.used,
        };
        self.claims -= 1;
        run.core.retire();
        Ok(())
    }

    /// Copy identities/frontiers, never the raw view fields. Full pages have
    /// no remaining tail and deliberately leave the owner pool at this stop.
    fn retention_snapshot(&self) -> Result<NumericRetention, RegistrationError> {
        let mut retained = NumericRetention::default();
        retained
            .floats
            .try_reserve_exact(self.floats.len())
            .map_err(|_| RegistrationError::Capacity)?;
        retained
            .bignums
            .try_reserve_exact(self.bignums.len())
            .map_err(|_| RegistrationError::Capacity)?;
        for page in &self.floats {
            if let PageState::Available { frontier } = page.state {
                if frontier < page.raw.slots {
                    retained.floats.push(RetainedPage {
                        base: page.raw.payload as usize,
                        frontier,
                    });
                }
            } else {
                return Err(RegistrationError::OpenRegion);
            }
        }
        for page in &self.bignums {
            if let PageState::Available { frontier } = page.state {
                if frontier < page.raw.slots {
                    retained.bignums.push(RetainedPage {
                        base: page.raw.payload as usize,
                        frontier,
                    });
                }
            } else {
                return Err(RegistrationError::OpenRegion);
            }
        }
        Ok(retained)
    }

    /// The parent additionally holds matching AllStopped. This is the only
    /// transition that can regain arena ownership, and only at zero views.
    #[cold]
    pub(super) fn into_mutable(
        self,
        generation: u64,
    ) -> Result<(MutableNumericStorage, NumericRetention), (RegistrationError, Self)> {
        if self.identity.abandoned.load(Ordering::Acquire) {
            return Err((RegistrationError::Poisoned, self));
        }
        if generation != self.generation {
            return Err((RegistrationError::StaleRegistration, self));
        }
        if self.claims != 0 {
            return Err((RegistrationError::OpenRegion, self));
        }
        // Check only raw bitmap/count fields while backing remains frozen.
        // No mutable arena reference is formed on a fallible path: that would
        // invalidate raw views cached for a possible later claim/retry.
        let invalid = self
            .floats
            .iter()
            .chain(self.bignums.iter())
            .find_map(|page| {
                let frontier = match page.state {
                    PageState::Available { frontier } => frontier,
                    PageState::Claimed { .. } => return Some(RegistrationError::OpenRegion),
                };
                // SAFETY: claims is zero, and the unique settled runs have been
                // consumed. The caller additionally holds the stopped transition.
                if unsafe { page.raw.has_initialized_frontier(frontier) } {
                    None
                } else {
                    Some(RegistrationError::Publication)
                }
            });
        if let Some(error) = invalid {
            return Err((error, self));
        }
        let retention = match self.retention_snapshot() {
            Ok(retention) => retention,
            Err(error) => return Err((error, self)),
        };
        let this = ManuallyDrop::new(self);
        // SAFETY: all unique claims have settled. The old raw fields are now
        // permanently retired, and Frozen Drop is disabled for this successful
        // transition. Reconstruct the original Box exactly once.
        unsafe {
            let mut mutable = Box::from_raw(this.backing);
            let floats = std::ptr::read(std::ptr::addr_of!(this.floats));
            let bignums = std::ptr::read(std::ptr::addr_of!(this.bignums));
            let identity = std::ptr::read(std::ptr::addr_of!(this.identity));
            for page in floats {
                let frontier = match page.state {
                    PageState::Available { frontier } => frontier,
                    PageState::Claimed { .. } => unreachable!("validated zero-view pool"),
                };
                mutable
                    .floats
                    .reconcile_numeric_owner_page(page.raw.page, frontier);
            }
            for page in bignums {
                let frontier = match page.state {
                    PageState::Available { frontier } => frontier,
                    PageState::Claimed { .. } => unreachable!("validated zero-view pool"),
                };
                mutable
                    .bignums
                    .reconcile_numeric_owner_page(page.raw.page, frontier);
            }
            drop(identity);
            Ok((*mutable, retention))
        }
    }
}

impl Drop for FrozenNumericStorage {
    fn drop(&mut self) {
        // There is deliberately no Box::from_raw here. A lost pool cannot
        // establish stopped ownership, even if no claim currently survives.
        // Retaining the whole backing also retains initialized Integer limbs.
        self.identity.abandoned.store(true, Ordering::Release);
    }
}

static_assertions::assert_not_impl_any!(FrozenNumericStorage: Send, Sync, Clone, Copy);
