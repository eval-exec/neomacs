//! Real GEN1 execution with canonical mappers, unary callbacks and cons homes.
use super::*;
use crate::emacs_core::eval::{SubrEntry, register_global_subr_entry};
use crate::emacs_core::intern::intern;
use crate::tagged::gc::function_write_hooks::HofOperation;
use crate::tagged::gc::{HofScratch, HofSingleton, PreparedHofBuiltin, PreparedUnaryFunction};
use crate::tagged::header::{SubrDispatchKind, SubrFn, SubrFn2};

pub(crate) struct HofTestInputs<'era> {
    pub(crate) mapper: PreparedHofBuiltin,
    pub(crate) callback: PreparedUnaryFunction,
    pub(crate) expected: OwnerRootSlot<'era>,
    pub(crate) replacement: OwnerRootSlot<'era>,
    pub(crate) singleton: HofSingleton<'era>,
    pub(crate) scratch: HofScratch<'era>,
    pub(crate) callback_symbol: SymbolHandle,
    pub(crate) mapper_replacement: OwnerRootSlot<'era>,
}

pub(super) fn spelling(operation: HofOperation) -> &'static str {
    match operation {
        HofOperation::Mapc => "mapc",
        HofOperation::Mapcar => "mapcar",
    }
}

pub(super) fn builtin(operation: HofOperation) -> SubrEntry {
    let body: SubrFn2 = match operation {
        HofOperation::Mapc => crate::emacs_core::builtins::builtin_mapc_2,
        HofOperation::Mapcar => crate::emacs_core::builtins::builtin_mapcar_2,
    };
    SubrEntry {
        function: Some(SubrFn::A2(body)),
        min_args: 2,
        max_args: Some(2),
        dispatch_kind: SubrDispatchKind::Builtin,
        interactive_spec: None,
    }
}

#[derive(Debug)]
enum ReconcileError {
    Stop(RendezvousError),
    Function(FunctionError),
}
fn reconcile_retry(error: &ReconcileError) -> Retry {
    match error {
        ReconcileError::Stop(error) => contention::rendezvous(error),
        ReconcileError::Function(error) => contention::function(error),
    }
}

pub(super) enum FixtureParameters {
    Nullary,
    Unary(crate::emacs_core::intern::SymId),
}

pub(super) fn definition(parameter: FixtureParameters, answer: i64) -> TaggedValue {
    let parameters = match parameter {
        FixtureParameters::Unary(parameter) => {
            TaggedValue::cons(TaggedValue::from_sym_id(parameter), TaggedValue::NIL)
        }
        FixtureParameters::Nullary => TaggedValue::NIL,
    };
    let body = TaggedValue::cons(TaggedValue::fixnum(answer), TaggedValue::NIL);
    let args = TaggedValue::cons(parameters, body);
    TaggedValue::cons(TaggedValue::from_sym_id(intern("lambda")), args)
}

