//! Eval-local cell/scalar execution fixture.
//!
//! Core's re-homed `prepared_cell_fixture` panics on a GEN0 refusal and
//! installs eval hooks through core. These tests stay in eval because the
//! compiled-code types they drive are `pub(crate)` here. The hook installer
//! lives in core, and eval cannot name it, so this copy omits that call.
//! A runtime failure from the missing hooks is reported as RED-with-owner;
//! the assertions themselves are unchanged.
//!
//! GEN1 is pinned the same way as core: the production admission is GEN1-only,
//! and `TaggedHeap::new` reads the mode at construction.

use super::FixtureReceipt;
use crate::emacs_core::eval::Context;
use crate::emacs_core::eval::world::{ParallelHeapEraWorldExt, WorldEra};
use neovm_value::intern::intern;
use neovm_value::tagged::gc::{
    Gen1Admission, Gen1AdmissionError, OwnerRootSlot, ParallelExecutionAdmission,
    PlainSymbolBacking, RegistrationError, RunAccess, SymbolHandle,
};
use neovm_value::tagged::value::TaggedValue;
use neovm_value::test_support::authority::{
    Deadline, Retry, nil_root_for_test, rendezvous, retry_owned, take_legacy_roots_for_test,
};
use std::ops::ControlFlow;

/// Pin the GEN1 heap mode for the fixture's minimal World and restore the
/// ambient setting on drop.
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

/// The seeded symbol state for [`with_test_prepared_cell_execution`].
#[derive(Clone, Copy, Debug)]
pub(crate) enum ExecutionTestSeed {
    Scalar,
    FunctionDefinition,
    WritableFunctionDefinition,
    WritableFunctionTwin,
    UnsupportedFunction,
}

/// Build a real GEN1 execution era with one serially seeded symbol and run
/// `body` inside an activated owner, with total teardown independent of the
/// body's assertions.
///
/// A GEN0 refusal is a hard panic: every caller of this helper asserts GEN1
/// completion. The returned receipt is `Gen1Completed` or `TelemetryRefused`.
pub(crate) fn with_test_prepared_cell_execution<P>(
    backing: PlainSymbolBacking,
    seed: ExecutionTestSeed,
    prepare: impl FnOnce(&Gen1Admission<'_>) -> P,
    body: impl for<'era, 'owner> FnOnce(
        &P,
        &ParallelExecutionAdmission<'era>,
        &mut RunAccess<'owner, 'era>,
        &OwnerRootSlot<'era>,
        &SymbolHandle,
    ),
) -> FixtureReceipt {
    let _gen1 = Gen1ModePin::new();
    let outcome = Context::try_with_test_symbol_backing_init_era(
        backing,
        |obarray, id| match seed {
            ExecutionTestSeed::Scalar => {}
            ExecutionTestSeed::FunctionDefinition
            | ExecutionTestSeed::WritableFunctionDefinition
            | ExecutionTestSeed::WritableFunctionTwin => {
                let lambda = TaggedValue::from_sym_id(intern("lambda"));
                let body = TaggedValue::cons(TaggedValue::fixnum(73), TaggedValue::NIL);
                let args = TaggedValue::cons(TaggedValue::NIL, body);
                let definition = TaggedValue::cons(lambda, args);
                obarray.seed_value_id(id, definition);
                obarray.set_symbol_function_id(id, definition);
                match seed {
                    ExecutionTestSeed::WritableFunctionTwin => {
                        let body = TaggedValue::cons(TaggedValue::fixnum(73), TaggedValue::NIL);
                        let args = TaggedValue::cons(TaggedValue::NIL, body);
                        let twin = TaggedValue::cons(lambda, args);
                        obarray.seed_value_id(id, twin);
                    }
                    ExecutionTestSeed::FunctionDefinition
                    | ExecutionTestSeed::WritableFunctionDefinition => {}
                    ExecutionTestSeed::Scalar | ExecutionTestSeed::UnsupportedFunction => {}
                }
            }
            ExecutionTestSeed::UnsupportedFunction => {
                obarray.set_symbol_function_id(id, TaggedValue::fixnum(73));
            }
        },
        |mut proof, id| {
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
            let era = WorldEra::admit_rooted(admitted, keeper, frozen).unwrap_or_else(|failure| {
                let (reason, collector, keeper) = failure.into_parts();
                drop(collector);
                drop(keeper);
                panic!("real rooted GEN1 era: {reason:?}");
            });
            let symbol = era.admit_plain_symbol(id).expect("activation Plain handle");
            match seed {
                ExecutionTestSeed::Scalar
                | ExecutionTestSeed::FunctionDefinition
                | ExecutionTestSeed::UnsupportedFunction => {}
                ExecutionTestSeed::WritableFunctionDefinition
                | ExecutionTestSeed::WritableFunctionTwin => {
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
                ExecutionTestSeed::Scalar
                | ExecutionTestSeed::FunctionDefinition
                | ExecutionTestSeed::UnsupportedFunction => {}
                ExecutionTestSeed::WritableFunctionDefinition
                | ExecutionTestSeed::WritableFunctionTwin => {
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

/// Scalar execution fixture with a caller-supplied admission preparation.
pub(crate) fn with_test_prepared_execution<P>(
    prepare: impl FnOnce(&Gen1Admission<'_>) -> P,
    body: impl for<'era, 'owner> FnOnce(
        &P,
        &ParallelExecutionAdmission<'era>,
        &mut RunAccess<'owner, 'era>,
        &OwnerRootSlot<'era>,
    ),
) -> FixtureReceipt {
    with_test_prepared_cell_execution(
        PlainSymbolBacking::Unprojected,
        ExecutionTestSeed::Scalar,
        prepare,
        |prepared, admission, run, destination, _symbol| {
            body(prepared, admission, run, destination);
        },
    )
}
