//! Unique finalized code moves to each owner; rooted answers survive real stops.
use super::*;
use neovm_value::tagged::gc::function_write_hooks::HofOperation;
use neovm_value::tagged::gc::{
    CohortPhase, CohortSize, ExecutionFixtureReceipt, ExecutionTestDeadline, ExecutionTestHomes,
    ExecutionTestRetry, GlobalError, HofTestInputs, PlainSymbolBacking, execution_test_retry,
    with_test_prepared_hof_cohort,
};
use neovm_value::tagged::value::TaggedValue;

struct Kernels {
    scalar: ParallelCompiledCode,
    value: ParallelCompiledCellCode,
    function: ParallelCompiledCellCode,
    boundp: ParallelCompiledCellCode,
    fboundp: ParallelCompiledCellCode,
    mapping: ParallelCompiledHofCode,
}
impl Kernels {
    #[doc(hidden)]
    pub fn compile(
        admission: &Gen1Admission<'_>,
        operation: HofOperation,
    ) -> Result<Self, ParallelCompileError> {
        Ok(Self {
            scalar: ParallelCompiledCode::compile(admission, ScalarProgram::Argument)?,
            value: ParallelCompiledCellCode::compile(admission, CellRead::GlobalValue)?,
            function: ParallelCompiledCellCode::compile(admission, CellRead::Function)?,
            boundp: ParallelCompiledCellCode::compile(admission, CellRead::GlobalBoundp)?,
            fboundp: ParallelCompiledCellCode::compile(admission, CellRead::FunctionBoundp)?,
            mapping: ParallelCompiledHofCode::compile(admission, operation)?,
        })
    }
}
// This is an auto-trait check, never an unsafe Send assertion. Every worker
// uniquely owns its finalized module, enters it by borrow, then destroys it.
static_assertions::assert_impl_all!(Kernels: Send);
static_assertions::assert_not_impl_any!(Kernels: Clone, Copy);
#[cfg(feature = "jit")]
static_assertions::assert_not_impl_any!(Kernels: Sync);

