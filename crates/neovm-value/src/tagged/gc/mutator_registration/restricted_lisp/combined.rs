//! Closed C6 mixed tapes; no generic VM, mutable graph or generated-code entry.
//! Actual mixed execution and stop-all collection live in typed child fixtures.
use super::*;
use std::num::NonZeroU32;

mod execution_tests;
mod executor;
mod tests;

pub(super) use super::WorldRoots;

/// Era openers supplied by the core shell. This module does not name the
/// evaluator. `Roots` is the World walk passed to stopped collection.
pub(super) trait CombinedParts {
    type Keeper: crate::tagged::gc::mutator_registration::EraKeeper;
    type Roots: WorldRoots<Self::Keeper>;
    fn symbol_backing<R>(
        &mut self,
        kind: crate::tagged::gc::PlainSymbolBacking,
        body: impl for<'e> FnOnce(
            crate::tagged::gc::ProofEra<'e, Self::Keeper>,
            crate::intern::SymId,
        ) -> R,
    ) -> R;
    fn symbol_functions<R>(
        &mut self,
        kind: crate::tagged::gc::PlainSymbolBacking,
        body: impl for<'e> FnOnce(
            crate::tagged::gc::ProofEra<'e, Self::Keeper>,
            Vec<crate::intern::SymId>,
        ) -> R,
    ) -> R;
    fn symbol_functions_init<R>(
        &mut self,
        kind: crate::tagged::gc::PlainSymbolBacking,
        init: impl FnOnce(&mut crate::symbol::Obarray, &[crate::intern::SymId]),
        body: impl for<'e> FnOnce(
            crate::tagged::gc::ProofEra<'e, Self::Keeper>,
            Vec<crate::intern::SymId>,
        ) -> R,
    ) -> R;
    fn symbol_name_inputs_init<R>(
        &mut self,
        kind: crate::tagged::gc::PlainSymbolBacking,
        spelling: &str,
        init: impl FnOnce(&mut crate::symbol::Obarray, &[crate::intern::SymId]),
        body: impl for<'e> FnOnce(
            crate::tagged::gc::ProofEra<'e, Self::Keeper>,
            Vec<crate::intern::SymId>,
        ) -> R,
    ) -> R;
    fn roots(&self) -> Self::Roots;
}

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
        keeper: &crate::symbol::parallel::KeeperWorldDesignator<'_>,
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

struct CombinedHost<'a, K: crate::tagged::gc::mutator_registration::EraKeeper, W> {
    symbol_backing: &'a mut dyn FnMut(
        crate::tagged::gc::PlainSymbolBacking,
        &mut dyn for<'e> FnMut(crate::tagged::gc::ProofEra<'e, K>, crate::intern::SymId),
    ),
    symbol_functions: &'a mut dyn FnMut(
        crate::tagged::gc::PlainSymbolBacking,
        &mut dyn for<'e> FnMut(crate::tagged::gc::ProofEra<'e, K>, Vec<crate::intern::SymId>),
    ),
    symbol_functions_init: &'a mut dyn FnMut(
        crate::tagged::gc::PlainSymbolBacking,
        &mut dyn FnMut(&mut crate::symbol::Obarray, &[crate::intern::SymId]),
        &mut dyn for<'e> FnMut(crate::tagged::gc::ProofEra<'e, K>, Vec<crate::intern::SymId>),
    ),
    symbol_name_inputs_init: &'a mut dyn FnMut(
        crate::tagged::gc::PlainSymbolBacking,
        &str,
        &mut dyn FnMut(&mut crate::symbol::Obarray, &[crate::intern::SymId]),
        &mut dyn for<'e> FnMut(crate::tagged::gc::ProofEra<'e, K>, Vec<crate::intern::SymId>),
    ),
    world_roots: W,
}

