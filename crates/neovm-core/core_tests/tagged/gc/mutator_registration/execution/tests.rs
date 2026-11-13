//! Real collector admission, resident owner roots and activation cleanup.
//!
//! Re-homed from value's pre-split `execution/tests` fixture: the file was
//! written against this crate's own `crate::emacs_core` and `crate::test_utils`
//! environment, so it belongs in core's test tree. The minimal retained World
//! is still built by eval's symbol-fixture body (peer ruling B1, the only
//! cross-crate route) through `try_with_test_symbol_backing_init_era`; the
//! proof era is then re-admitted as the production execution era, keeping the
//! sealed legacy-root census through the value TA2 fence.

use crate::tagged::gc::mutator_registration::era_fixtures::prepare_keeper_backing;
use neovm_eval::test_support::authority::try_with_test_symbol_backing_init_era;
use neovm_value::tagged::gc::{
    Gen1Admission, Gen1AdmissionError, ImmediateResult, ImmediateResultError, OwnerRootSlot,
    ParallelExecutionAdmission, ParallelHeapEra, PlainSymbolBacking, RegistrationError, RunAccess,
    SymbolHandle,
};
use neovm_value::tagged::value::{Fixnum, TaggedValue};
use neovm_value::test_support::authority::{
    Deadline, Retry, nil_root_for_test, publication_table_identity_for_test, rendezvous,
    retry_owned, table_shape_for_test, take_legacy_roots_for_test,
};
use std::ops::ControlFlow;

/// Pin the GEN1 heap mode for the fixture's minimal World and restore the
/// ambient setting on drop.
///
/// The execution admission this fixture re-admits is GEN1-only by design, and
/// `TaggedHeap::new` reads the mode at construction (value `tagged/gc.rs`
/// :1056). Pinning keeps the fixture deterministic when the surrounding run
/// asks for GEN0, the same mode-pin convention the GC tests use.
struct Gen1ModePin {
    previous: Option<std::ffi::OsString>,
}

impl Gen1ModePin {
    fn new() -> Self {
        let previous = std::env::var_os("NEOVM_GC_GENERATIONAL");
        // SAFETY: the lib-test target executes one test per process, and the
        // ambient value is restored before the test returns.
        unsafe { std::env::set_var("NEOVM_GC_GENERATIONAL", "1") };
        Self { previous }
    }
}

impl Drop for Gen1ModePin {
    fn drop(&mut self) {
        // SAFETY: single-threaded test process; see `new`.
        unsafe {
            match self.previous.take() {
                Some(value) => std::env::set_var("NEOVM_GC_GENERATIONAL", value),
                None => std::env::remove_var("NEOVM_GC_GENERATIONAL"),
            }
        }
    }
}

/// The seeded symbol state for [`prepared_cell_fixture`].
#[derive(Clone, Copy, Debug)]
pub(crate) enum FixtureSeed {
    Scalar,
    FunctionDefinition,
    WritableFunctionDefinition,
    WritableFunctionTwin,
    UnsupportedFunction,
}

/// How the prepared execution fixture finished.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FixtureReceipt {
    Gen1Completed,
    TelemetryRefused,
}

