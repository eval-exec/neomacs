//! Rooted, finite constant-lambda calls; no legacy evaluator or native entry.
use super::*;
use std::num::NonZeroU32;

/// The payload home is reserved privately, so no operand alias can replace a
/// cached definition while its symbol/clock metadata still describes it.
pub struct FunctionCallCache<'era> {
    home: OwnerRootSlot<'era>,
    symbol: Option<SymbolHandle>,
    observed_epoch: Option<u64>,
    #[cfg(test)]
    counts: (usize, usize, usize), // successful refills, resyncs, equal hits
}
impl<'era> FunctionCallCache<'era> {
    pub fn new(run: &mut RunAccess<'_, 'era>) -> Result<Self, FunctionError> {
        check_run(run)?;
        Ok(Self {
            home: run.private_nil_root()?,
            symbol: None,
            observed_epoch: None,
            #[cfg(test)]
            counts: (0, 0, 0),
        })
    }
    pub fn observed_epoch(&self) -> Option<u64> {
        self.observed_epoch
    }

    /// Release the cached definition without clearing the caller's root frame.
    pub fn clear(&mut self, run: &mut RunAccess<'_, 'era>) -> Result<(), FunctionError> {
        check_run(run)?;
        globals::check_home(run, &self.home)?;
        let dst = run
            .owner
            .local
            .as_mut()
            .and_then(|local| local.root_scratch.get_mut(self.home.index))
            .ok_or(RegistrationError::StaleRegistration)?;
        *dst = MarkWord::of(Value::NIL);
        self.symbol = None;
        self.observed_epoch = None;
        Ok(())
    }
    #[cfg(test)]
    pub(crate) fn counts_for_test(&self) -> (usize, usize, usize) {
        self.counts
    }
}

#[derive(Clone, Copy)]
enum Instruction {
    Validate,
    Return,
}
#[derive(Clone, Copy)]
enum ActivationState {
    Empty,
    Selected {
        epoch: u64,
        lambda: SymId,
        shape: CallShape,
        instruction: Instruction,
    },
    Returned {
        epoch: u64,
    },
}

/// Its private home is the selected definition's lifetime authority. A later
/// fset/unbind cannot redirect this activation. Every resume checks the home.
pub struct FunctionActivation<'era> {
    home: OwnerRootSlot<'era>,
    state: ActivationState,
}
impl<'era> FunctionActivation<'era> {
    pub fn new(run: &mut RunAccess<'_, 'era>) -> Result<Self, FunctionError> {
        check_run(run)?;
        Ok(Self {
            home: run.private_nil_root()?,
            state: ActivationState::Empty,
        })
    }
    pub fn selected_epoch(&self) -> Option<u64> {
        match self.state {
            ActivationState::Empty => None,
            ActivationState::Selected { epoch, .. } | ActivationState::Returned { epoch } => {
                Some(epoch)
            }
        }
    }
    /// Explicit cancellation/reuse; no hidden definition remains a root.
    pub fn clear(&mut self, run: &mut RunAccess<'_, 'era>) -> Result<(), FunctionError> {
        check_run(run)?;
        globals::check_home(run, &self.home)?;
        let dst = run
            .owner
            .local
            .as_mut()
            .and_then(|local| local.root_scratch.get_mut(self.home.index))
            .ok_or(RegistrationError::StaleRegistration)?;
        *dst = MarkWord::of(Value::NIL);
        self.state = ActivationState::Empty;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FunctionStep {
    Yielded,
    Returned,
}

/// A named guard either rooted its exact supported target or changed nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FunctionSelection {
    Selected,
    Changed,
}

fn check_run(run: &RunAccess<'_, '_>) -> Result<(), RegistrationError> {
    run.owner.check_thread()?;
    if run.owner.registry.symbols.poisoned.load(Ordering::Acquire) {
        return Err(RegistrationError::Poisoned);
    }
    if run.symbol_loan {
        return Err(RegistrationError::OutstandingLoan);
    }
    Ok(())
}

impl<'era> RunAccess<'_, 'era> {
    /// Select only the supported constant-lambda whose identity is expected.
    /// The private activation home retains a successful selection even after
    /// redefinition; every subsequent call performs a fresh guarded selection.
    pub fn sym_named_call_begin(
        &mut self,
        symbol: &SymbolHandle,
        expected: &OwnerRootSlot<'era>,
        activation: &mut FunctionActivation<'era>,
    ) -> Result<FunctionSelection, FunctionError> {
        self.named_call_begin_with_shape(
            symbol,
            expected,
            activation,
            CallShape::Nullary(ReturnSubset::Scalar),
        )
    }

    pub(super) fn named_call_begin_with_shape(
        &mut self,
        symbol: &SymbolHandle,
        expected: &OwnerRootSlot<'era>,
        activation: &mut FunctionActivation<'era>,
        shape: CallShape,
    ) -> Result<FunctionSelection, FunctionError> {
        self.check_named_operands(expected, activation)?;
        self.symbol_ref(symbol)?
            .select_named(OwnerRootIndex::from(expected), activation, shape)
    }

