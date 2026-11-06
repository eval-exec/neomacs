//! Production-only bounded execution admission and resident result homes.
//!
//! This value-layer boundary names no compiler or evaluator. The production
//! wrapper alone supplies its sealed GEN1 construction witness. A callback
//! borrows its activation, so it cannot forget an owned guard or release the
//! enclosing IN_SLICE admission before all borrowed table addresses are dead.
mod hof;
#[cfg(not(feature = "test-support"))]
mod read;
#[cfg(feature = "test-support")]
pub mod read;
#[cfg(all(any(test, feature = "test-support"), not(feature = "test-support")))]
mod test_support_data;
#[cfg(feature = "test-support")]
pub mod test_support_data;
use super::construction::Gen1Construction;
use super::*;
use crate::tagged::gc::mark_word::MarkWord;
use crate::tagged::gc::symbol_table::publication::{PublicationView, TableView};
use crate::tagged::value::{Fixnum, TaggedValue};
#[doc(hidden)]
pub use hof::{HofError, HofScratch, HofSingleton};
#[doc(hidden)]
pub use read::{CellRead, CellReadError};

/// A checked immediate, never a managed pointer smuggled through native ABI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum ImmediateResult {
    Nil,
    True,
    Fixnum(Fixnum),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum ImmediateResultError {
    NotImmediate,
}
impl ImmediateResult {
    #[doc(hidden)]
    pub fn from_word(bits: usize) -> Result<Self, ImmediateResultError> {
        let value = unsafe {
            /* SAFETY: value-internal construction from a real Value */
            TaggedValue::from_bits(bits)
        };
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
    #[doc(hidden)]
    pub fn word(self) -> usize {
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
#[doc(hidden)]
pub struct ExecutionActivation<'activation, 'owner, 'era> {
    registry: &'era Registry,
    run: &'activation mut RunAccess<'owner, 'era>,
    result: OwnerRootSlot<'era>,
}

pub(crate) trait EraCoreExecutionExt {
    fn execution_admission(&self, _production: &Gen1Construction)
    -> ParallelExecutionAdmission<'_>;
}

impl<K: EraKeeper> EraCoreExecutionExt for EraCore<'_, K> {
    fn execution_admission(
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
    #[doc(hidden)]
    pub fn with_activation<'owner, R>(
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
    #[doc(hidden)]
    pub fn publication(&self) -> PublicationView<'_> {
        self.registry.symbols.tables.publication()
    }

    /// Commit a validated immediate to the canonical resident result home.
    /// Cell-derived values need their separate sealed root-transfer methods;
    /// a pointer-shaped machine result cannot enter through this interface.
    #[doc(hidden)]
    pub fn store_immediate(&mut self, result: ImmediateResult) -> Result<(), RegistrationError> {
        self.run.owner.check_thread()?;
        let destination = self
            .run
            .owner
            .local
            .as_mut()
            .and_then(|local| local.root_scratch.get_mut(self.result.index))
            .ok_or(RegistrationError::StaleRegistration)?;
        *destination = MarkWord::of(unsafe {
            /* SAFETY: value-internal construction from a real Value */
            TaggedValue::from_bits(result.word())
        });
        Ok(())
    }
}

static_assertions::assert_impl_all!(ParallelExecutionAdmission<'static>: Send, Sync);
static_assertions::assert_not_impl_any!(ParallelExecutionAdmission<'static>: Clone, Copy);
static_assertions::assert_not_impl_any!(ExecutionActivation<'static, 'static, 'static>: Send, Sync, Clone, Copy);
static_assertions::assert_impl_all!(ImmediateResult: Send, Sync, Copy);

// Slim parent for the restored cohort/hof fixture files (LEAD 21:5xZ). The
// execution fixture helper and its tests now live only in core's re-homed
// execution/tests.rs; this module keeps the non-duplicated cohort/hof
// fixtures, which remain the single home their eval consumers will use.
#[cfg(test)]
mod tests;
