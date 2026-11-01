//! Closed singleton mapping over fresh owner-rooted inputs and scalar callbacks.
//! No arbitrary list graph, mutable cell address or native entry crosses this seam.
use super::super::symbol_authority::{
    PreparedHofBuiltin, PreparedUnaryFunction, check_symbol_home as check_home,
};
use super::*;
use crate::tagged::gc::function_write_hooks::HofOperation;
use std::num::NonZeroU32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HofError {
    Authority(RegistrationError),
    Function(FunctionError),
    AliasedOperand,
    ConsumedInput,
    UnexpectedCallbackState,
}
impl From<RegistrationError> for HofError {
    fn from(error: RegistrationError) -> Self {
        Self::Authority(error)
    }
}
impl From<FunctionError> for HofError {
    fn from(error: FunctionError) -> Self {
        Self::Function(error)
    }
}

#[derive(Clone, Copy)]
enum SingletonState {
    Ready,
    Consumed,
}
/// A freshly allocated singleton with a checked immediate element. Its homes
/// are private until its one successful mapping consumes the capability.
/// This closed executor never traverses an arbitrary shared
/// list or accepts a mutable graph as an inline callback source. These homes
/// remain resident until the owner's normal root-frame cleanup.
#[must_use = "retain the singleton's matching owner homes until mapping returns"]
pub(crate) struct HofSingleton<'era> {
    state: SingletonState,
    list: OwnerRootSlot<'era>,
    argument: OwnerRootSlot<'era>,
    tail: OwnerRootSlot<'era>,
}
impl<'era> HofSingleton<'era> {
    pub(crate) fn new(
        run: &mut RunAccess<'_, 'era>,
        element: ImmediateResult,
    ) -> Result<Self, RegistrationError> {
        let argument = run.private_nil_root()?;
        let tail = run.private_nil_root()?;
        let mut list = run.private_nil_root()?;
        store(run, &argument, TaggedValue::from_bits(element.word()))?;
        run.cons_into(&argument, &tail, &mut list)?;
        Ok(Self {
            state: SingletonState::Ready,
            list,
            argument,
            tail,
        })
    }

    #[cfg(test)]
    pub(crate) fn word_for_test(&self, run: &RunAccess<'_, '_>) -> usize {
        self.list.word_for_test(run)
    }
}

/// Independent private homes retain the selected definition and temporary
/// answer. A failed allocation cannot overwrite the caller's result home.
#[must_use = "retain the scratch homes for the matching owner and generation"]
pub(crate) struct HofScratch<'era> {
    selected: FunctionActivation<'era>,
    answer: OwnerRootSlot<'era>,
    temporary: OwnerRootSlot<'era>,
}
impl<'era> HofScratch<'era> {
    pub(crate) fn new(run: &mut RunAccess<'_, 'era>) -> Result<Self, HofError> {
        Ok(Self {
            selected: FunctionActivation::new(run)?,
            answer: run.private_nil_root()?,
            temporary: run.private_nil_root()?,
        })
    }
}

fn load(
    run: &RunAccess<'_, '_>,
    home: &OwnerRootSlot<'_>,
) -> Result<TaggedValue, RegistrationError> {
    check_home(run, home)?;
    run.owner
        .local
        .as_ref()
        .and_then(|local| local.root_scratch.get(home.index))
        .map(|word| word.value())
        .ok_or(RegistrationError::StaleRegistration)
}
fn store(
    run: &mut RunAccess<'_, '_>,
    home: &OwnerRootSlot<'_>,
    value: TaggedValue,
) -> Result<(), RegistrationError> {
    check_home(run, home)?;
    let destination = run
        .owner
        .local
        .as_mut()
        .and_then(|local| local.root_scratch.get_mut(home.index))
        .ok_or(RegistrationError::StaleRegistration)?;
    // Stop-all is the only collector in an era. Both source and destination
    // remain resident owner roots; no managed-container barrier is bypassed.
    *destination = MarkWord::of(value);
    Ok(())
}

impl<'era> ExecutionActivation<'_, '_, 'era> {
    /// Native descriptor borrows end before this sealed semantic leaf. Exactly
    /// one callback executes, using the same mapper/callback reader and private
    /// activation root as the qualified guard. No lease survives allocation.
    pub(crate) fn map_singleton(
        &mut self,
        mapper: &PreparedHofBuiltin,
        callback: &PreparedUnaryFunction,
        expected: &OwnerRootSlot<'era>,
        input: &mut HofSingleton<'era>,
        scratch: &mut HofScratch<'era>,
    ) -> Result<FunctionSelection, HofError> {
        for home in [
            expected,
            &input.list,
            &input.argument,
            &input.tail,
            &scratch.answer,
            &scratch.temporary,
        ] {
            check_home(self.run, home)?;
            if home.index == self.result.index {
                return Err(HofError::AliasedOperand);
            }
        }
        match input.state {
            SingletonState::Ready => {}
            SingletonState::Consumed => return Err(HofError::ConsumedInput),
        }
        let selection = self.run.sym_hof_call_begin(
            mapper,
            callback,
            expected,
            &input.argument,
            &mut scratch.selected,
        )?;
        match selection {
            FunctionSelection::Changed => return Ok(FunctionSelection::Changed),
            FunctionSelection::Selected => {}
        }
        let execution: Result<TaggedValue, HofError> = (|| {
            // Two finite program instructions, never a contention budget.
            match self.run.sym_call_step(
                &mut scratch.selected,
                &mut scratch.answer,
                NonZeroU32::MIN,
            )? {
                FunctionStep::Yielded => {}
                FunctionStep::Returned => return Err(HofError::UnexpectedCallbackState),
            }
            match self.run.sym_call_step(
                &mut scratch.selected,
                &mut scratch.answer,
                NonZeroU32::MIN,
            )? {
                FunctionStep::Returned => {}
                FunctionStep::Yielded => return Err(HofError::UnexpectedCallbackState),
            }
            match mapper.operation() {
                HofOperation::Mapc => Ok(load(self.run, &input.list)?),
                HofOperation::Mapcar => {
                    self.run
                        .cons_into(&scratch.answer, &input.tail, &mut scratch.temporary)?;
                    Ok(load(self.run, &scratch.temporary)?)
                }
            }
        })();
        // Cancel on EVERY selected-path outcome, including callback refusal or
        // exhausted allocation credits. No pending callback escapes this entry.
        let cleanup = scratch.selected.clear(self.run).map_err(HofError::Function);
        let result = execution?;
        cleanup?;
        // Resolve the destination before the non-fallible commit. Publishing
        // mapc's original list permits aliases to mutate it afterward; this
        // one-shot input can never be reused to trust its captured element.
        check_home(self.run, &self.result)?;
        let destination = self
            .run
            .owner
            .local
            .as_mut()
            .and_then(|local| local.root_scratch.get_mut(self.result.index))
            .ok_or(RegistrationError::StaleRegistration)?;
        input.state = SingletonState::Consumed;
        *destination = MarkWord::of(result);
        Ok(FunctionSelection::Selected)
    }
}
static_assertions::assert_not_impl_any!(HofSingleton<'static>: Clone, Copy);
static_assertions::assert_not_impl_any!(HofScratch<'static>: Clone, Copy);