    pub(super) fn check_named_operands(
        &self,
        expected: &OwnerRootSlot<'era>,
        activation: &FunctionActivation<'era>,
    ) -> Result<(), FunctionError> {
        check_run(self)?;
        globals::check_home(self, expected)?;
        globals::check_home(self, &activation.home)?;
        if expected.index == activation.home.index {
            return Err(RegistrationError::Publication.into());
        }
        if matches!(activation.state, ActivationState::Selected { .. }) {
            return Err(FunctionError::ActiveActivation);
        }
        Ok(())
    }

    pub fn sym_call_begin(
        &mut self,
        symbol: &SymbolHandle,
        cache: &mut FunctionCallCache<'era>,
        activation: &mut FunctionActivation<'era>,
    ) -> Result<(), FunctionError> {
        self.call_begin_with_subset(symbol, cache, activation, ReturnSubset::Scalar)
    }

    pub(super) fn call_begin_with_subset(
        &mut self,
        symbol: &SymbolHandle,
        cache: &mut FunctionCallCache<'era>,
        activation: &mut FunctionActivation<'era>,
        subset: ReturnSubset,
    ) -> Result<(), FunctionError> {
        check_run(self)?;
        globals::check_home(self, &cache.home)?;
        globals::check_home(self, &activation.home)?;
        if cache.home.index == activation.home.index {
            return Err(RegistrationError::Publication.into());
        }
        if matches!(activation.state, ActivationState::Selected { .. }) {
            return Err(FunctionError::ActiveActivation);
        }
        self.symbol_ref(symbol)?
            .select_call(cache, activation, subset)
    }

    /// At most two finite instructions; one unit validates and yields, the
    /// next returns. No symbol lease, callback, allocation or poll survives a
    /// step. A returned activation cannot silently execute a second time.
    pub fn sym_call_step(
        &mut self,
        activation: &mut FunctionActivation<'era>,
        dst: &mut OwnerRootSlot<'era>,
        fuel: NonZeroU32,
    ) -> Result<FunctionStep, FunctionError> {
        check_run(self)?;
        globals::check_home(self, &activation.home)?;
        globals::check_home(self, dst)?;
        if dst.index == activation.home.index {
            return Err(RegistrationError::Publication.into());
        }
        let definition = self
            .owner
            .local
            .as_ref()
            .and_then(|local| local.root_scratch.get(activation.home.index))
            .ok_or(RegistrationError::StaleRegistration)?
            .value();
        for _ in 0..fuel.get() {
            match activation.state {
                ActivationState::Empty | ActivationState::Returned { .. } => {
                    return Err(FunctionError::InactiveActivation);
                }
                ActivationState::Selected {
                    epoch,
                    lambda,
                    shape,
                    instruction,
                } => {
                    let answer = constant::decode_shape(definition, lambda, shape)?;
                    match instruction {
                        Instruction::Validate => {
                            activation.state = ActivationState::Selected {
                                epoch,
                                lambda,
                                shape,
                                instruction: Instruction::Return,
                            }
                        }
                        Instruction::Return => {
                            let local = self
                                .owner
                                .local
                                .as_mut()
                                .ok_or(RegistrationError::StaleRegistration)?;
                            let [answer_home, selected_home] = local
                                .root_scratch
                                .get_disjoint_mut([dst.index, activation.home.index])
                                .map_err(|_| RegistrationError::StaleRegistration)?;
                            *answer_home = MarkWord::of(answer);
                            *selected_home = MarkWord::of(Value::NIL);
                            activation.state = ActivationState::Returned { epoch };
                            return Ok(FunctionStep::Returned);
                        }
                    }
                }
            }
        }
        Ok(FunctionStep::Yielded)
    }
    #[cfg(test)]
    pub(crate) fn set_symbol_poison_for_test(&mut self, poisoned: bool) {
        self.owner
            .registry
            .symbols
            .poisoned
            .store(poisoned, Ordering::Release);
    }
    #[cfg(test)]
    pub(crate) fn symbol_word_for_test(&self) -> u64 {
        self.owner.registry.symbols.word.load(Ordering::Acquire)
    }
}

