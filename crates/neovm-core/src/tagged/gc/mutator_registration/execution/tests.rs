//! Real collector admission, resident owner roots and activation cleanup.
#[path = "tests/cohort.rs"]
mod cohort;
#[path = "tests/hof.rs"]
mod hof;
pub(crate) use super::super::retry::{
    Deadline as ExecutionTestDeadline, Retry as ExecutionTestRetry, retry as execution_test_retry,
};
use super::super::symbol_functions::retry::{self as contention, Deadline, Retry, retry_owned};
use super::*;
use crate::emacs_core::eval::Context;
use crate::tagged::gc::{Gen1Admission, Gen1AdmissionError};
pub(crate) use cohort::{
    CohortPhase, CohortSize, PublicationFacts, PublicationInputs, prepared_hof_cohort,
    prepared_publication_cohort,
};
pub(crate) use hof::{HofTestInputs, prepared_hof_fixture};
use std::ops::ControlFlow;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActualMode {
    Gen0,
    Gen1,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FixtureReceipt {
    Gen0Refused,
    Gen1Completed,
    TelemetryRefused,
}

/// Test-only adapter; the production root-bank leaves remain parent-private.
pub(crate) struct ExecutionTestHomes;
impl ExecutionTestHomes {
    /// Test-only observation through the existing GC-owned root adapter. No
    /// raw managed word is exported by a production activation interface.
    pub(crate) fn word(home: &OwnerRootSlot<'_>, run: &RunAccess<'_, '_>) -> usize {
        home.word_for_test(run)
    }

    pub(crate) fn read(home: &OwnerRootSlot<'_>, run: &RunAccess<'_, '_>) -> ImmediateResult {
        ImmediateResult::from_word(home.word_for_test(run))
            .expect("scalar result home is immediate")
    }

    pub(crate) fn is_cons(home: &OwnerRootSlot<'_>, run: &RunAccess<'_, '_>) -> bool {
        TaggedValue::from_bits(home.word_for_test(run)).is_cons()
    }

    pub(crate) fn nil<'era>(
        run: &mut RunAccess<'_, 'era>,
    ) -> Result<OwnerRootSlot<'era>, RegistrationError> {
        run.private_nil_root()
    }
}

pub(crate) fn fixture(
    body: impl for<'era, 'owner> FnOnce(
        &ParallelExecutionAdmission<'era>,
        &mut RunAccess<'owner, 'era>,
        &OwnerRootSlot<'era>,
    ),
) -> FixtureReceipt {
    prepared_fixture(
        |_| (),
        |(), admission, run, destination| {
            body(admission, run, destination);
        },
    )
}