pub(crate) fn prepared_hof_fixture<P>(
    backing: PlainSymbolBacking,
    operation: HofOperation,
    prepare: impl FnOnce(&Gen1Admission<'_>) -> P,
    body: impl for<'era, 'owner> FnOnce(
        &P,
        &ParallelExecutionAdmission<'era>,
        &mut RunAccess<'owner, 'era>,
        &OwnerRootSlot<'era>,
        HofTestInputs<'era>,
    ),
) -> FixtureReceipt {
    // Use the actual fixed builtin bodies and their real canonical objects.
    // This is cold setup: no generic Context bootstrap or serial collection.
    for kind in [HofOperation::Mapc, HofOperation::Mapcar] {
        register_global_subr_entry(intern(spelling(kind)), builtin(kind));
    }
    let mapper_id = intern(spelling(operation));
    let canonical = TaggedValue::subr_from_sym_id(mapper_id);
    let parameter = intern("nmjit-native-hof-ordinary-parameter");
    let replacement = intern("nmjit-native-hof-replacement");
    let mapper_replacement = intern("nmjit-native-hof-mapper-replacement");
    let outcome = Context::try_with_test_symbol_backing_init_era(
        backing,
        |obarray, id| {
            obarray.seed_value_id(mapper_id, TaggedValue::NIL);
            obarray.set_symbol_function_id(mapper_id, canonical);
            obarray.seed_value_id(parameter, TaggedValue::NIL);
            let original = definition(FixtureParameters::Unary(parameter), 73);
            obarray.seed_value_id(id, original);
            obarray.set_symbol_function_id(id, original);
            obarray.seed_value_id(
                replacement,
                definition(FixtureParameters::Unary(parameter), 97),
            );
            obarray.seed_value_id(
                mapper_replacement,
                definition(FixtureParameters::Nullary, 123),
            );
        },
        |mut proof, id| {
            let frozen = proof
                .core
                .legacy_roots
                .take()
                .expect("rooted HOF setup evidence");
            let (mut collector, keeper) = proof.finish().expect("small HOF setup return");
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
                .expect("real rooted HOF GEN1 era");
            let mapper_key = era
                .admit_plain_symbol(mapper_id)
                .expect("canonical mapper handle");
            let callback_symbol = era.admit_plain_symbol(id).expect("unary callback handle");
            let replacement_key = era
                .admit_plain_symbol(replacement)
                .expect("replacement handle");
            let mapper_replacement_key = era
                .admit_plain_symbol(mapper_replacement)
                .expect("mapper replacement handle");
            let callback = era
                .prepare_unary_function_symbol(&callback_symbol)
                .expect("prepared unary callback");
            let mapper = era
                .prepare_hof_builtin(operation, &mapper_key)
                .expect("prepared canonical mapper");
            // The singleton and mapcar output allocate only from real detached
            // owner cons storage, never through the serial allocation path.
            era.start_cons_storage().expect("HOF owner cons storage");
            let admission = era.execution_admission();
            let mut owner = era
                .register(1, 16)
                .expect("HOF fixture registration")
                .activate(era.heap_identity(), 1)
                .expect("HOF fixture owner");
            // Keep RunAccess OUTSIDE the assertion-catching closure: even a
            // body/setup assertion must close its canonical region before the
            // run's Drop checks the original IN_SLICE exit obligation.
            let result = {
                let mut run = owner.run_access().expect("HOF fixture run");
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let destination = run.private_nil_root().expect("HOF result home");
                    let mut expected = run.private_nil_root().expect("HOF expected callback home");
                    run.sym_function_into(&callback_symbol, &mut expected)
                        .expect("root original callback");
                    let mut replacement_home =
                        run.private_nil_root().expect("HOF replacement home");
                    run.sym_value_into(&replacement_key, &mut replacement_home)
                        .expect("root replacement callback");
                    let mut mapper_replacement_home =
                        run.private_nil_root().expect("HOF mapper replacement home");
                    run.sym_value_into(&mapper_replacement_key, &mut mapper_replacement_home)
                        .expect("root mapper replacement");
                    let singleton = HofSingleton::new(
                        &mut run,
                        ImmediateResult::Fixnum(Fixnum::try_from(7).expect("small input fixnum")),
                    )
                    .expect("real rooted singleton input");
                    let scratch = HofScratch::new(&mut run).expect("resident HOF scratch");
                    let inputs = HofTestInputs {
                        mapper,
                        callback,
                        expected,
                        replacement: replacement_home,
                        singleton,
                        scratch,
                        callback_symbol,
                        mapper_replacement: mapper_replacement_home,
                    };
                    body(&prepared, &admission, &mut run, &destination, inputs);
                }));
                let closed = run.close_cons_region();
                drop(run);
                assert_eq!(closed, Ok(()), "HOF fixture closed canonical region");
                result
            };
            // Body locals/activation borrows are gone and the run is closed.
            // Clear all exact homes regardless of the body's result. A real
            // stopped collection, not a no-op trace, must drain the owner's
            // young backlinks before terminal image merging can succeed.
            owner
                .run_access()
                .and_then(|mut run| run.clear_root_slots())
                .expect("HOF fixture cleared roots");
            drop(admission);
            let _probes = crate::tagged::gc::mutator_registration::a8_probes::suppress();
            // This is terminal keeper-thread publication, NOT a blocking
            // worker/native body. The keeper drives and completes its own
            // stop synchronously, then retires this already-published owner.
            // with_native_extent's worker-only wait/restore contract is not
            // appropriate here: there is no blocked native body to resume.
            contention::retry(
                Deadline::contention(),
                || owner.enter_native(&[]),
                contention::rendezvous,
            )
            .expect("HOF fixture terminal publication");
            let mut stopped = contention::retry(
                Deadline::contention(),
                || era.stop(),
                contention::rendezvous,
            )
            .expect("HOF fixture stop");
            contention::retry(
                Deadline::contention(),
                || era.return_cons_storage(&stopped),
                contention::registration,
            )
            .expect("return HOF cons storage");
            contention::retry(
                Deadline::contention(),
                || {
                    era.reconcile_functions(&mut stopped.symbols().map_err(ReconcileError::Stop)?)
                        .map_err(ReconcileError::Function)
                },
                reconcile_retry,
            )
            .expect("HOF function reconciliation");
            let collection = contention::retry(
                Deadline::contention(),
                || era.collect_stopped(&mut stopped),
                contention::rendezvous,
            )
            .expect("HOF fixture real stopped collection");
            assert!(collection.major);
            assert_eq!(collection.frozen_owners, 1);
            retry_owned(
                Deadline::contention(),
                stopped,
                |stopped| stopped.complete(),
                |failure| match contention::rendezvous(&failure.reason) {
                    Retry::Busy => ControlFlow::Continue(failure.into_stopped()),
                    Retry::Refuse => ControlFlow::Break(failure),
                },
            )
            .expect("HOF fixture resume");
            let cleanup = retry_owned(
                Deadline::contention(),
                owner,
                |owner| owner.retire_rooted(&[]),
                |failure| match contention::rendezvous(&failure.reason) {
                    Retry::Busy => ControlFlow::Continue(failure.into_owner()),
                    Retry::Refuse => ControlFlow::Break(failure),
                },
            )
            .map_err(|failure| format!("HOF fixture retirement: {failure:?}"));
            eprintln!(
                "NMJIT_N4_HOF_CLEANUP_COLLECTION_DRAINED {}",
                collection.epoch
            );
            let (collector, keeper) = era.finish().expect("HOF fixture total finish");
            drop(collector);
            drop(keeper);
            drop(prepared);
            eprintln!("NMJIT_N4_HOF_EXECUTION_TEARDOWN_OK {operation:?} {backing:?}");
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
