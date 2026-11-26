//! Cold numeric policy and catalog-gated claims. Exhaustion requests are counts,
//! not owner-keyed reservations. No page descriptor or payload is borrowed here.

use super::numeric_storage::{NumericRetention, pool_pages_for_interval};
use super::{BignumRun, ConsCatalog, FloatRun, MutatorKey, RegistrationError};
use std::sync::MutexGuard;
use std::sync::atomic::Ordering;

#[derive(Clone, Copy, Default)]
pub(super) struct NumericInterval {
    floats: u64,
    bignums: u64,
}

#[derive(Clone, Copy)]
pub(super) enum NumericEpoch {
    Unprepared,
    Frozen {
        generation: u64,
        /// An outer resume can fail after the pool itself committed. Its
        /// terminal retained bank keeps this exact immutable consumption
        /// receipt; a future unpoison protocol would have to rearm from it.
        float_requests: u64,
        bignum_requests: u64,
    },
}

impl NumericEpoch {
    pub(super) fn generation(self) -> u64 {
        match self {
            Self::Unprepared => 0,
            Self::Frozen { generation, .. } => generation,
        }
    }
}

/// This history is acquired only after the existing catalog state gate.
/// It survives explicit return to the heap without growing any serial state.
pub(super) struct NumericHistory {
    next_generation: u64,
    previous: NumericInterval,
    retention: NumericRetention,
    float_requests: u64,
    bignum_requests: u64,
    #[cfg(any(test, feature = "test-support"))]
    commit_observer: Option<Box<dyn FnOnce() + Send>>,
}

pub(super) struct NumericPlan {
    pub(super) float_pages: usize,
    pub(super) bignum_pages: usize,
    pub(super) generation: u64,
    float_requests: u64,
    bignum_requests: u64,
}

impl NumericPlan {
    pub(super) fn epoch(&self) -> NumericEpoch {
        NumericEpoch::Frozen {
            generation: self.generation,
            float_requests: self.float_requests,
            bignum_requests: self.bignum_requests,
        }
    }
}

impl NumericHistory {
    pub(super) fn new() -> Self {
        Self {
            next_generation: 1,
            previous: NumericInterval::default(),
            retention: NumericRetention::default(),
            float_requests: 0,
            bignum_requests: 0,
            #[cfg(any(test, feature = "test-support"))]
            commit_observer: None,
        }
    }

    /// A failed preparation may burn a generation but never consumes a
    /// request or reuses an issued generation. Retained partial pages refreeze
    /// without fresh minting; only a requesting class receives additional pages.
    pub(super) fn plan(&mut self) -> Result<Option<NumericPlan>, RegistrationError> {
        // Even a no-pool install is a distinct request epoch. Reusing zero
        // for every Unprepared state would let an owner that skipped an entire
        // frozen interval mistake its old pending fact for a current request.
        let generation = self.next_generation;
        self.next_generation = generation
            .checked_add(1)
            .ok_or(RegistrationError::Capacity)?;
        if self.float_requests == 0 && self.bignum_requests == 0 && self.retention.is_empty() {
            return Ok(None);
        }
        let (floats, bignums) =
            pool_pages_for_interval(self.previous.floats, self.previous.bignums);
        let float_requests =
            usize::try_from(self.float_requests).map_err(|_| RegistrationError::Capacity)?;
        let bignum_requests =
            usize::try_from(self.bignum_requests).map_err(|_| RegistrationError::Capacity)?;
        Ok(Some(NumericPlan {
            float_pages: if float_requests == 0 {
                0
            } else {
                floats.max(float_requests)
            },
            bignum_pages: if bignum_requests == 0 {
                0
            } else {
                bignums.max(bignum_requests)
            },
            generation,
            float_requests: self.float_requests,
            bignum_requests: self.bignum_requests,
        }))
    }

    pub(super) fn commit_mint(&mut self, plan: NumericPlan) {
        // Both locks remain held across planning and installation: no request
        // can race this commit or disappear on a failed freeze.
        debug_assert_eq!(self.float_requests, plan.float_requests);
        debug_assert_eq!(self.bignum_requests, plan.bignum_requests);
        self.float_requests = 0;
        self.bignum_requests = 0;
        #[cfg(any(test, feature = "test-support"))]
        if let Some(observer) = self.commit_observer.take() {
            observer();
        }
    }

    pub(super) fn retained(&self) -> &NumericRetention {
        &self.retention
    }

