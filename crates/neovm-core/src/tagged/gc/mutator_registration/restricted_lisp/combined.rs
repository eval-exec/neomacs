//! C6.1 typed preparation surface. The mixed executor, keeper census and
//! stopped minor are deliberately absent until their individual engine stages.
//! Nothing in this cfg(test) module is a production implementation or fast path.
use super::*;
use std::num::NonZeroU32;

mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RootRegister(u16);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RootCapacity(NonZeroU32);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SuccessfulStepBudget(NonZeroU32);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TapeError {
    Capacity,
    Register,
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
    SharedGraphMutation,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CombinedInstruction {
    GlobalToHome {
        destination: RootRegister,
    },
    HomeToFunction {
        source: RootRegister,
    },
    CallToHome {
        destination: RootRegister,
    },
    InitialIntern {
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
        // Inspect every instruction before roots or owners are admitted. No
        // generated-code entry or mutable shared graph operation can hide here.
        for instruction in &instructions {
            match *instruction {
                CombinedInstruction::GlobalToHome { destination }
                | CombinedInstruction::CallToHome { destination } => {
                    RootRegister::checked(destination.0, capacity)?;
                }
                CombinedInstruction::HomeToFunction { source } => {
                    RootRegister::checked(source.0, capacity)?;
                }
                CombinedInstruction::InitialIntern { name, destination } => {
                    RootRegister::checked(name.0, capacity)?;
                    RootRegister::checked(destination.0, capacity)?;
                }
                CombinedInstruction::Safepoint => {}
                CombinedInstruction::Excluded(operation) => {
                    return Err(TapeError::Excluded(operation));
                }
            }
        }
        Ok(Self {
            instructions,
            capacity,
            steps,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MissingCapability {
    GlobalFunctionTransfer,
    FunctionInternTransfer,
    ImmutableNameReturn,
    AllOwnerMinor,
}
impl MissingCapability {
    fn name(self) -> &'static str {
        match self {
            Self::GlobalFunctionTransfer => "combined_global_function_transfer",
            Self::FunctionInternTransfer => "combined_function_intern_transfer",
            Self::ImmutableNameReturn => "immutable_managed_name_return",
            Self::AllOwnerMinor => "all_owner_stopped_minor",
        }
    }
}
#[derive(Debug, PartialEq, Eq)]
enum PreparationError {
    Missing(MissingCapability),
    Authority(RegistrationError),
    Global(crate::tagged::gc::GlobalError),
}
#[derive(Clone, Copy)]
enum RunningRequest {
    Noop,
    GlobalFunction,
    FunctionIntern,
    ManagedNameCall,
}
#[derive(Clone, Copy)]
enum StoppedRequest {
    Noop,
    KeeperCensus,
    Minor,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CollectionKind {
    Major,
    Minor,
}

/// These signatures are the C6.1 test surface, not a fallback to a legacy
/// evaluator. Later commits replace each precise Missing result with the
/// corresponding sealed implementation. The source audit, not these stubs,
/// establishes which engine capability is currently absent.
struct PendingC6;
impl PendingC6 {
    fn running<'owner, 'era>(
        request: RunningRequest,
        run: &mut RunAccess<'owner, 'era>,
        symbol: &crate::tagged::gc::SymbolHandle,
        home: &mut OwnerRootSlot<'era>,
    ) -> Result<(), PreparationError> {
        // Even the no-op uses the real root bank and an actual reader lease;
        // the fixture must release them on every missing-operation outcome.
        run.sym_boundp_into(symbol, home)
            .map_err(PreparationError::Global)?;
        match request {
            RunningRequest::Noop => Ok(()),
            RunningRequest::GlobalFunction => Err(PreparationError::Missing(
                MissingCapability::GlobalFunctionTransfer,
            )),
            RunningRequest::FunctionIntern => Err(PreparationError::Missing(
                MissingCapability::FunctionInternTransfer,
            )),
            RunningRequest::ManagedNameCall => Err(PreparationError::Missing(
                MissingCapability::ImmutableNameReturn,
            )),
        }
    }
    fn stopped(
        request: StoppedRequest,
        symbols: &crate::tagged::gc::KeeperSymbols<'_>,
        keeper: &crate::emacs_core::eval::world::Baton,
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
            StoppedRequest::Minor => {
                Err(PreparationError::Missing(MissingCapability::AllOwnerMinor))
            }
        }
    }
    fn collection_kind(request: StoppedRequest) -> CollectionKind {
        match request {
            StoppedRequest::Noop | StoppedRequest::KeeperCensus => CollectionKind::Major,
            StoppedRequest::Minor => CollectionKind::Minor,
        }
    }
}

static_assertions::assert_eq_size!(RootRegister, u16);
static_assertions::assert_eq_size!(RootCapacity, u32);
static_assertions::assert_eq_size!(SuccessfulStepBudget, u32);