impl SymbolRef<'_, '_, '_> {
    pub(super) fn select_named(
        &mut self,
        expected: OwnerRootIndex,
        activation: &mut FunctionActivation<'_>,
        shape: CallShape,
    ) -> Result<FunctionSelection, FunctionError> {
        let symbols = &self.run.owner.registry.symbols;
        // Match select_call's single-reader rule, without its cached branch:
        // the original clock, prepared parser metadata and fresh definition
        // belong to this one selection, never to a later version reload.
        let (clock, lambda, definition) = {
            let token = self.read_token();
            // SAFETY: this matching identity-checked reader retains the World
            // and excludes writers throughout the complete version snapshot.
            let (clock, _) = unsafe {
                symbols
                    .backing
                    .function_version(self.key.symbol, None, &token)
            }
            .ok_or(RegistrationError::Publication)?;
            if clock == u64::MAX {
                return Err(FunctionError::EpochExhausted);
            }
            // SAFETY: prepared records stay immutable under this same reader.
            let lambda = unsafe { symbols.functions.lambda_for(self.key.symbol, &token) };
            // SAFETY: the same live reader protects the complete function-cell
            // state and retained backing until the selected value is rooted.
            let definition = match unsafe { symbols.backing.read_function(self.key.symbol, &token) }
                .ok_or(RegistrationError::Publication)?
            {
                FunctionCellSnapshot::Bound(value) => value,
                FunctionCellSnapshot::Empty | FunctionCellSnapshot::ExplicitlyUnbound => {
                    return Err(FunctionError::VoidFunction);
                }
            };
            if definition.is_nil() {
                return Err(FunctionError::VoidFunction);
            }
            // Only admission-prepared metadata authorizes a supported shape.
            // Defer this refusal so an empty cell still reports VoidFunction.
            let lambda = lambda.ok_or(FunctionError::UnsupportedDefinition)?;
            constant::decode_shape(definition, lambda, shape)?;
            (clock, lambda, definition)
        };
        let local = self
            .run
            .owner
            .local
            .as_mut()
            .ok_or(RegistrationError::StaleRegistration)?;
        let expected_value = local
            .root_scratch
            .get(usize::from(expected))
            .ok_or(RegistrationError::StaleRegistration)?
            .value();
        if expected_value.bits() != definition.bits() {
            return Ok(FunctionSelection::Changed);
        }
        let selected = OwnerRootIndex::from(&activation.home);
        let selected_home = local
            .root_scratch
            .get_mut(usize::from(selected))
            .ok_or(RegistrationError::StaleRegistration)?;
        // Install the selected census root before releasing this SymbolRef.
        // No fallible operation follows installation or activation mutation.
        *selected_home = MarkWord::of(definition);
        activation.state = ActivationState::Selected {
            epoch: clock,
            lambda,
            shape,
            instruction: Instruction::Validate,
        };
        Ok(FunctionSelection::Selected)
    }

    fn select_call(
        &mut self,
        cache: &mut FunctionCallCache<'_>,
        activation: &mut FunctionActivation<'_>,
        subset: ReturnSubset,
    ) -> Result<(), FunctionError> {
        let symbols = &self.run.owner.registry.symbols;
        let since = (cache.symbol == Some(self.key))
            .then_some(cache.observed_epoch)
            .flatten();
        // The token keeps all canonical observations inside this same reader.
        let (clock, unchanged, lambda, definition) = {
            let token = self.read_token();
            // SAFETY: identity-checked reader, stable membership, retained World.
            let (clock, unchanged) = unsafe {
                symbols
                    .backing
                    .function_version(self.key.symbol, since, &token)
            }
            .ok_or(RegistrationError::Publication)?;
            if clock == u64::MAX {
                return Err(FunctionError::EpochExhausted);
            }
            // SAFETY: prepared records are immutable while this reader is live.
            let lambda = unsafe { symbols.functions.lambda_for(self.key.symbol, &token) }
                .ok_or(RegistrationError::Publication)?;
            let definition = if unchanged {
                self.run
                    .owner
                    .local
                    .as_ref()
                    .and_then(|local| local.root_scratch.get(cache.home.index))
                    .ok_or(RegistrationError::StaleRegistration)?
                    .value()
            } else {
                // SAFETY: same live reader, complete cell/state snapshot.
                match unsafe { symbols.backing.read_function(self.key.symbol, &token) }
                    .ok_or(RegistrationError::Publication)?
                {
                    FunctionCellSnapshot::Bound(value) => value,
                    FunctionCellSnapshot::Empty | FunctionCellSnapshot::ExplicitlyUnbound => {
                        return Err(FunctionError::VoidFunction);
                    }
                }
            };
            if definition.is_nil() {
                return Err(FunctionError::VoidFunction);
            }
            constant::decode(definition, lambda, subset)?;
            (clock, unchanged, lambda, definition)
        };
        let local = self
            .run
            .owner
            .local
            .as_mut()
            .ok_or(RegistrationError::StaleRegistration)?;
        let [cache_home, selected_home] = local
            .root_scratch
            .get_disjoint_mut([cache.home.index, activation.home.index])
            .map_err(|_| RegistrationError::StaleRegistration)?;
        // Install BOTH census roots before releasing reader authority. Saved
        // metadata records the original clock, never a later clock reload.
        *cache_home = MarkWord::of(definition);
        *selected_home = MarkWord::of(definition);
        #[cfg(test)]
        if !unchanged {
            cache.counts.0 = cache.counts.0.saturating_add(1);
        } else if cache.observed_epoch != Some(clock) {
            cache.counts.1 = cache.counts.1.saturating_add(1);
        } else {
            cache.counts.2 = cache.counts.2.saturating_add(1);
        }
        #[cfg(not(test))]
        let _ = unchanged;
        cache.symbol = Some(self.key);
        cache.observed_epoch = Some(clock);
        activation.state = ActivationState::Selected {
            epoch: clock,
            lambda,
            shape: CallShape::Nullary(subset),
            instruction: Instruction::Validate,
        };
        Ok(())
    }
}

static_assertions::assert_not_impl_any!(FunctionCallCache<'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(FunctionActivation<'static>: Send, Sync, Clone, Copy);