    pub(super) fn finish_interval(
        &mut self,
        interval: NumericInterval,
        retention: NumericRetention,
    ) {
        self.previous = interval;
        self.retention = retention;
    }

    fn request_float(&mut self) -> Result<(), RegistrationError> {
        self.float_requests = self
            .float_requests
            .checked_add(1)
            .ok_or(RegistrationError::Capacity)?;
        Ok(())
    }

    fn request_bignum(&mut self) -> Result<(), RegistrationError> {
        self.bignum_requests = self
            .bignum_requests
            .checked_add(1)
            .ok_or(RegistrationError::Capacity)?;
        Ok(())
    }
}

impl ConsCatalog {
    /// Fixed order: the caller already holds the existing state gate.
    pub(super) fn lock_numeric_history(
        &self,
    ) -> Result<MutexGuard<'_, NumericHistory>, RegistrationError> {
        self.numeric_history.lock().map_err(|_| {
            self.identity.abandoned.store(true, Ordering::Release);
            RegistrationError::Poisoned
        })
    }

    /// The canonical caller validates the admitted entry/full owner identity.
    /// Generation is returned even for an empty pool, so the canonical
    /// owner book can retain one request fact per class/generation. A stale
    /// fact is ignored after refreeze. Subsequent slot commits never reacquire
    /// either catalog lock; the pool remains unkeyed.
    #[cold]
    pub(in crate::tagged::gc) fn claim_float(
        &self,
        owner: MutatorKey,
        requested_generation: Option<u64>,
    ) -> Result<(u64, Option<FloatRun>), RegistrationError> {
        self.admit_slice(owner)?;
        let mut gate = self.lock()?;
        let state = gate.as_mut().ok_or(RegistrationError::Publication)?;
        let next_active = state
            .active_regions
            .checked_add(1)
            .ok_or(RegistrationError::Capacity)?;
        let (generation, claim) = match state.numeric_epoch {
            NumericEpoch::Unprepared => {
                // plan issues an epoch for every successful install, including
                // no-pool N1 transfer. The state gate keeps it stable here.
                (self.lock_numeric_history()?.next_generation - 1, None)
            }
            NumericEpoch::Frozen { generation, .. } => {
                (generation, state.numeric.claim_float(owner, generation)?)
            }
        };
        match claim {
            Some(run) => {
                state.active_regions = next_active;
                Ok((generation, Some(run)))
            }
            None => {
                // Only a validated empty claim asks the next stopped resume
                // for another unkeyed page. This is not a page/owner lease.
                if requested_generation != Some(generation) {
                    self.lock_numeric_history()?.request_float()?;
                }
                Ok((generation, None))
            }
        }
    }

    #[cold]
    pub(in crate::tagged::gc) fn claim_bignum(
        &self,
        owner: MutatorKey,
        requested_generation: Option<u64>,
    ) -> Result<(u64, Option<BignumRun>), RegistrationError> {
        self.admit_slice(owner)?;
        let mut gate = self.lock()?;
        let state = gate.as_mut().ok_or(RegistrationError::Publication)?;
        let next_active = state
            .active_regions
            .checked_add(1)
            .ok_or(RegistrationError::Capacity)?;
        let (generation, claim) = match state.numeric_epoch {
            NumericEpoch::Unprepared => {
                // plan issues an epoch for every successful install, including
                // no-pool N1 transfer. The state gate keeps it stable here.
                (self.lock_numeric_history()?.next_generation - 1, None)
            }
            NumericEpoch::Frozen { generation, .. } => {
                (generation, state.numeric.claim_bignum(owner, generation)?)
            }
        };
        match claim {
            Some(run) => {
                state.active_regions = next_active;
                Ok((generation, Some(run)))
            }
            None => {
                if requested_generation != Some(generation) {
                    self.lock_numeric_history()?.request_bignum()?;
                }
                Ok((generation, None))
            }
        }
    }

    #[cold]
    pub(in crate::tagged::gc) fn settle_float(
        &self,
        owner: MutatorKey,
        run: FloatRun,
    ) -> Result<(), (RegistrationError, FloatRun)> {
        if owner.heap_identity() != self.heap {
            return Err((RegistrationError::WrongHeap, run));
        }
        let mut gate = match self.lock() {
            Ok(gate) => gate,
            Err(error) => return Err((error, run)),
        };
        let Some(state) = gate.as_mut() else {
            return Err((RegistrationError::Publication, run));
        };
        let Some(next_active) = state.active_regions.checked_sub(1) else {
            return Err((RegistrationError::Publication, run));
        };
        // A claim uses at most one fixed-size page. This is sizing policy,
        // already capped by the recipe; exact allocation counters live on the
        // owner. Closing an owned run cannot fail for policy capacity.
        let next_interval = state
            .numeric_interval
            .floats
            .saturating_add(run.used() as u64);
        state.numeric.settle_float(owner, run)?;
        state.active_regions = next_active;
        state.numeric_interval.floats = next_interval;
        Ok(())
    }

    #[cold]
    pub(in crate::tagged::gc) fn settle_bignum(
        &self,
        owner: MutatorKey,
        run: BignumRun,
    ) -> Result<(), (RegistrationError, BignumRun)> {
        if owner.heap_identity() != self.heap {
            return Err((RegistrationError::WrongHeap, run));
        }
        let mut gate = match self.lock() {
            Ok(gate) => gate,
            Err(error) => return Err((error, run)),
        };
        let Some(state) = gate.as_mut() else {
            return Err((RegistrationError::Publication, run));
        };
        let Some(next_active) = state.active_regions.checked_sub(1) else {
            return Err((RegistrationError::Publication, run));
        };
        // A claim uses at most one fixed-size page. This is sizing policy,
        // already capped by the recipe; exact allocation counters live on the
        // owner. Closing an owned run cannot fail for policy capacity.
        let next_interval = state
            .numeric_interval
            .bignums
            .saturating_add(run.used() as u64);
        state.numeric.settle_bignum(owner, run)?;
        state.active_regions = next_active;
        state.numeric_interval.bignums = next_interval;
        Ok(())
    }
}