pub(super) fn registration(error: &RegistrationError) -> ExecutionTestRetry {
    match error {
        RegistrationError::Busy => ExecutionTestRetry::Busy,
        RegistrationError::WrongHeap
        | RegistrationError::WrongOwner
        | RegistrationError::StaleRegistration
        | RegistrationError::DuplicateOwner
        | RegistrationError::WrongThread
        | RegistrationError::Poisoned
        | RegistrationError::Exhausted
        | RegistrationError::Capacity
        | RegistrationError::OutstandingLoan
        | RegistrationError::OpenRegion
        | RegistrationError::ResidualSatb
        | RegistrationError::Publication
        | RegistrationError::PendingRoots
        | RegistrationError::PendingYoung => ExecutionTestRetry::Refuse,
    }
}
pub(super) fn function(error: &FunctionError) -> ExecutionTestRetry {
    match error {
        FunctionError::Authority(error) => registration(error),
        FunctionError::VoidFunction
        | FunctionError::UnsupportedDefinition
        | FunctionError::EpochExhausted
        | FunctionError::ActiveActivation
        | FunctionError::InactiveActivation => ExecutionTestRetry::Refuse,
    }
}
fn contention(error: &ParallelEntryError) -> ExecutionTestRetry {
    match error {
        ParallelEntryError::Authority(error) => registration(error),
        ParallelEntryError::CellRead(CellReadError::Global(GlobalError::Authority(error))) => {
            registration(error)
        }
        ParallelEntryError::CellRead(CellReadError::Global(GlobalError::VoidVariable)) => {
            ExecutionTestRetry::Refuse
        }
        ParallelEntryError::CellRead(CellReadError::Function(error))
        | ParallelEntryError::Function(error) => function(error),
        ParallelEntryError::Hof(HofError::Authority(error)) => registration(error),
        ParallelEntryError::Hof(HofError::Function(error)) => function(error),
        ParallelEntryError::Hof(
            HofError::AliasedOperand | HofError::ConsumedInput | HofError::UnexpectedCallbackState,
        )
        | ParallelEntryError::UnsupportedAbi
        | ParallelEntryError::MalformedStatus
        | ParallelEntryError::NonImmediateResult
        | ParallelEntryError::NoSlotBacking
        | ParallelEntryError::UnexpectedEntryKind => ExecutionTestRetry::Refuse,
    }
}
pub(super) fn retry<T>(
    operation: impl FnMut() -> Result<T, ParallelEntryError>,
) -> Result<T, String> {
    execution_test_retry(ExecutionTestDeadline::contention(), operation, contention)
        .map_err(|error| format!("native owner entry: {error:?}"))
}
fn list(home: &OwnerRootSlot<'_>, run: &RunAccess<'_, '_>) -> String {
    let value = TaggedValue::from_bits(ExecutionTestHomes::word(home, run));
    assert!(value.is_cons());
    assert!(value.cons_cdr_unobserved().is_nil());
    format!(
        "({})",
        value
            .cons_car_unobserved()
            .as_fixnum()
            .expect("one scalar result")
    )
}
fn assert_mapping(operation: HofOperation, home: &OwnerRootSlot<'_>, run: &RunAccess<'_, '_>) {
    let rows = include_str!("hof_oracle.expected")
        .lines()
        .collect::<Vec<_>>();
    assert_eq!(
        list(home, run),
        match operation {
            HofOperation::Mapc => rows[5],
            HofOperation::Mapcar => rows[0],
        }
    );
}
fn body<'era>(
    kernels: &Result<Kernels, ParallelCompileError>,
    operation: HofOperation,
    admission: &ParallelExecutionAdmission<'era>,
    run: &mut RunAccess<'_, 'era>,
    home: &OwnerRootSlot<'era>,
    inputs: &mut HofTestInputs<'era>,
    phase: CohortPhase,
) -> Result<(), String> {
    let kernels = match kernels {
        Ok(kernels) => kernels,
        Err(ParallelCompileError::BackendUnavailable) => {
            assert!(!cfg!(feature = "jit"));
            eprintln!("NMJIT_N4_COHORT_BACKEND_UNAVAILABLE_REFUSED_CLEAN");
            return Ok(());
        }
        Err(
            error @ (ParallelCompileError::Module(_)
            | ParallelCompileError::Define(_)
            | ParallelCompileError::Finalize(_)
            | ParallelCompileError::UnsupportedPointerWidth
            | ParallelCompileError::AbiLayout),
        ) => return Err(format!("cohort compilation: {error:?}")),
    };
    match phase {
        CohortPhase::Initial => {}
        CohortPhase::AfterFirstCollection | CohortPhase::AfterSecondCollection => {
            // The prior phase's managed result was retained only by this home.
            assert_mapping(operation, home, run);
            inputs.singleton = HofSingleton::new(
                run,
                ImmediateResult::from_word(TaggedValue::fixnum(7).bits()).unwrap(),
            )
            .map_err(|error| format!("fresh singleton: {error:?}"))?;
            eprintln!("NMJIT_N4_COHORT_MANAGED_RESULT_SURVIVED {phase:?}");
        }
    }
    retry(|| {
        kernels
            .scalar
            .run(admission, run, home, ImmediateResult::True)
    })?;
    assert_eq!(ExecutionTestHomes::read(home, run), ImmediateResult::True);
    for code in [&kernels.value, &kernels.function] {
        retry(|| code.run(admission, run, home, &inputs.callback_symbol))?;
        assert!(ExecutionTestHomes::is_cons(home, run));
        assert_eq!(
            ExecutionTestHomes::word(home, run),
            ExecutionTestHomes::word(&inputs.expected, run)
        );
    }
    for code in [&kernels.boundp, &kernels.fboundp] {
        retry(|| code.run(admission, run, home, &inputs.callback_symbol))?;
        assert_eq!(ExecutionTestHomes::read(home, run), ImmediateResult::True);
    }
    let input = inputs.singleton.word_for_test(run);
    let selected = retry(|| {
        kernels.mapping.run(
            admission,
            run,
            home,
            &inputs.mapper,
            &inputs.callback,
            &inputs.expected,
            &mut inputs.singleton,
            &mut inputs.scratch,
        )
    })?;
    assert_eq!(selected, FunctionSelection::Selected);
    assert_mapping(operation, home, run);
    match operation {
        HofOperation::Mapc => assert_eq!(input, ExecutionTestHomes::word(home, run)),
        HofOperation::Mapcar => assert_ne!(input, ExecutionTestHomes::word(home, run)),
    }
    eprintln!("NMJIT_N4_COHORT_NATIVE_PHASE_OK {phase:?} {operation:?}");
    Ok(())
}
fn cohort(size: CohortSize) {
    for (backing, operation) in [
        (PlainSymbolBacking::Unprojected, HofOperation::Mapc),
        (PlainSymbolBacking::Projected, HofOperation::Mapcar),
    ] {
        let receipt = with_test_prepared_hof_cohort(
            backing,
            operation,
            size,
            |admission| Kernels::compile(admission, operation),
            |kernels, admission, run, home, inputs, phase| {
                body(kernels, operation, admission, run, home, inputs, phase)
            },
        );
        match receipt {
            ExecutionFixtureReceipt::Gen1Completed => {
                eprintln!("NMJIT_N4_OWNED_NATIVE_HANDOFF_OK {size:?} {backing:?}")
            }
            ExecutionFixtureReceipt::Gen0Refused | ExecutionFixtureReceipt::TelemetryRefused => {}
        }
    }
}
#[test]
fn nmjit_n4_one_owner_native_handoff_roots_survive_two_collections() {
    cohort(CohortSize::One);
}
#[test]
fn nmjit_n4_two_owner_native_handoff_roots_survive_two_collections() {
    cohort(CohortSize::Two);
}
#[test]
fn nmjit_n4_four_owner_native_handoff_roots_survive_two_collections() {
    cohort(CohortSize::Four);
}
#[test]
fn nmjit_n4_owner_cohort_noop_and_body_failure_teardown() {
    let receipt = with_test_prepared_hof_cohort(
        PlainSymbolBacking::Unprojected,
        HofOperation::Mapcar,
        CohortSize::Two,
        |_| (),
        |(), _, _, _, _, _| Ok(()),
    );
    #[derive(Clone, Copy)]
    enum BodyControl {
        Return,
        Panic,
    }
    for control in [BodyControl::Return, BodyControl::Panic] {
        let result = std::panic::catch_unwind(|| {
            with_test_prepared_hof_cohort(
                PlainSymbolBacking::Projected,
                HofOperation::Mapcar,
                CohortSize::Two,
                |_| (),
                |(), _, _, _, _, _| match control {
                    BodyControl::Panic => panic!("NMJIT_N4_COHORT_BODY_CONTROL"),
                    BodyControl::Return => Err("NMJIT_N4_COHORT_RETURN_CONTROL".into()),
                },
            );
        });
        match receipt {
            ExecutionFixtureReceipt::Gen1Completed => {
                let payload = result.expect_err("body failure stays visible after clean finish");
                let text = payload
                    .downcast_ref::<String>()
                    .map(String::as_str)
                    .or_else(|| payload.downcast_ref::<&str>().copied())
                    .expect("named body error");
                assert!(
                    text.contains(match control {
                        BodyControl::Panic => "NMJIT_N4_COHORT_BODY_CONTROL",
                        BodyControl::Return => "NMJIT_N4_COHORT_RETURN_CONTROL",
                    }),
                    "{text}"
                );
            }
            ExecutionFixtureReceipt::Gen0Refused | ExecutionFixtureReceipt::TelemetryRefused => {
                assert!(result.is_ok())
            }
        }
    }
}
