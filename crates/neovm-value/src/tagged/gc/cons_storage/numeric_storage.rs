//! Numeric storage has two phases: stopped descriptor ownership and frozen
//! raw owner views. The frozen type has no arena/Vec access or restoration API.
//! Every set allocation bit always describes a completely initialized payload.

use super::super::TaggedHeap;
use super::super::arena_pages::{ObjectArena, ObjectPage, PagedObject};
use super::super::chunk_map::ChunkMap;
use super::{CatalogIdentity, HeapIdentity, MutatorKey, RegistrationError};
use crate::tagged::header::{BignumObj, FloatObj, MarkParity};
use std::sync::Arc;

mod frozen;
mod retained;
mod runs;
pub(in crate::tagged::gc) use retained::NumericRetention;

pub(in crate::tagged::gc) use frozen::FrozenNumericStorage;
pub(crate) use runs::InitializedNumeric;
pub(in crate::tagged::gc) use runs::{BignumRun, FloatRun};

const MAX_POOL_PAGES: usize = 64;

/// Whole-page policy: twice the preceding interval, a one-page floor, and
/// the existing per-owner reserve ceiling. The pool itself is unkeyed.
pub(in crate::tagged::gc) fn pool_pages_for_interval(
    float_slots: u64,
    bignum_slots: u64,
) -> (usize, usize) {
    fn pages(measured: u64, slots: usize) -> usize {
        measured
            .saturating_mul(2)
            .max(slots as u64)
            .div_ceil(slots as u64)
            .min(MAX_POOL_PAGES as u64) as usize
    }
    (
        pages(float_slots, ObjectPage::<FloatObj>::SLOTS),
        pages(bignum_slots, ObjectPage::<BignumObj>::SLOTS),
    )
}

/// Only this stopped type may form arena or descriptor-buffer references.
struct MutableNumericStorage {
    floats: ObjectArena<FloatObj>,
    bignums: ObjectArena<BignumObj>,
}

enum Bank {
    Mutable(MutableNumericStorage),
    Frozen(FrozenNumericStorage),
}

/// Compatibility envelope for N1's existing stopped catalog transaction.
/// Expected placeholder identities are copied before freezing, separately
/// from the arena ownership: checking the shell never borrows frozen backing.
pub(super) struct NumericStorage {
    bank: Option<Bank>,
    heap: HeapIdentity,
    float_map: Option<Arc<ChunkMap>>,
    bignum_map: Option<Arc<ChunkMap>>,
    retention: NumericRetention,
}

/// A stopped, fully thawed bank whose exit cannot inspect Frozen storage.
/// Only `take_restorable` can construct it, before the catalog is taken.
pub(super) struct RestorableNumericStorage {
    mutable: MutableNumericStorage,
    retention: NumericRetention,
}

impl RestorableNumericStorage {
    pub(super) fn restore(self, heap: &mut TaggedHeap) -> NumericRetention {
        heap.float_arena = self.mutable.floats;
        heap.bignum_arena = self.mutable.bignums;
        self.retention
    }
}

impl NumericStorage {
    pub(super) fn detach(heap: &mut TaggedHeap) -> Self {
        let floats = heap.float_arena.empty_for_transfer();
        let bignums = heap.bignum_arena.empty_for_transfer();
        let float_map = heap.float_arena.numeric_transfer_map();
        let bignum_map = heap.bignum_arena.numeric_transfer_map();
        Self {
            heap: heap.heap_identity(),
            float_map,
            bignum_map,
            retention: NumericRetention::default(),
            bank: Some(Bank::Mutable(MutableNumericStorage {
                floats: std::mem::replace(&mut heap.float_arena, floats),
                bignums: std::mem::replace(&mut heap.bignum_arena, bignums),
            })),
        }
    }

