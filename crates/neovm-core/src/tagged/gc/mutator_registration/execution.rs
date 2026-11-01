//! Production-only bounded execution admission and resident result homes.
//!
//! This value-layer boundary names no compiler or evaluator. The production
//! wrapper alone supplies its sealed GEN1 construction witness. A callback
//! borrows its activation, so it cannot forget an owned guard or release the
//! enclosing IN_SLICE admission before all borrowed table addresses are dead.
mod hof;
mod read;
use super::construction::Gen1Construction;
use super::*;
use crate::tagged::gc::mark_word::MarkWord;
use crate::tagged::gc::symbol_table::publication::{PublicationView, TableView};
use crate::tagged::value::{Fixnum, TaggedValue};
pub(crate) use hof::{HofError, HofScratch, HofSingleton};
pub(crate) use read::{CellRead, CellReadError};

/// A checked immediate, never a managed pointer smuggled through native ABI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ImmediateResult {
    Nil,
    True,
    Fixnum(Fixnum),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ImmediateResultError {
    NotImmediate,
}
impl ImmediateResult {
    pub(crate) fn from_word(bits: usize) -> Result<Self, ImmediateResultError> {
        let value = TaggedValue::from_bits(bits);
        if value.is_nil() {
            Ok(Self::Nil)
        } else if value.is_t() {
            Ok(Self::True)
        } else if let Some(fixnum) = value.as_fixnum_value() {
            Ok(Self::Fixnum(fixnum))
        } else {
            Err(ImmediateResultError::NotImmediate)
        }
    }
    pub(crate) fn word(self) -> usize {
        match self {
            Self::Nil => TaggedValue::NIL.bits(),
            Self::True => TaggedValue::T.bits(),
            Self::Fixnum(fixnum) => TaggedValue::from_fixnum(fixnum).bits(),
        }
    }
}

/// A nominal production witness, borrowing this exact era's registry.
/// No proof wrapper or integer identity can construct it.
#[must_use = "retain production admission while bounded executions use it"]
pub struct ParallelExecutionAdmission<'era> {
    registry: &'era Registry,
    _era: PhantomData<fn(&'era ()) -> &'era ()>,
}

/// Only borrowed by the closed activation callback. The exclusive RunAccess
/// keeps its original IN_SLICE admission until the callback and native stack
/// have returned; neither root-bank mutation nor native handoff is exposed.
#[must_use = "the wrapper retains the activation until its borrowed callback returns"]
pub(crate) struct ExecutionActivation<'activation, 'owner, 'era> {
    registry: &'era Registry,
    run: &'activation mut RunAccess<'owner, 'era>,
    result: OwnerRootSlot<'era>,
}

impl<K: EraKeeper> EraCore<'_, K> {
    pub(super) fn execution_admission(
        &self,
        _production: &Gen1Construction,
    ) -> ParallelExecutionAdmission<'_> {
        ParallelExecutionAdmission {
            registry: self.registry(),
            _era: PhantomData,
        }
    }
}

impl<'era> ParallelExecutionAdmission<'era> {
    /// Validate exact registry/owner/home identity before entering a bounded
    /// activation. Refusal neither enters the body nor changes its result.
    pub(crate) fn with_activation<'owner, R>(
        &self,
        run: &mut RunAccess<'owner, 'era>,
        result: &OwnerRootSlot<'era>,
        body: impl FnOnce(&mut ExecutionActivation<'_, 'owner, 'era>) -> R,
    ) -> Result<R, RegistrationError> {
        run.owner.check_thread()?;
        if self.registry.heap != run.owner.registry.heap {
            return Err(RegistrationError::WrongHeap);
        }
        if !std::ptr::eq(self.registry, run.owner.registry) {
            return Err(RegistrationError::StaleRegistration);
        }
        if run.symbol_loan {
            return Err(RegistrationError::OutstandingLoan);
        }
        self.registry.symbols.execution_ready()?;
        let local = run
            .owner
            .local
            .as_ref()
            .ok_or(RegistrationError::StaleRegistration)?;
        result.check_binding(
            run.owner.entry.key,
            local.root_slot_generation,
            local.root_scratch.len(),
        )?;
        let mut activation = ExecutionActivation {
            registry: self.registry,
            run,
            result: result.clone(),
        };
        Ok(body(&mut activation))
    }
}

impl ExecutionActivation<'_, '_, '_> {
    /// This shared immutable view performs N3's Acquire publication load.
    /// Its borrow cannot escape the activation callback's return type.
    pub(crate) fn table_view(&self) -> TableView<'_> {
        self.registry.symbols.tables.view()
    }

    /// The stack-confined consumer loads one immutable descriptor from this
    /// slot, never from the mutable backing spine. IN_SLICE covers all uses.
    pub(crate) fn publication(&self) -> PublicationView<'_> {
        self.registry.symbols.tables.publication()
    }

    /// Commit a validated immediate to the canonical resident result home.
    /// Cell-derived values need their separate sealed root-transfer methods;
    /// a pointer-shaped machine result cannot enter through this interface.
    pub(crate) fn store_immediate(
        &mut self,
        result: ImmediateResult,
    ) -> Result<(), RegistrationError> {
        self.run.owner.check_thread()?;
        let destination = self
            .run
            .owner
            .local
            .as_mut()
            .and_then(|local| local.root_scratch.get_mut(self.result.index))
            .ok_or(RegistrationError::StaleRegistration)?;
        *destination = MarkWord::of(TaggedValue::from_bits(result.word()));
        Ok(())
    }
}

static_assertions::assert_impl_all!(ParallelExecutionAdmission<'static>: Send, Sync);
static_assertions::assert_not_impl_any!(ParallelExecutionAdmission<'static>: Clone, Copy);
static_assertions::assert_not_impl_any!(ExecutionActivation<'static, 'static, 'static>: Send, Sync, Clone, Copy);
static_assertions::assert_impl_all!(ImmediateResult: Send, Sync, Copy);

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use tests::{
    CohortPhase, CohortSize, ExecutionTestDeadline, ExecutionTestHomes, ExecutionTestRetry,
    FixtureReceipt as ExecutionFixtureReceipt, FixtureSeed as ExecutionTestSeed, HofTestInputs,
    PublicationFacts, PublicationInputs, execution_test_retry,
    prepared_cell_fixture as with_test_prepared_cell_execution,
    prepared_fixture as with_test_prepared_execution,
    prepared_hof_cohort as with_test_prepared_hof_cohort,
    prepared_hof_fixture as with_test_prepared_hof_execution,
    prepared_publication_cohort as with_test_prepared_publication_cohort,
};