/// Copied cold observations only. No returned field can name a descriptor,
/// owner, allocation, run, or permission to restart a poisoned era.
#[cfg(any(test, feature = "test-support"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) struct NumericCatalogSnapshot {
    pub installed: bool,
    pub generation: u64,
    pub pending_float_requests: u64,
    pub pending_bignum_requests: u64,
    pub consumed_float_requests: u64,
    pub consumed_bignum_requests: u64,
    pub current_float_slots: u64,
    pub current_bignum_slots: u64,
    pub previous_float_slots: u64,
    pub previous_bignum_slots: u64,
    pub active_regions: usize,
}

#[cfg(any(test, feature = "test-support"))]
impl ConsCatalog {
    /// Test-only terminal observer. Raw mutex access deliberately bypasses
    /// check_live so poison-retention rows can inspect scalar requests and the
    /// committed receipt. Preserve the same state-gate -> history lock order.
    #[cold]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(in crate::tagged::gc) fn numeric_catalog_snapshot_for_test(
        &self,
    ) -> NumericCatalogSnapshot {
        let gate = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let history = self
            .numeric_history
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let (generation, consumed_float_requests, consumed_bignum_requests) =
            match gate.as_ref().map(|state| state.numeric_epoch) {
                Some(NumericEpoch::Frozen {
                    generation,
                    float_requests,
                    bignum_requests,
                }) => (generation, float_requests, bignum_requests),
                _ => (0, 0, 0),
            };
        let current = gate
            .as_ref()
            .map_or(NumericInterval::default(), |state| state.numeric_interval);
        NumericCatalogSnapshot {
            installed: gate.is_some(),
            generation,
            pending_float_requests: history.float_requests,
            pending_bignum_requests: history.bignum_requests,
            consumed_float_requests,
            consumed_bignum_requests,
            current_float_slots: current.floats,
            current_bignum_slots: current.bignums,
            previous_float_slots: history.previous.floats,
            previous_bignum_slots: history.previous.bignums,
            active_regions: gate.as_ref().map_or(0, |state| state.active_regions),
        }
    }
}

// The test owns this one-shot observer through its actual catalog. No callback
// field or new TLS exists in shipping code.
#[cfg(any(test, feature = "test-support"))]
impl ConsCatalog {
    /// The observer runs with catalog/history locks held. It must not access
    /// those locks; a native test actor may separately change registry state.
    pub(in crate::tagged::gc) fn observe_next_numeric_commit_for_test(
        &self,
        observer: Box<dyn FnOnce() + Send>,
    ) -> Result<(), RegistrationError> {
        let _gate = self.lock()?;
        let mut history = self.lock_numeric_history()?;
        assert!(
            history.commit_observer.is_none(),
            "one numeric commit observer at a time"
        );
        history.commit_observer = Some(observer);
        Ok(())
    }
}