pub(crate) fn prepared_fixture<P>(
    prepare: impl FnOnce(&Gen1Admission<'_>) -> P,
    body: impl for<'era, 'owner> FnOnce(
        &P,
        &ParallelExecutionAdmission<'era>,
        &mut RunAccess<'owner, 'era>,
        &OwnerRootSlot<'era>,
    ),
) -> FixtureReceipt {
    prepared_cell_fixture(
        PlainSymbolBacking::Unprojected,
        FixtureSeed::Scalar,
        prepare,
        |prepared, admission, run, destination, _symbol| {
            body(prepared, admission, run, destination);
        },
    )
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum FixtureSeed {
    Scalar,
    FunctionDefinition,
    WritableFunctionDefinition,
    WritableFunctionTwin,
    UnsupportedFunction,
}

pub(crate) fn prepared_cell_fixture<P>(
    backing: PlainSymbolBacking,
    seed: FixtureSeed,
    prepare: impl FnOnce(&Gen1Admission<'_>) -> P,
    body: impl for<'era, 'owner> FnOnce(
        &P,
        &ParallelExecutionAdmission<'era>,
        &mut RunAccess<'owner, 'era>,
        &OwnerRootSlot<'era>,
        &SymbolHandle,
    ),
) -> FixtureReceipt {
    // The small proof era is setup only. Its complete finish returns the
    // actual collector/keeper; a fresh production check then controls entry.
    // There is no ProofEra -> execution admission conversion or GEN0 bypass.
    let outcome = Context::try_with_test_symbol_backing_init_era(
        backing,
        |obarray, id| match seed {
            FixtureSeed::Scalar => {}
            FixtureSeed::FunctionDefinition
            | FixtureSeed::WritableFunctionDefinition
            | FixtureSeed::WritableFunctionTwin => {
                let lambda = TaggedValue::from_sym_id(crate::emacs_core::intern::intern("lambda"));
                let body = TaggedValue::cons(TaggedValue::fixnum(73), TaggedValue::NIL);
                let args = TaggedValue::cons(TaggedValue::NIL, body);
                let definition = TaggedValue::cons(lambda, args);
                obarray.seed_value_id(id, definition);
                obarray.set_symbol_function_id(id, definition);
                match seed {
                    FixtureSeed::WritableFunctionTwin => {
                        let body = TaggedValue::cons(TaggedValue::fixnum(73), TaggedValue::NIL);
                        let args = TaggedValue::cons(TaggedValue::NIL, body);
                        let twin = TaggedValue::cons(lambda, args);
                        obarray.seed_value_id(id, twin);
                    }
                    FixtureSeed::FunctionDefinition | FixtureSeed::WritableFunctionDefinition => {}
                    FixtureSeed::Scalar | FixtureSeed::UnsupportedFunction => {}
                }
            }
            FixtureSeed::UnsupportedFunction => {
                obarray.set_symbol_function_id(id, TaggedValue::fixnum(73));
            }
        },
        |mut proof, id| {
            // Preserve the actual sealed census made by this minimal World;
            // finishing setup does not authorize fabricating a new census.
            // No Lisp or collection occurs between finish and re-admission.
            let frozen = proof
                .core
                .legacy_roots
                .take()
                .expect("rooted setup evidence");
            let (mut collector, keeper) = proof.finish().expect("small setup return");
            let mode = if collector.registration_heap_mut().generational_enabled() {
                ActualMode::Gen1
            } else {
                ActualMode::Gen0
            };
            let admitted = match Gen1Admission::check(collector) {
                Ok(admitted) => admitted,
                Err(failure) => {
                    let reason = failure.reason;
                    drop(failure.collector);
                    drop(frozen);
                    drop(keeper);
                    assert_eq!(reason, Gen1AdmissionError::GenerationalRequired);
                    assert_eq!(mode, ActualMode::Gen0);
                    eprintln!("NMJIT_N4_GEN0_EXECUTION_ADMISSION_REFUSED_CLEAN");
                    return FixtureReceipt::Gen0Refused;
                }
            };
            let prepared = prepare(&admitted);
            let era = ParallelHeapEra::admit_rooted(admitted, keeper, frozen)
                .expect("real rooted GEN1 era");
            let symbol = era.admit_plain_symbol(id).expect("activation Plain handle");
            match seed {
                FixtureSeed::Scalar
                | FixtureSeed::FunctionDefinition
                | FixtureSeed::UnsupportedFunction => {}
                FixtureSeed::WritableFunctionDefinition | FixtureSeed::WritableFunctionTwin => {
                    era.prepare_function_symbol(&symbol)
                        .expect("prepared function journal");
                }
            }
            let admission = era.execution_admission();
            let mut owner = era
                .register(1, 4)
                .expect("activation fixture registration")
                .activate(era.heap_identity(), 1)
                .expect("activation fixture owner");
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut run = owner.run_access().expect("activation fixture run");
                let destination = run.private_nil_root().expect("resident result home");
                body(&prepared, &admission, &mut run, &destination, &symbol);
            }));
            // Body locals and all borrowed activations have already unwound.
            // Clear resident homes and retire before inspecting its assertion.
            let cleanup = (|| {
                owner
                    .run_access()
                    .and_then(|mut run| run.clear_root_slots())
                    .map_err(|reason| format!("activation fixture roots: {reason:?}"))?;
                retry_owned(
                    Deadline::contention(),
                    owner,
                    |owner| owner.retire_rooted(&[]),
                    |failure| match contention::rendezvous(&failure.reason) {
                        Retry::Busy => ControlFlow::Continue(failure.into_owner()),
                        Retry::Refuse => ControlFlow::Break(failure),
                    },
                )
                .map_err(|failure| format!("activation fixture retirement: {failure:?}"))
            })();
            drop(admission);
            match seed {
                FixtureSeed::Scalar
                | FixtureSeed::FunctionDefinition
                | FixtureSeed::UnsupportedFunction => {}
                FixtureSeed::WritableFunctionDefinition | FixtureSeed::WritableFunctionTwin => {
                    let mut stopped = era.stop().expect("function fixture stop");
                    era.reconcile_functions(
                        &mut stopped.symbols().expect("stopped symbol authority"),
                    )
                    .expect("function fixture reconciliation");
                    stopped
                        .trace(|_, _| Ok(()))
                        .expect("function fixture stopped trace");
                    stopped.complete().expect("function fixture resume");
                }
            }
            let (collector, keeper) = era.finish().expect("activation fixture finish");
            drop(collector);
            drop(keeper);
            drop(prepared);
            eprintln!("NMJIT_N4_ACTIVATION_TEARDOWN_OK");
            assert_eq!(cleanup, Ok(()));
            assert_eq!(mode, ActualMode::Gen1);
            if let Err(assertion) = result {
                std::panic::resume_unwind(assertion);
            }
            FixtureReceipt::Gen1Completed
        },
    );
    match outcome {
        Ok(receipt) => {
            assert!(!cfg!(feature = "gc-memory-telemetry"));
            receipt
        }
        Err(reason) => {
            assert!(cfg!(feature = "gc-memory-telemetry"));
            assert_eq!(reason, RegistrationError::Publication);
            eprintln!("NMJIT_N4_TELEMETRY_EXECUTION_ADMISSION_REFUSED_CLEAN");
            FixtureReceipt::TelemetryRefused
        }
    }
}