    /// Freeze only under the parent's matching stopped catalog transaction,
    /// with no existing numeric view. All descriptor growth finishes first.
    #[cold]
    pub(super) fn freeze_new_pool(
        &mut self,
        float_pages: usize,
        bignum_pages: usize,
        retained: &NumericRetention,
        catalog: Arc<CatalogIdentity>,
        generation: u64,
        parity: MarkParity,
    ) -> Result<(), RegistrationError> {
        // The recipe is capped, outstanding exhaustion requests are not:
        // every requester must be able to claim a page after the same stop.
        if float_pages == 0 && bignum_pages == 0 && retained.is_empty() {
            return Err(RegistrationError::Capacity);
        }
        let Some(Bank::Mutable(_)) = self.bank.as_ref() else {
            return Err(RegistrationError::Busy);
        };
        let Some(Bank::Mutable(mutable)) = self.bank.take() else {
            unreachable!("checked mutable numeric bank")
        };
        match mutable.freeze(
            float_pages,
            bignum_pages,
            retained,
            self.heap,
            catalog,
            generation,
            parity,
        ) {
            Ok(frozen) => {
                self.bank = Some(Bank::Frozen(frozen));
                Ok(())
            }
            Err((error, mutable)) => {
                self.bank = Some(Bank::Mutable(mutable));
                Err(error)
            }
        }
    }

    pub(super) fn claim_float(
        &mut self,
        owner: MutatorKey,
        generation: u64,
    ) -> Result<Option<FloatRun>, RegistrationError> {
        match self.bank.as_mut() {
            Some(Bank::Frozen(bank)) => bank.claim_float(owner, generation),
            _ => Err(RegistrationError::Publication),
        }
    }

    pub(super) fn claim_bignum(
        &mut self,
        owner: MutatorKey,
        generation: u64,
    ) -> Result<Option<BignumRun>, RegistrationError> {
        match self.bank.as_mut() {
            Some(Bank::Frozen(bank)) => bank.claim_bignum(owner, generation),
            _ => Err(RegistrationError::Publication),
        }
    }

    pub(super) fn settle_float(
        &mut self,
        owner: MutatorKey,
        run: FloatRun,
    ) -> Result<(), (RegistrationError, FloatRun)> {
        match self.bank.as_mut() {
            Some(Bank::Frozen(bank)) => bank.settle_float(owner, run),
            _ => Err((RegistrationError::Publication, run)),
        }
    }

    pub(super) fn settle_bignum(
        &mut self,
        owner: MutatorKey,
        run: BignumRun,
    ) -> Result<(), (RegistrationError, BignumRun)> {
        match self.bank.as_mut() {
            Some(Bank::Frozen(bank)) => bank.settle_bignum(owner, run),
            _ => Err((RegistrationError::Publication, run)),
        }
    }

    /// The caller holds matching AllStopped and calls this BEFORE taking the
    /// catalog state. A refusal preserves the complete frozen bank for retry.
    #[cold]
    pub(super) fn checked_thaw(&mut self, generation: u64) -> Result<(), RegistrationError> {
        let Some(Bank::Frozen(_)) = self.bank.as_ref() else {
            return if matches!(self.bank, Some(Bank::Mutable(_))) {
                Ok(())
            } else {
                Err(RegistrationError::Publication)
            };
        };
        let Some(Bank::Frozen(frozen)) = self.bank.take() else {
            unreachable!("checked frozen numeric bank")
        };
        match frozen.into_mutable(generation) {
            Ok((mutable, retention)) => {
                self.bank = Some(Bank::Mutable(mutable));
                self.retention = retention;
                Ok(())
            }
            Err((error, frozen)) => {
                self.bank = Some(Bank::Frozen(frozen));
                Err(error)
            }
        }
    }

    pub(super) fn placeholders_intact(&self, heap: &TaggedHeap) -> bool {
        // N1's infallible restoration is available only after checked thaw.
        matches!(self.bank, Some(Bank::Mutable(_)))
            && heap
                .float_arena
                .is_numeric_transfer_placeholder(self.float_map.as_ref())
            && heap
                .bignum_arena
                .is_numeric_transfer_placeholder(self.bignum_map.as_ref())
    }

    /// Extract stopped ownership only after checked thaw and placeholder
    /// validation. Refusal leaves Frozen storage and its metadata untouched.
    pub(super) fn take_restorable(
        &mut self,
    ) -> Result<RestorableNumericStorage, RegistrationError> {
        match self.bank.take() {
            Some(Bank::Mutable(mutable)) => Ok(RestorableNumericStorage {
                mutable,
                retention: std::mem::take(&mut self.retention),
            }),
            bank => {
                self.bank = bank;
                Err(RegistrationError::Publication)
            }
        }
    }
}

static_assertions::assert_not_impl_any!(NumericStorage: Clone, Copy);