impl<'a, K, W> CombinedParts for CombinedHost<'a, K, W>
where
    K: crate::tagged::gc::mutator_registration::EraKeeper,
    W: WorldRoots<K>,
{
    type Keeper = K;
    type Roots = W;

    fn symbol_backing<R>(
        &mut self,
        kind: crate::tagged::gc::PlainSymbolBacking,
        body: impl for<'e> FnOnce(crate::tagged::gc::ProofEra<'e, K>, crate::intern::SymId) -> R,
    ) -> R {
        let mut body = Some(body);
        let mut slot = None;
        (self.symbol_backing)(kind, &mut |era, id| {
            let body = body.take().expect("symbol backing body runs once");
            slot = Some(body(era, id));
        });
        slot.expect("symbol backing body runs once")
    }

    fn symbol_functions<R>(
        &mut self,
        kind: crate::tagged::gc::PlainSymbolBacking,
        body: impl for<'e> FnOnce(crate::tagged::gc::ProofEra<'e, K>, Vec<crate::intern::SymId>) -> R,
    ) -> R {
        let mut body = Some(body);
        let mut slot = None;
        (self.symbol_functions)(kind, &mut |era, ids| {
            let body = body.take().expect("symbol functions body runs once");
            slot = Some(body(era, ids));
        });
        slot.expect("symbol functions body runs once")
    }

    fn symbol_functions_init<R>(
        &mut self,
        kind: crate::tagged::gc::PlainSymbolBacking,
        init: impl FnOnce(&mut crate::symbol::Obarray, &[crate::intern::SymId]),
        body: impl for<'e> FnOnce(crate::tagged::gc::ProofEra<'e, K>, Vec<crate::intern::SymId>) -> R,
    ) -> R {
        let mut init = Some(init);
        let mut body = Some(body);
        let mut slot = None;
        (self.symbol_functions_init)(
            kind,
            &mut |obarray, ids| {
                let init = init.take().expect("symbol functions init runs once");
                init(obarray, ids);
            },
            &mut |era, ids| {
                let body = body.take().expect("symbol functions body runs once");
                slot = Some(body(era, ids));
            },
        );
        slot.expect("symbol functions body runs once")
    }

    fn symbol_name_inputs_init<R>(
        &mut self,
        kind: crate::tagged::gc::PlainSymbolBacking,
        spelling: &str,
        init: impl FnOnce(&mut crate::symbol::Obarray, &[crate::intern::SymId]),
        body: impl for<'e> FnOnce(crate::tagged::gc::ProofEra<'e, K>, Vec<crate::intern::SymId>) -> R,
    ) -> R {
        let mut init = Some(init);
        let mut body = Some(body);
        let mut slot = None;
        (self.symbol_name_inputs_init)(
            kind,
            spelling,
            &mut |obarray, ids| {
                let init = init.take().expect("name inputs init runs once");
                init(obarray, ids);
            },
            &mut |era, ids| {
                let body = body.take().expect("name inputs body runs once");
                slot = Some(body(era, ids));
            },
        );
        slot.expect("name inputs body runs once")
    }

    fn roots(&self) -> W {
        self.world_roots
    }
}