#[test]
fn nmjit_n4_activation_noop_has_total_teardown() {
    fixture(|admission, run, destination| {
        admission
            .with_activation(run, destination, |activation| {
                assert!(activation.table_view().table().slots().get() > 0);
            })
            .unwrap();
        assert_eq!(destination.word_for_test(run), TaggedValue::NIL.bits());
    });
}

#[test]
fn nmjit_n4_activation_commits_only_checked_immediates() {
    fixture(|admission, run, destination| {
        for result in [
            ImmediateResult::Nil,
            ImmediateResult::True,
            ImmediateResult::Fixnum(Fixnum::try_from(73).unwrap()),
            ImmediateResult::Fixnum(Fixnum::try_from(-97).unwrap()),
        ] {
            admission
                .with_activation(run, destination, |activation| {
                    let table = activation.table_view();
                    assert!(table.table().chunk_count().get() > 0);
                    drop(table);
                    activation.store_immediate(result)
                })
                .unwrap()
                .unwrap();
            assert_eq!(destination.word_for_test(run), result.word());
        }
    });
}

#[test]
fn nmjit_n4_activation_refuses_stale_home_before_body() {
    fixture(|admission, run, destination| {
        run.clear_root_slots().unwrap();
        let fresh = run.private_nil_root().unwrap();
        let result = admission.with_activation(run, destination, |_activation| -> () {
            panic!("stale result entered activation body")
        });
        assert_eq!(result, Err(RegistrationError::StaleRegistration));
        assert_eq!(fresh.word_for_test(run), TaggedValue::NIL.bits());
    });
}

#[test]
fn nmjit_n4_activation_unwind_keeps_run_and_root_usable() {
    fixture(|admission, run, destination| {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            admission
                .with_activation(run, destination, |activation| -> () {
                    activation.store_immediate(ImmediateResult::True).unwrap();
                    panic!("controlled activation callback exit");
                })
                .unwrap();
        }));
        assert!(result.is_err());
        assert_eq!(destination.word_for_test(run), TaggedValue::T.bits());
        admission
            .with_activation(run, destination, |activation| {
                activation.store_immediate(ImmediateResult::Nil)
            })
            .unwrap()
            .unwrap();
        assert_eq!(destination.word_for_test(run), TaggedValue::NIL.bits());
    });
}

#[test]
fn nmjit_n4_immediate_native_result_decode_is_total() {
    for value in [
        TaggedValue::NIL,
        TaggedValue::T,
        TaggedValue::fixnum(TaggedValue::MOST_NEGATIVE_FIXNUM),
        TaggedValue::fixnum(TaggedValue::MOST_POSITIVE_FIXNUM),
    ] {
        assert_eq!(
            ImmediateResult::from_word(value.bits()).unwrap().word(),
            value.bits()
        );
    }
    for bits in [TaggedValue::UNBOUND.bits(), 1, 3, 5, 7, usize::MAX] {
        assert_eq!(
            ImmediateResult::from_word(bits),
            Err(ImmediateResultError::NotImmediate)
        );
    }
    assert!(Fixnum::try_from(i64::MAX).is_err());
    assert!(Fixnum::try_from(i64::MIN).is_err());
}

#[test]
fn nmjit_n4_execution_production_only_source_guard_test() {
    let wrappers = include_str!("../wrappers.rs");
    let (production, proof) = wrappers
        .split_once("#[cfg(test)]\nimpl<'heap, K: EraKeeper> ProofEra<'heap, K>")
        .expect("distinct wrapper definitions");
    assert!(production.contains("impl<'heap, K: EraKeeper> ParallelHeapEra<'heap, K>"));
    assert!(production.contains("pub fn execution_admission"));
    assert!(!proof.contains("fn execution_admission"));
    let execution = include_str!("../execution.rs");
    assert!(!execution.contains("crate::emacs_core"));
    assert!(!execution.contains("generational_enabled"));
    assert!(!execution.contains("std::env"));
    assert!(execution.contains("_production: &Gen1Construction"));
}

#[test]
fn nmjit_n4_activation_borrows_publication_until_return() {
    fixture(|admission, run, home| {
        admission
            .with_activation(run, home, |activation| {
                let publication = activation.publication();
                let observed = publication.load();
                let checked = activation.table_view();
                assert!(std::ptr::eq(observed.table(), checked.table()));
                assert!(observed.table().slots().get() > 0);
            })
            .unwrap();
        assert_eq!(ExecutionTestHomes::read(home, run), ImmediateResult::Nil);
    });
}