/// Build a real GEN1 execution era with one serially seeded symbol and run
/// `body` inside an activated owner, with total teardown independent of the
/// body's assertions.
///
/// The fixture requires the GEN1 heap the production admission exists for; a
/// GEN0 refusal is a hard panic (the pre-move fixture recorded it as a
/// receipt, and every caller of this helper asserted GEN1 completion anyway).
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
    // The admission below is GEN1-only; pin the mode before the fixture builds
    // its heap.
    let _gen1 = Gen1ModePin::new();
    // The minimal World has no Context, so install the one real boot hook set
    // the activation/read path consults. Idempotent: another fixture in this
    // test process may have published the same canonical set already.
    let _hooks = crate::emacs_core::eval_hook_installers::install_eval_hooks();
    // The small proof era is setup only. Its complete finish returns the
    // actual collector/keeper; a fresh production check then controls entry.
    // There is no ProofEra -> execution admission conversion or GEN0 bypass.
    let outcome = try_with_test_symbol_backing_init_era(
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
            let frozen = take_legacy_roots_for_test(&mut proof).expect("rooted setup evidence");
            let (collector, keeper) = proof.finish().expect("small setup return");
            let admitted = match Gen1Admission::check(collector) {
                Ok(admitted) => admitted,
                Err(failure) => {
                    let (reason, collector) = failure.into_parts();
                    drop(collector);
                    drop(frozen);
                    drop(keeper);
                    assert_eq!(reason, Gen1AdmissionError::GenerationalRequired);
                    panic!("the execution fixture requires the GEN1 heap");
                }
            };
            let prepared = prepare(&admitted);
            // SAFETY: the keeper is this minimal World's actual retained
            // keeper and `frozen` is its sealed legacy-root census, taken from
            // the proof era above; the prepare callback derives the retained
            // epoch/backing at the lower collector's preparation point.
            let era = unsafe {
                ParallelHeapEra::admit_rooted_with(admitted, keeper, frozen, prepare_keeper_backing)
            }
            .unwrap_or_else(|failure| {
                let (reason, collector, keeper) = failure.into_parts();
                drop(collector);
                drop(keeper);
                panic!("real rooted GEN1 era: {reason:?}");
            });
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
                let destination = nil_root_for_test(&mut run).expect("resident result home");
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
                    |failure| match rendezvous(&failure.reason) {
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

/// The no-seed scalar fixture the activation tests use.
fn fixture(
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

/// Scalar execution fixture with a caller-supplied admission preparation.
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

#[test]
fn nmjit_n4_activation_noop_has_total_teardown() {
    fixture(|admission, run, destination| {
        admission
            .with_activation(run, destination, |activation| {
                let (slots, _, _) = table_shape_for_test(activation);
                assert!(slots > 0);
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
                    let (_, _, chunk_count) = table_shape_for_test(activation);
                    assert!(chunk_count > 0);
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
        let fresh = nil_root_for_test(run).unwrap();
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
    let wrappers = include_str!(
        "../../../../../../neovm-value/src/tagged/gc/mutator_registration/wrappers.rs"
    );
    // The proof wrapper's test gate spelling is the split-era
    // `cfg(any(test, feature = "test-support"))`; the assertion below still
    // separates the production era impl from the test-only proof wrapper.
    let (production, proof) = wrappers
        .split_once("#[cfg(any(test, feature = \"test-support\"))]\nimpl<'heap, K: EraKeeper> ProofEra<'heap, K>")
        .expect("distinct wrapper definitions");
    assert!(production.contains("impl<'heap, K: EraKeeper> ParallelHeapEra<'heap, K>"));
    assert!(production.contains("pub fn execution_admission"));
    assert!(!proof.contains("fn execution_admission"));
    let execution = include_str!(
        "../../../../../../neovm-value/src/tagged/gc/mutator_registration/execution.rs"
    );
    assert!(!execution.contains("crate::emacs_core"));
    assert!(!execution.contains("tagged_heap_generational_enabled"));
    assert!(!execution.contains("std::env"));
    assert!(execution.contains("_production: &Gen1Construction"));
}

#[test]
fn nmjit_n4_activation_borrows_publication_until_return() {
    fixture(|admission, run, home| {
        admission
            .with_activation(run, home, |activation| {
                let (published, checked) = publication_table_identity_for_test(activation);
                let (slots, _, _) = table_shape_for_test(activation);
                assert_eq!(published, checked);
                assert!(slots > 0);
            })
            .unwrap();
        assert_eq!(
            ImmediateResult::from_word(home.word_for_test(run))
                .expect("scalar result home is immediate"),
            ImmediateResult::Nil
        );
    });
}
