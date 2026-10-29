use super::*;
use crate::tagged::gc::{GlobalError, PlainSymbolBacking};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Debug, PartialEq, Eq)]
enum CleanupError {
    Registration(RegistrationError),
    Retirement(RendezvousError),
}
#[derive(Debug, PartialEq, Eq)]
enum CleanupReceipt {
    Pending,
    Finished(Result<(), CleanupError>),
}
enum Resources<'era> {
    Resident {
        owner: Mutator<'era>,
        seed: SharedRoot,
        homes: Vec<OwnerRootSlot<'era>>,
    },
    Released,
}
#[must_use = "release all fixture homes and retire the owner before era.finish"]
struct FixtureGuard<'era> {
    resources: Resources<'era>,
    receipt: Rc<RefCell<CleanupReceipt>>,
}
impl Drop for FixtureGuard<'_> {
    fn drop(&mut self) {
        let resources = std::mem::replace(&mut self.resources, Resources::Released);
        let result = match resources {
            Resources::Released => return,
            Resources::Resident {
                mut owner,
                seed,
                homes,
            } => (|| {
                // Root handles drop before clearing the actual resident census;
                // dropping a Rust handle alone does not clear its bank word.
                drop(homes);
                contention::retry(
                    Deadline::contention(),
                    || owner.run_access()?.clear_root_slots(),
                    contention::registration,
                )
                .map_err(CleanupError::Registration)?;
                drop(seed);
                retry_owned(
                    Deadline::contention(),
                    owner,
                    |owner| owner.retire_rooted(&[]),
                    |failed| match contention::rendezvous(&failed.reason) {
                        Retry::Busy => ControlFlow::Continue(failed.into_owner()),
                        Retry::Refuse => ControlFlow::Break(failed),
                    },
                )
                .map_err(|failed| CleanupError::Retirement(failed.reason))
            })(),
        };
        // Nonpanicking protocol cleanup records a separate outcome. The body
        // cannot borrow this private receipt while Drop needs it.
        *self.receipt.borrow_mut() = CleanupReceipt::Finished(result);
    }
}
#[derive(Clone, Copy)]
enum FixtureBody {
    Request,
    UnwindWithReader,
}

fn classify(error: &PreparationError) -> Retry {
    match error {
        PreparationError::Authority(error) => contention::registration(error),
        PreparationError::Global(error) => contention::global(error),
    }
}
fn running_expected(request: RunningRequest) -> Result<(), PreparationError> {
    match request {
        RunningRequest::Noop => Ok(()),
    }
}
fn stopped_expected(request: StoppedRequest) -> Result<(), PreparationError> {
    match request {
        StoppedRequest::Noop => Ok(()),
        StoppedRequest::KeeperCensus => Ok(()),
    }
}