/// One moved combined-executor test. The core shell opens the symbol era and
/// passes the baton World walk. The typed-tape refusal stays `#[test]` here.
pub(crate) fn run_combined_executor_test<K, W>(
    name: &str,
    symbol_backing: &mut dyn FnMut(
        crate::tagged::gc::PlainSymbolBacking,
        &mut dyn for<'e> FnMut(crate::tagged::gc::ProofEra<'e, K>, crate::intern::SymId),
    ),
    symbol_functions: &mut dyn FnMut(
        crate::tagged::gc::PlainSymbolBacking,
        &mut dyn for<'e> FnMut(crate::tagged::gc::ProofEra<'e, K>, Vec<crate::intern::SymId>),
    ),
    symbol_functions_init: &mut dyn FnMut(
        crate::tagged::gc::PlainSymbolBacking,
        &mut dyn FnMut(&mut crate::symbol::Obarray, &[crate::intern::SymId]),
        &mut dyn for<'e> FnMut(crate::tagged::gc::ProofEra<'e, K>, Vec<crate::intern::SymId>),
    ),
    symbol_name_inputs_init: &mut dyn FnMut(
        crate::tagged::gc::PlainSymbolBacking,
        &str,
        &mut dyn FnMut(&mut crate::symbol::Obarray, &[crate::intern::SymId]),
        &mut dyn for<'e> FnMut(crate::tagged::gc::ProofEra<'e, K>, Vec<crate::intern::SymId>),
    ),
    world_roots: W,
) where
    K: crate::tagged::gc::mutator_registration::EraKeeper,
    W: WorldRoots<K>,
{
    let mut host = CombinedHost {
        symbol_backing,
        symbol_functions,
        symbol_functions_init,
        symbol_name_inputs_init,
        world_roots,
    };
    match name {
        "p3sym_c6_global_function_plain" => tests::p3sym_c6_global_function_plain(&mut host),
        "p3sym_c6_global_function_projected" => tests::p3sym_c6_global_function_projected(&mut host),
        "p3sym_c6_function_intern_plain" => tests::p3sym_c6_function_intern_plain(&mut host),
        "p3sym_c6_function_intern_projected" => tests::p3sym_c6_function_intern_projected(&mut host),
        "p3sym_c6_keeper_census_plain" => tests::p3sym_c6_keeper_census_plain(&mut host),
        "p3sym_c6_keeper_census_projected" => tests::p3sym_c6_keeper_census_projected(&mut host),
        "p3sym_c6_minor_plain" => tests::p3sym_c6_minor_plain(&mut host),
        "p3sym_c6_minor_projected" => tests::p3sym_c6_minor_projected(&mut host),
        "p3sym_c6_noop_control_finishes_plain_and_projected" => tests::p3sym_c6_noop_control_finishes_plain_and_projected(&mut host),
        "p3sym_c6_unwind_control_releases_live_reader_before_finish" => tests::p3sym_c6_unwind_control_releases_live_reader_before_finish(&mut host),
        "p3sym_c6_mixed_noop_control_finishes_with_a_real_frame" => execution_tests::p3sym_c6_mixed_noop_control_finishes_with_a_real_frame(&mut host),
        "p3sym_c6_mixed_unwind_releases_reader_and_selected_activation" => execution_tests::p3sym_c6_mixed_unwind_releases_reader_and_selected_activation(&mut host),
        "p3sym_c6_mixed_selected_old_name_survives_scalar_replacement" => execution_tests::p3sym_c6_mixed_selected_old_name_survives_scalar_replacement(&mut host),
        "p3sym_c6_mixed_intern_and_soft_allow_the_same_source_destination_home" => execution_tests::p3sym_c6_mixed_intern_and_soft_allow_the_same_source_destination_home(&mut host),
        "p3sym_c6_mixed_keyword_refusal_preserves_pc_and_destination" => execution_tests::p3sym_c6_mixed_keyword_refusal_preserves_pc_and_destination(&mut host),
        "p3sym_c6_mixed_seed_capacity_and_repeated_seed_preserve_the_bank" => execution_tests::p3sym_c6_mixed_seed_capacity_and_repeated_seed_preserve_the_bank(&mut host),
        "p3sym_c6_mixed_expired_pending_instruction_never_restarts" => execution_tests::p3sym_c6_mixed_expired_pending_instruction_never_restarts(&mut host),
        "p3sym_c6_mixed_program_budget_is_independent_of_contention" => execution_tests::p3sym_c6_mixed_program_budget_is_independent_of_contention(&mut host),
        "p3sym_c6_mixed_retired_session_and_stale_frame_refuse_before_access" => execution_tests::p3sym_c6_mixed_retired_session_and_stale_frame_refuse_before_access(&mut host),
        "p3sym_c6_mixed_admission_checks_every_program_before_preparing_or_registering" => execution_tests::p3sym_c6_mixed_admission_checks_every_program_before_preparing_or_registering(&mut host),
        "p3sym_c6_cohort_two_plain_full_root_transfers" => execution_tests::collection::p3sym_c6_cohort_two_plain_full_root_transfers(&mut host),
        "p3sym_c6_cohort_two_projected_full_root_transfers" => execution_tests::collection::p3sym_c6_cohort_two_projected_full_root_transfers(&mut host),
        "p3sym_c6_cohort_four_plain_full_root_transfers" => execution_tests::collection::p3sym_c6_cohort_four_plain_full_root_transfers(&mut host),
        "p3sym_c6_cohort_four_projected_full_root_transfers" => execution_tests::collection::p3sym_c6_cohort_four_projected_full_root_transfers(&mut host),
        "p3sym_c6_cohort_noop_control_releases_every_owner" => execution_tests::collection::p3sym_c6_cohort_noop_control_releases_every_owner(&mut host),
        "p3sym_c6_cohort_unwind_releases_every_owner" => execution_tests::collection::p3sym_c6_cohort_unwind_releases_every_owner(&mut host),
        "p3sym_c6_minor_adapter_two_plain" => execution_tests::collection::p3sym_c6_minor_adapter_two_plain(&mut host),
        "p3sym_c6_minor_adapter_two_projected" => execution_tests::collection::p3sym_c6_minor_adapter_two_projected(&mut host),
        "p3sym_c6_minor_adapter_four_plain" => execution_tests::collection::p3sym_c6_minor_adapter_four_plain(&mut host),
        "p3sym_c6_minor_adapter_four_projected" => execution_tests::collection::p3sym_c6_minor_adapter_four_projected(&mut host),
        "p3sym_c6_fresh_two_plain" => execution_tests::collection::fresh::p3sym_c6_fresh_two_plain(&mut host),
        "p3sym_c6_fresh_two_projected" => execution_tests::collection::fresh::p3sym_c6_fresh_two_projected(&mut host),
        "p3sym_c6_fresh_four_plain" => execution_tests::collection::fresh::p3sym_c6_fresh_four_plain(&mut host),
        "p3sym_c6_fresh_four_projected" => execution_tests::collection::fresh::p3sym_c6_fresh_four_projected(&mut host),
        "p3sym_c6_fresh_unwind_returns_catalog_and_releases_banks" => execution_tests::collection::fresh::p3sym_c6_fresh_unwind_returns_catalog_and_releases_banks(&mut host),
        "p3sym_c6_fresh_noop_returns_catalog_and_releases_banks" => execution_tests::collection::fresh::p3sym_c6_fresh_noop_returns_catalog_and_releases_banks(&mut host),
        "p3sym_c6_relog_two_plain" => execution_tests::collection::relog::p3sym_c6_relog_two_plain(&mut host),
        "p3sym_c6_relog_two_projected" => execution_tests::collection::relog::p3sym_c6_relog_two_projected(&mut host),
        "p3sym_c6_relog_four_plain" => execution_tests::collection::relog::p3sym_c6_relog_four_plain(&mut host),
        "p3sym_c6_relog_four_projected" => execution_tests::collection::relog::p3sym_c6_relog_four_projected(&mut host),
        "p3sym_c6_relog_noop_returns_catalog_and_releases_banks" => execution_tests::collection::relog::p3sym_c6_relog_noop_returns_catalog_and_releases_banks(&mut host),
        "p3sym_c6_relog_unwind_returns_catalog_and_releases_banks" => execution_tests::collection::relog::p3sym_c6_relog_unwind_returns_catalog_and_releases_banks(&mut host),
        "p3sym_c6_name_minor_four_plain" => execution_tests::collection::name_minor::p3sym_c6_name_minor_four_plain(&mut host),
        "p3sym_c6_name_minor_four_void" => execution_tests::collection::name_minor::p3sym_c6_name_minor_four_void(&mut host),
        "p3sym_c6_name_minor_noop_releases_banks" => execution_tests::collection::name_minor::p3sym_c6_name_minor_noop_releases_banks(&mut host),
        "p3sym_c6_name_minor_unwind_releases_banks" => execution_tests::collection::name_minor::p3sym_c6_name_minor_unwind_releases_banks(&mut host),
        other => panic!("unknown combined executor test {other}"),
    }
}
