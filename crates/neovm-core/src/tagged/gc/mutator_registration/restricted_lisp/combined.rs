//! Closed C6 mixed tapes; no generic VM, mutable graph or generated-code entry.
//! Actual mixed execution and stop-all collection live in typed child fixtures.
use super::*;
use std::num::NonZeroU32;

mod execution_tests;
mod executor;
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RootRegister(u16);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RootCapacity(NonZeroU32);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SuccessfulStepBudget(NonZeroU32);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct InputIndex(u16);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TapeError {
    Capacity,
    Register,
    Input,
    StepBudget,
    Excluded(ExcludedOperation),
}
impl TryFrom<usize> for RootCapacity {
    type Error = TapeError;
    fn try_from(value: usize) -> Result<Self, Self::Error> {
        let value = u32::try_from(value).map_err(|_| TapeError::Capacity)?;
        if value > u32::from(u16::MAX) + 1 {
            return Err(TapeError::Capacity);
        }
        NonZeroU32::new(value).map(Self).ok_or(TapeError::Capacity)
    }
}
impl TryFrom<u32> for SuccessfulStepBudget {
    type Error = TapeError;
    fn try_from(value: u32) -> Result<Self, Self::Error> {
        let value = NonZeroU32::new(value).ok_or(TapeError::StepBudget)?;
        if value.get() > STEP_CAP {
            return Err(TapeError::StepBudget);
        }
        Ok(Self(value))
    }
}
impl RootRegister {
    fn checked(index: u16, capacity: RootCapacity) -> Result<Self, TapeError> {
        if u32::from(index) >= capacity.0.get() {
            return Err(TapeError::Register);
        }
        Ok(Self(index))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExcludedOperation {
    Jit,
    Callback,
    LegacyContext,
    Cons,
    Car,
    Cdr,
    SetCar,
    SetCdr,
    StringMutation,
    PropertyMutation,
    TableMutation,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CombinedInstruction {
    GlobalToHome {
        input: InputIndex,
        destination: RootRegister,
    },
    HomeToGlobal {
        input: InputIndex,
        source: RootRegister,
        destination: RootRegister,
    },
    HomeToFunction {
        source: RootRegister,
        destination: RootRegister,
    },
    CallBegin,
    CallStep {
        destination: RootRegister,
    },
    ClearCache,
    ClearActivation,
    ClearHome {
        home: RootRegister,
    },
    InitialIntern {
        name: RootRegister,
        destination: RootRegister,
    },
    InitialSoft {
        name: RootRegister,
        destination: RootRegister,
    },
    Safepoint,
    Excluded(ExcludedOperation),
}
#[must_use = "a checked tape is required before combined owner execution"]
struct CombinedTape {
    instructions: Vec<CombinedInstruction>,
    capacity: RootCapacity,
    steps: SuccessfulStepBudget,
}
impl CombinedTape {
    fn admit(
        instructions: Vec<CombinedInstruction>,
        capacity: RootCapacity,
        steps: SuccessfulStepBudget,
    ) -> Result<Self, TapeError> {
        let tape = Self {
            instructions,
            capacity,
            steps,
        };
        tape.validate()?;
        Ok(tape)
    }

    fn validate(&self) -> Result<(), TapeError> {
        if self.instructions.len() > STEP_CAP as usize {
            return Err(TapeError::StepBudget);
        }
        // Inspect every instruction before roots or owners are admitted. No
        // generated-code entry or mutable shared graph operation can hide here.
        for instruction in &self.instructions {
            match *instruction {
                CombinedInstruction::GlobalToHome { destination, .. }
                | CombinedInstruction::CallStep { destination } => {
                    RootRegister::checked(destination.0, self.capacity)?;
                }
                CombinedInstruction::HomeToGlobal {
                    source,
                    destination,
                    ..
                }
                | CombinedInstruction::HomeToFunction {
                    source,
                    destination,
                } => {
                    RootRegister::checked(source.0, self.capacity)?;
                    RootRegister::checked(destination.0, self.capacity)?;
                }
                CombinedInstruction::InitialIntern { name, destination }
                | CombinedInstruction::InitialSoft { name, destination } => {
                    RootRegister::checked(name.0, self.capacity)?;
                    RootRegister::checked(destination.0, self.capacity)?;
                }
                CombinedInstruction::ClearHome { home } => {
                    RootRegister::checked(home.0, self.capacity)?;
                }
                CombinedInstruction::CallBegin
                | CombinedInstruction::ClearCache
                | CombinedInstruction::ClearActivation
                | CombinedInstruction::Safepoint => {}
                CombinedInstruction::Excluded(operation) => {
                    return Err(TapeError::Excluded(operation));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq)]
enum PreparationError {
    Authority(RegistrationError),
    Global(crate::tagged::gc::GlobalError),
}
#[derive(Clone, Copy)]
enum RunningRequest {
    Noop,
}
#[derive(Clone, Copy)]
enum StoppedRequest {
    Noop,
    KeeperCensus,
}
/// Retained no-op/census fixture checks. Real mixed execution and collection
/// now use Session and ProofEra, with no missing-operation fallback.
struct PreparationChecks;
impl PreparationChecks {
    fn running<'owner, 'era>(
        request: RunningRequest,
        run: &mut RunAccess<'owner, 'era>,
        symbol: &crate::tagged::gc::SymbolHandle,
        home: &mut OwnerRootSlot<'era>,
    ) -> Result<(), PreparationError> {
        // Even the no-op uses the real root bank and an actual reader lease;
        // the fixture releases them independently of the body outcome.
        run.sym_boundp_into(symbol, home)
            .map_err(PreparationError::Global)?;
        match request {
            RunningRequest::Noop => Ok(()),
        }
    }
    fn stopped(
        request: StoppedRequest,
        symbols: &crate::tagged::gc::KeeperSymbols<'_>,
        keeper: &crate::emacs_core::symbol::parallel::KeeperWorldDesignator<'_>,
    ) -> Result<(), PreparationError> {
        match request {
            StoppedRequest::Noop => Ok(()),
            StoppedRequest::KeeperCensus => {
                let mut roots = Vec::new();
                symbols
                    .append_current_symbol_roots(keeper, &mut roots)
                    .map_err(PreparationError::Authority)?;
                symbols
                    .append_current_projection_roots(keeper, &mut roots)
                    .map_err(PreparationError::Authority)
            }
        }
    }
}

static_assertions::assert_eq_size!(RootRegister, u16);
static_assertions::assert_eq_size!(RootCapacity, u32);
static_assertions::assert_eq_size!(SuccessfulStepBudget, u32);
static_assertions::assert_eq_size!(InputIndex, u16);