/// Running and stopped no-op/census checks finish the same actual fixture.
/// The combined collector cases live in execution_tests, without stubs.
fn fixture(
    backing: PlainSymbolBacking,
    running: RunningRequest,
    stopped_request: StoppedRequest,
    body: FixtureBody,
) -> std::thread::Result<Result<(), PreparationError>> {
    Context::with_test_symbol_backing_era(backing, |era, id| {
        let symbol = era.admit_plain_symbol(id).unwrap();
        let mut stopped = era.stop().unwrap();
        let seed = era
            .with_stopped_heap_for_test(&stopped, |heap| unsafe {
                // SAFETY: matching real stopped heap; only immediate NIL is rooted.
                SharedRoot::new(heap, TaggedValue::NIL)
            })
            .unwrap();
        stopped.trace(|_, _| Ok(())).unwrap();
        stopped.complete().unwrap();
        let capacity = RootCapacity::try_from(2).unwrap();
        let permit = era
            .register(1, usize::try_from(capacity.0.get()).unwrap())
            .unwrap();
        let owner = retry_owned(
            Deadline::contention(),
            permit,
            |permit| permit.activate(era.heap_identity(), 1),
            |failed| match contention::registration(&failed.reason) {
                Retry::Busy => ControlFlow::Continue(failed.into_permit()),
                Retry::Refuse => ControlFlow::Break(failed),
            },
        )
        .unwrap();
        let receipt = Rc::new(RefCell::new(CleanupReceipt::Pending));
        let mut guard = FixtureGuard {
            resources: Resources::Resident {
                owner,
                seed,
                homes: Vec::new(),
            },
            receipt: Rc::clone(&receipt),
        };
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            match &mut guard.resources {
                Resources::Released => panic!("C6 fixture released before its body"),
                Resources::Resident { owner, seed, homes } => {
                    for _ in 0..capacity.0.get() {
                        contention::retry(
                            Deadline::contention(),
                            || {
                                let mut run = owner.run_access()?;
                                homes.push(run.root_slot(seed)?);
                                Ok(())
                            },
                            contention::registration,
                        )
                        .unwrap();
                    }
                    match body {
                        FixtureBody::Request => {
                            let answer = contention::retry(
                                Deadline::contention(),
                                || {
                                    let mut run =
                                        owner.run_access().map_err(PreparationError::Authority)?;
                                    PreparationChecks::running(
                                        running,
                                        &mut run,
                                        &symbol,
                                        &mut homes[0],
                                    )
                                },
                                classify,
                            );
                            assert_eq!(
                                answer,
                                running_expected(running),
                                "C6 specific running refusal"
                            );
                            answer
                        }
                        FixtureBody::UnwindWithReader => {
                            let mut run = owner.run_access().unwrap();
                            let _reader = run.symbol_ref(&symbol).unwrap();
                            panic!("C6 controlled fixture unwind");
                        }
                    }
                }
            }
        }));
        drop(guard);
        assert_eq!(
            *receipt.borrow(),
            CleanupReceipt::Finished(Ok(())),
            "C6 independent root teardown"
        );
        let mut stopped = era.stop().unwrap();
        let answer = {
            let symbols = stopped.symbols().unwrap();
            PreparationChecks::stopped(stopped_request, &symbols, era.core.keeper.as_ref().unwrap())
        };
        assert_eq!(
            answer,
            stopped_expected(stopped_request),
            "C6 specific stopped refusal"
        );
        // No-op fixture callbacks succeed. A callback error is terminal,
        // including before images; expected refusals stay outside callbacks.
        stopped.trace(|_, _| Ok(())).unwrap();
        stopped.complete().unwrap();
        let (collector, keeper) = era
            .finish()
            .expect("C6 finish after independent total teardown");
        drop(collector);
        drop(keeper);
        eprintln!("P3SYM_C6_TEARDOWN_OK {backing:?}");
        outcome.map(|running_answer| running_answer.and(answer))
    })
}

#[test]
fn p3sym_c6_global_function_plain() {
    super::execution_tests::transfer_case(
        PlainSymbolBacking::Unprojected,
        super::execution_tests::Transfer::Function,
    );
}
#[test]
fn p3sym_c6_global_function_projected() {
    super::execution_tests::transfer_case(
        PlainSymbolBacking::Projected,
        super::execution_tests::Transfer::Function,
    );
}
#[test]
fn p3sym_c6_function_intern_plain() {
    super::execution_tests::transfer_case(
        PlainSymbolBacking::Unprojected,
        super::execution_tests::Transfer::Intern,
    );
}
#[test]
fn p3sym_c6_function_intern_projected() {
    super::execution_tests::transfer_case(
        PlainSymbolBacking::Projected,
        super::execution_tests::Transfer::Intern,
    );
}
#[test]
fn p3sym_c6_name_return_plain() {
    super::super::super::symbol_functions::names::return_case(PlainSymbolBacking::Unprojected);
}
#[test]
fn p3sym_c6_name_return_projected() {
    super::super::super::symbol_functions::names::return_case(PlainSymbolBacking::Projected);
}
#[test]
fn p3sym_c6_keeper_census_plain() {
    assert_eq!(
        fixture(
            PlainSymbolBacking::Unprojected,
            RunningRequest::Noop,
            StoppedRequest::KeeperCensus,
            FixtureBody::Request,
        )
        .unwrap(),
        Ok(())
    );
}
#[test]
fn p3sym_c6_keeper_census_projected() {
    assert_eq!(
        fixture(
            PlainSymbolBacking::VoidProjection,
            RunningRequest::Noop,
            StoppedRequest::KeeperCensus,
            FixtureBody::Request,
        )
        .unwrap(),
        Ok(())
    );
}
#[test]
fn p3sym_c6_minor_plain() {
    super::execution_tests::minor_case(PlainSymbolBacking::Unprojected);
}
#[test]
fn p3sym_c6_minor_projected() {
    super::execution_tests::minor_case(PlainSymbolBacking::VoidProjection);
}

