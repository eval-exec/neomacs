//! Confined numeric run books and rooted allocation for admitted owners.
//! Empty class pools refuse into the existing stopped refill protocol.

use super::super::MemoryUseCountSlot;
use super::super::cons_storage::{BignumRun, FloatRun, InitializedNumeric};
use super::*;
use crate::tagged::header::{BignumObj, FloatObj};
use crate::tagged::transport::{RootedNumericDestination, RootedSlot};
use malachite::integer::Integer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericClass {
    Float,
    Bignum,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum NumericAllocError {
    #[error(transparent)]
    Registration(#[from] RegistrationError),
    #[error("numeric class pool exhausted: {0:?}")]
    Exhausted(NumericClass),
}

enum RunBook<R> {
    Closed,
    Open { generation: u64, run: R },
}

/// An owner contributes at most one pending request per class and catalog
/// generation. This scalar fact carries no run, slot reservation, or owner ID
/// into the unkeyed pool. Closing/reclaiming a run does not erase the request.
#[derive(Clone, Copy)]
enum RequestState {
    Idle,
    Requested { generation: u64 },
}

impl RequestState {
    fn generation(&self) -> Option<u64> {
        match *self {
            Self::Idle => None,
            Self::Requested { generation } => Some(generation),
        }
    }

    fn discard_stale(&mut self, generation: u64) {
        if self.generation().is_some_and(|old| old != generation) {
            *self = Self::Idle;
        }
    }
}

/// Confined run ownership and scalar request facts; the cold pool stays unkeyed.
pub(super) struct NumericOwnerBook {
    floats: RunBook<FloatRun>,
    bignums: RunBook<BignumRun>,
    float_request: RequestState,
    bignum_request: RequestState,
}

impl NumericOwnerBook {
    pub(super) fn new() -> Self {
        Self {
            floats: RunBook::Closed,
            bignums: RunBook::Closed,
            float_request: RequestState::Idle,
            bignum_request: RequestState::Idle,
        }
    }

    pub(super) fn is_closed(&self) -> bool {
        matches!(self.floats, RunBook::Closed) && matches!(self.bignums, RunBook::Closed)
    }

    /// Refreeze installs a fresh catalog generation, logically invalidating
    /// both old requests at once. Physical clearing is lazy on the first cold
    /// class claim, using the generation returned under the existing gate.
    /// A native round trip without refreeze preserves both pending facts.
    fn observe_generation(&mut self, generation: u64) {
        self.float_request.discard_stale(generation);
        self.bignum_request.discard_stale(generation);
    }
}

impl RunAccess<'_, '_> {
    /// Install an initialized numeric result in a validated canonical home.
    /// Refusal leaves the home and allocation counters unchanged.
    #[inline]
    pub fn float_into<R: RootedSlot>(
        &mut self,
        number: f64,
        result: &mut R,
    ) -> Result<(), NumericAllocError> {
        crate::tagged::transport::float_into(self, number, result)
    }

    #[inline]
    pub fn bignum_into<R: RootedSlot>(
        &mut self,
        number: Integer,
        result: &mut R,
    ) -> Result<(), NumericAllocError> {
        crate::tagged::transport::bignum_into(self, number, result)
    }

    #[inline]
    pub(crate) fn allocate_float(
        &mut self,
        destination: &RootedNumericDestination<'_>,
        number: f64,
    ) -> Result<InitializedNumeric<'_>, NumericAllocError> {
        self.validate_numeric_destination(destination)?;
        let (objects, bytes) = self.numeric_charge(size_of::<FloatObj>())?;
        self.ensure_float_run()?;
        let owner = self.owner.entry.key;
        let local = self.owner.local.as_mut().expect("resident owner state");
        let RunBook::Open { generation, run } = &mut local.numeric.floats else {
            unreachable!("successful numeric grant")
        };
        run.validate(owner, *generation)?;
        let slot = run.next_slot().expect("unexhausted numeric grant");
        let initialized = slot.initialize(number);
        // All checks and capacity decisions preceded the full object write.
        // These infallible stores precede the immediate sealed root install.
        local.hot.allocated_count = objects;
        local.hot.bytes_since_gc = bytes;
        let index = MemoryUseCountSlot::Floats.index();
        local.hot.memory_use_counts[index] = local.hot.memory_use_counts[index].wrapping_add(1);
        Ok(initialized)
    }

    #[inline]
    pub(crate) fn allocate_bignum(
        &mut self,
        destination: &RootedNumericDestination<'_>,
        number: Integer,
    ) -> Result<InitializedNumeric<'_>, NumericAllocError> {
        self.validate_numeric_destination(destination)?;
        let (objects, bytes) = self.numeric_charge(size_of::<BignumObj>())?;
        self.ensure_bignum_run()?;
        let owner = self.owner.entry.key;
        let local = self.owner.local.as_mut().expect("resident owner state");
        let RunBook::Open { generation, run } = &mut local.numeric.bignums else {
            unreachable!("successful numeric grant")
        };
        run.validate(owner, *generation)?;
        let slot = run.next_slot().expect("unexhausted numeric grant");
        let initialized = slot.initialize(number);
        local.hot.allocated_count = objects;
        local.hot.bytes_since_gc = bytes;
        // Like the serial bignum path, no memory-use-counts slot is charged.
        Ok(initialized)
    }

    fn validate_numeric_destination(
        &mut self,
        destination: &RootedNumericDestination<'_>,
    ) -> Result<(), RegistrationError> {
        if destination.heap_identity() != self.heap_identity() {
            return Err(RegistrationError::WrongHeap);
        }
        self.cons_state().check_boundary()
    }

    fn numeric_charge(&self, bytes: usize) -> Result<(usize, usize), RegistrationError> {
        let hot = &self.owner.local.as_ref().expect("resident owner state").hot;
        let objects = hot
            .allocated_count
            .checked_add(1)
            .ok_or(RegistrationError::Capacity)?;
        let bytes = hot
            .bytes_since_gc
            .checked_add(bytes)
            .ok_or(RegistrationError::Capacity)?;
        Ok((objects, bytes))
    }

    fn ensure_float_run(&mut self) -> Result<(), NumericAllocError> {
        let ready = matches!(
            &self.owner.local.as_ref().expect("resident owner state").numeric.floats,
            RunBook::Open { run, .. } if !run.empty()
        );
        if !ready {
            self.close_float_run()?;
            let requested_generation = self
                .owner
                .local
                .as_ref()
                .expect("resident owner state")
                .numeric
                .float_request
                .generation();
            let (generation, run) = self
                .owner
                .registry
                .cons_catalog
                .claim_float(self.owner.entry.key, requested_generation)?;
            let numeric = &mut self
                .owner
                .local
                .as_mut()
                .expect("resident owner state")
                .numeric;
            numeric.observe_generation(generation);
            let Some(run) = run else {
                numeric.float_request = RequestState::Requested { generation };
                return Err(NumericAllocError::Exhausted(NumericClass::Float));
            };
            // A later successful same-generation claim cannot cancel a
            // request already counted for the next matching stopped mint.
            numeric.floats = RunBook::Open { generation, run };
        }
        Ok(())
    }

    fn ensure_bignum_run(&mut self) -> Result<(), NumericAllocError> {
        let ready = matches!(
            &self.owner.local.as_ref().expect("resident owner state").numeric.bignums,
            RunBook::Open { run, .. } if !run.empty()
        );
        if !ready {
            self.close_bignum_run()?;
            let requested_generation = self
                .owner
                .local
                .as_ref()
                .expect("resident owner state")
                .numeric
                .bignum_request
                .generation();
            let (generation, run) = self
                .owner
                .registry
                .cons_catalog
                .claim_bignum(self.owner.entry.key, requested_generation)?;
            let numeric = &mut self
                .owner
                .local
                .as_mut()
                .expect("resident owner state")
                .numeric;
            numeric.observe_generation(generation);
            let Some(run) = run else {
                numeric.bignum_request = RequestState::Requested { generation };
                return Err(NumericAllocError::Exhausted(NumericClass::Bignum));
            };
            // A later successful same-generation claim cannot cancel a
            // request already counted for the next matching stopped mint.
            numeric.bignums = RunBook::Open { generation, run };
        }
        Ok(())
    }

    /// Settle every class before releasing the run or publishing an image.
    /// A failed settlement restores its owned run for retry or retention.
    #[cold]
    #[inline(never)]
    pub fn close_numeric_runs(&mut self) -> Result<(), RegistrationError> {
        self.cons_state().check_boundary()?;
        self.close_float_run()?;
        self.close_bignum_run()
    }

    /// End allocation authority before every publication boundary. The fixed
    /// order matches the VM driver's stopped refill protocol.
    #[cold]
    #[inline(never)]
    pub fn close_all_runs(&mut self) -> Result<(), RegistrationError> {
        self.close_cons_region()?;
        self.close_numeric_runs()
    }

    fn close_float_run(&mut self) -> Result<(), RegistrationError> {
        let book = &mut self
            .owner
            .local
            .as_mut()
            .expect("resident owner state")
            .numeric
            .floats;
        let RunBook::Open { generation, run } = std::mem::replace(book, RunBook::Closed) else {
            return Ok(());
        };
        if let Err((error, run)) = self
            .owner
            .registry
            .cons_catalog
            .settle_float(self.owner.entry.key, run)
        {
            *book = RunBook::Open { generation, run };
            return Err(error);
        }
        Ok(())
    }

    fn close_bignum_run(&mut self) -> Result<(), RegistrationError> {
        let book = &mut self
            .owner
            .local
            .as_mut()
            .expect("resident owner state")
            .numeric
            .bignums;
        let RunBook::Open { generation, run } = std::mem::replace(book, RunBook::Closed) else {
            return Ok(());
        };
        if let Err((error, run)) = self
            .owner
            .registry
            .cons_catalog
            .settle_bignum(self.owner.entry.key, run)
        {
            *book = RunBook::Open { generation, run };
            return Err(error);
        }
        Ok(())
    }
}

static_assertions::assert_not_impl_any!(NumericOwnerBook: Send, Sync, Clone, Copy);