#[test]
fn p3sym_c6_noop_control_finishes_plain_and_projected() {
    for backing in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::VoidProjection,
    ] {
        assert_eq!(
            fixture(
                backing,
                RunningRequest::Noop,
                StoppedRequest::Noop,
                FixtureBody::Request
            )
            .unwrap(),
            Ok(())
        );
    }
}
#[test]
fn p3sym_c6_unwind_control_releases_live_reader_before_finish() {
    for backing in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::VoidProjection,
    ] {
        let outcome = fixture(
            backing,
            RunningRequest::Noop,
            StoppedRequest::Noop,
            FixtureBody::UnwindWithReader,
        );
        let payload =
            outcome.expect_err("controlled unwind must remain visible after clean teardown");
        let text = payload.downcast_ref::<&str>().copied().unwrap_or("");
        assert_eq!(text, "C6 controlled fixture unwind");
    }
}
#[test]
fn p3sym_c6_typed_tape_refuses_every_excluded_operation() {
    let capacity = RootCapacity::try_from(2).unwrap();
    let home = RootRegister::checked(0, capacity).unwrap();
    for excluded in [
        ExcludedOperation::Jit,
        ExcludedOperation::Callback,
        ExcludedOperation::LegacyContext,
        ExcludedOperation::Cons,
        ExcludedOperation::Car,
        ExcludedOperation::Cdr,
        ExcludedOperation::SetCar,
        ExcludedOperation::SetCdr,
        ExcludedOperation::StringMutation,
        ExcludedOperation::PropertyMutation,
        ExcludedOperation::TableMutation,
    ] {
        let result = CombinedTape::admit(
            vec![
                CombinedInstruction::GlobalToHome {
                    input: InputIndex(0),
                    destination: home,
                },
                CombinedInstruction::CallBegin,
                CombinedInstruction::CallStep { destination: home },
                CombinedInstruction::Safepoint,
                CombinedInstruction::Excluded(excluded),
            ],
            capacity,
            SuccessfulStepBudget::try_from(1).unwrap(),
        );
        assert_eq!(result.err(), Some(TapeError::Excluded(excluded)));
    }
    let tape = CombinedTape::admit(
        vec![
            CombinedInstruction::GlobalToHome {
                input: InputIndex(0),
                destination: home,
            },
            CombinedInstruction::HomeToFunction {
                source: home,
                destination: home,
            },
            CombinedInstruction::CallBegin,
            CombinedInstruction::CallStep { destination: home },
            CombinedInstruction::InitialIntern {
                name: home,
                destination: home,
            },
            CombinedInstruction::Safepoint,
        ],
        capacity,
        SuccessfulStepBudget::try_from(6).unwrap(),
    )
    .unwrap();
    assert_eq!(tape.instructions.len(), 6);
    assert_eq!(tape.capacity, capacity);
    assert_eq!(tape.steps, SuccessfulStepBudget::try_from(6).unwrap());
    assert_eq!(RootRegister::checked(2, capacity), Err(TapeError::Register));
    assert_eq!(RootCapacity::try_from(0), Err(TapeError::Capacity));
    assert_eq!(
        SuccessfulStepBudget::try_from(0),
        Err(TapeError::StepBudget)
    );
    assert!(matches!(
        classify(&PreparationError::Global(GlobalError::VoidVariable)),
        Retry::Refuse
    ));
}
