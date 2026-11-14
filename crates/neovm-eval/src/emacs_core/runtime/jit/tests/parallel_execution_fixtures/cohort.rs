//! Owned compiled entries return before every published stop; no JIT grace receipt.
use crate::emacs_core::eval::subr_abi::SubrValueExt;
mod inputs;
use super::hof::{FixtureParameters, builtin, definition, spelling};
use super::*;
use crate::emacs_core::eval::register_global_subr_entry;
use crate::emacs_core::eval::world::ParallelHeapEraWorldExt;
use inputs::{CohortInputs, Scenario, ScenarioSeed};
pub(crate) use inputs::{PublicationFacts, PublicationInputs};
use neovm_value::intern::intern;
use neovm_value::tagged::gc::HofOperation;
use neovm_value::tagged::gc::{PreparedHofBuiltin, PreparedUnaryFunction};
use std::cell::RefCell;
use std::sync::mpsc::{self, Receiver, Sender};

#[derive(Clone, Copy, Debug)]
pub(crate) enum CohortSize {
    One,
    Two,
    Four,
}
impl CohortSize {
    pub(crate) fn count(self) -> usize {
        match self {
            Self::One => 1,
            Self::Two => 2,
            Self::Four => 4,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CohortPhase {
    Initial,
    AfterFirstCollection,
    AfterSecondCollection,
}
#[derive(Clone, Copy, Debug)]
enum Command {
    Resume,
    Retire,
    Abort,
}
#[derive(Debug)]
enum Event {
    Paused(usize, CohortPhase),
    Failed(usize, String),
}
#[derive(Debug)]
enum ReconcileError {
    Stop(RendezvousError),
    Function(FunctionError),
}
fn reconcile_retry(error: &ReconcileError) -> Retry {
    match error {
        ReconcileError::Stop(error) => rendezvous(error),
        ReconcileError::Function(error) => function(error),
    }
}
#[must_use = "release every waiting worker before joining the scoped cohort"]
struct Commands(Vec<Sender<Command>>);
impl Drop for Commands {
    fn drop(&mut self) {
        for sender in &self.0 {
            let _ = sender.send(Command::Abort);
        }
    }
}
fn receive<T>(receiver: &Receiver<T>) -> Result<T, String> {
    // Interpreter collection work may exceed the native wait budget. Keep
    // identical rendezvous/disconnection semantics without racing that clock,
    // as in the retained combined-collection fixture.
    #[cfg(not(miri))]
    {
        receiver
            .recv_timeout(SPIN_WAIT)
            .map_err(|error| format!("cohort wait: {error:?}"))
    }
    #[cfg(miri)]
    {
        receiver
            .recv()
            .map_err(|error| format!("cohort wait: {error:?}"))
    }
}
/// Wait on the product's resume-debt event, never on the body's outcome.
/// The predicate gate covers admission only and is released before the body;
/// concurrent native code, publication, and symbol writers remain concurrent.
fn with_run<'era, R>(
    owner: &mut Mutator<'era>,
    body: impl FnMut(&mut RunAccess<'_, 'era>) -> Result<R, String>,
) -> Result<R, String> {
    with_registry_admission_gate_for_test(owner, body)
}

fn seed<'era>(
    run: &mut RunAccess<'_, 'era>,
    mapper: PreparedHofBuiltin,
    callback: PreparedUnaryFunction,
    callback_symbol: SymbolHandle,
    replacement_key: &SymbolHandle,
    mapper_replacement_key: &SymbolHandle,
) -> Result<(OwnerRootSlot<'era>, HofTestInputs<'era>), String> {
    let destination = nil_root_for_test(run).map_err(|e| format!("destination: {e:?}"))?;
    let mut expected = nil_root_for_test(run).map_err(|e| format!("expected: {e:?}"))?;
    retry(
        Deadline::contention(),
        || run.sym_function_into(&callback_symbol, &mut expected),
        function,
    )
    .map_err(|e| format!("expected definition: {e:?}"))?;
    let mut replacement = nil_root_for_test(run).map_err(|e| format!("replacement: {e:?}"))?;
    retry(
        Deadline::contention(),
        || run.sym_value_into(replacement_key, &mut replacement),
        global,
    )
    .map_err(|e| format!("replacement value: {e:?}"))?;
    let mut mapper_replacement =
        nil_root_for_test(run).map_err(|e| format!("mapper replacement: {e:?}"))?;
    retry(
        Deadline::contention(),
        || run.sym_value_into(mapper_replacement_key, &mut mapper_replacement),
        global,
    )
    .map_err(|e| format!("mapper replacement value: {e:?}"))?;
    let singleton = hof_singleton_new_for_test(
        run,
        ImmediateResult::Fixnum(Fixnum::try_from(7).expect("small fixnum")),
    )
    .map_err(|e| format!("singleton: {e:?}"))?;
    let scratch = hof_scratch_new_for_test(run).map_err(|e| format!("scratch: {e:?}"))?;
    Ok((
        destination,
        HofTestInputs {
            mapper,
            callback,
            expected,
            replacement,
            singleton,
            scratch,
            callback_symbol,
            mapper_replacement,
        },
    ))
}

/// Small original fixture: no publication names, padding or extra root homes.
pub(crate) fn prepared_hof_cohort<P: Send>(
    backing: PlainSymbolBacking,
    operation: HofOperation,
    size: CohortSize,
    prepare: impl Fn(&Gen1Admission<'_>) -> P,
    work: impl for<'era, 'owner> Fn(
        &P,
        &ParallelExecutionAdmission<'era>,
        &mut RunAccess<'owner, 'era>,
        &OwnerRootSlot<'era>,
        &mut HofTestInputs<'era>,
        CohortPhase,
    ) -> Result<(), String>
    + Sync,
) -> FixtureReceipt {
    prepared_cohort(
        backing,
        operation,
        size,
        Scenario::Handoff,
        prepare,
        |code, admission, run, destination, inputs, phase| match inputs {
            CohortInputs::Handoff(inputs) => work(code, admission, run, destination, inputs, phase),
            CohortInputs::Publication(_) => Err("publication inputs in handoff fixture".into()),
        },
    )
}
/// Distinct publication history: operands exist before registry-only gaps.
pub(crate) fn prepared_publication_cohort<P: Send>(
    backing: PlainSymbolBacking,
    operation: HofOperation,
    size: CohortSize,
    prepare: impl Fn(&Gen1Admission<'_>) -> P,
    work: impl for<'era, 'owner> Fn(
        &P,
        &ParallelExecutionAdmission<'era>,
        &mut RunAccess<'owner, 'era>,
        &OwnerRootSlot<'era>,
        &mut PublicationInputs<'era>,
        CohortPhase,
    ) -> Result<(), String>
    + Sync,
) -> FixtureReceipt {
    prepared_cohort(
        backing,
        operation,
        size,
        Scenario::Publication,
        prepare,
        |code, admission, run, destination, inputs, phase| match inputs {
            CohortInputs::Publication(inputs) => {
                work(code, admission, run, destination, inputs, phase)
            }
            CohortInputs::Handoff(_) => Err("handoff inputs in publication fixture".into()),
        },
    )
}
/// Cold compilation returns owned values, one moved to each actual owner.
/// The keeper alone retains the era; workers receive no Context or heap loan.
fn prepared_cohort<P: Send>(
    backing: PlainSymbolBacking,
    operation: HofOperation,
    size: CohortSize,
    scenario: Scenario,
    prepare: impl Fn(&Gen1Admission<'_>) -> P,
    work: impl for<'era, 'owner> Fn(
        &P,
        &ParallelExecutionAdmission<'era>,
        &mut RunAccess<'owner, 'era>,
        &OwnerRootSlot<'era>,
        &mut CohortInputs<'era>,
        CohortPhase,
    ) -> Result<(), String>
    + Sync,
) -> FixtureReceipt {
    for kind in [HofOperation::Mapc, HofOperation::Mapcar] {
        register_global_subr_entry(intern(spelling(kind)), builtin(kind));
    }
    let mapper_id = intern(spelling(operation));
    let canonical = <TaggedValue as SubrValueExt>::subr_from_sym_id(mapper_id);
    let parameter = intern("nmjit-native-cohort-ordinary-parameter");
    let replacement_id = intern("nmjit-native-cohort-replacement");
    let mapper_replacement_id = intern("nmjit-native-cohort-mapper-replacement");
    let scenario_seed = RefCell::new(ScenarioSeed::Handoff);
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
                replacement_id,
                definition(FixtureParameters::Unary(parameter), 97),
            );
            obarray.seed_value_id(
                mapper_replacement_id,
                definition(FixtureParameters::Nullary, 123),
            );
            *scenario_seed.borrow_mut() = scenario.initialize(obarray, size);
        },
        |mut proof, id| {
            let frozen = take_legacy_roots_for_test(&mut proof).expect("cohort frozen setup roots");
            let (collector, keeper) = proof.finish().expect("cohort setup return");
            let admitted = match Gen1Admission::check(collector) {
                Ok(admitted) => admitted,
                Err(failed) => {
                    let (reason, collector) = failed.into_parts();
                    drop(collector);
                    drop(frozen);
                    drop(keeper);
                    assert_eq!(reason, Gen1AdmissionError::GenerationalRequired);
                    eprintln!("NMJIT_N4_GEN0_COHORT_ADMISSION_REFUSED_CLEAN");
                    return FixtureReceipt::Gen0Refused;
                }
            };
            let compiled = (0..size.count())
                .map(|_| prepare(&admitted))
                .collect::<Vec<_>>();
            let era = ParallelHeapEra::admit_rooted(admitted, keeper, frozen)
                .expect("real GEN1 cohort era");
            let mapper_key = era
                .admit_plain_symbol(mapper_id)
                .expect("cohort mapper key");
            let callback_symbol = era.admit_plain_symbol(id).expect("cohort callback key");
            let replacement_key = era
                .admit_plain_symbol(replacement_id)
                .expect("cohort replacement key");
            let mapper_replacement_key = era
                .admit_plain_symbol(mapper_replacement_id)
                .expect("cohort mapper replacement key");
            let callback = era_prepare_unary_function_symbol_for_test(&era, &callback_symbol)
                .expect("cohort unary preparation");
            let mapper = era_prepare_hof_builtin_for_test(&era, operation, &mapper_key)
                .expect("cohort builtin preparation");
            let prepared_inputs =
                std::mem::replace(&mut *scenario_seed.borrow_mut(), ScenarioSeed::Handoff)
                    .prepare(&era);
            era.start_cons_storage().expect("cohort cons storage");
            let admission = era.execution_admission();
            let permits = (0..size.count())
                .map(|index| {
                    era.register(index as u64 + 1, scenario.capacity())
                        .expect("cohort permit")
                })
                .collect::<Vec<_>>();
            let (controller, workers) = std::thread::scope(|scope| {
                let (events_tx, events_rx) = mpsc::channel();
                let mut commands = Commands(Vec::new());
                let mut joins = Vec::new();
                for (index, (code, permit)) in compiled.into_iter().zip(permits).enumerate() {
                    let (command_tx, command_rx) = mpsc::channel();
                    commands.0.push(command_tx);
                    let events = events_tx.clone();
                    let work = &work;
                    let prepared_inputs = &prepared_inputs;
                    let admission = &admission;
                    let heap = era.heap_identity();
                    joins.push(scope.spawn(move || -> Result<(), String> {
                        let mut owner = match retry_owned(
                            Deadline::contention(),
                            permit,
                            |permit| permit.activate(heap, index as u64 + 1),
                            |failed| match registration(&failed.reason) {
                                Retry::Busy => ControlFlow::Continue(failed.into_permit()),
                                Retry::Refuse => ControlFlow::Break(failed),
                            },
                        ) {
                            Ok(owner) => owner,
                            Err(failed) => {
                                let message = format!("cohort activation: {:?}", failed.reason);
                                drop(failed.into_permit());
                                let _ = events.send(Event::Failed(index, message.clone()));
                                return Err(message);
                            }
                        };
                        let mut frame = None;
                        let mut result = Ok(());
                        for phase in [
                            CohortPhase::Initial,
                            CohortPhase::AfterFirstCollection,
                            CohortPhase::AfterSecondCollection,
                        ] {
                            if result.is_ok() {
                                result = with_run(&mut owner, |run| {
                                    if frame.is_none() {
                                        let (destination, hof) = seed(
                                            run,
                                            mapper,
                                            callback,
                                            callback_symbol,
                                            &replacement_key,
                                            &mapper_replacement_key,
                                        )?;
                                        frame = Some((
                                            destination,
                                            prepared_inputs.seed(index, run, hof)?,
                                        ));
                                    }
                                    let (destination, inputs) =
                                        frame.as_mut().ok_or("cohort frame absent")?;
                                    work(&code, admission, run, destination, inputs, phase)
                                });
                            }
                            if matches!(phase, CohortPhase::AfterSecondCollection) {
                                drop(frame.take());
                                let cleared = with_run(&mut owner, |run| {
                                    run.clear_root_slots()
                                        .map_err(|error| format!("cohort clear roots: {error:?}"))
                                });
                                if let Err(error) = cleared {
                                    result =
                                        Err(format!("cohort clear: {error:?}; body={result:?}"));
                                }
                            }
                            let _probes = suppress_for_test();
                            let parked = owner.with_native_extent(&[], |_| {
                                events
                                    .send(Event::Paused(index, phase))
                                    .map_err(|error| format!("cohort pause event: {error:?}"))?;
                                receive(&command_rx)
                            });
                            match parked {
                                Ok(returned) => match returned.result {
                                    Ok(Command::Resume)
                                        if !matches!(phase, CohortPhase::AfterSecondCollection) => {
                                    }
                                    Ok(Command::Retire)
                                        if matches!(phase, CohortPhase::AfterSecondCollection) => {}
                                    Ok(Command::Resume | Command::Retire | Command::Abort) => {
                                        result = Err(format!(
                                            "cohort controller aborted; body={result:?}"
                                        ));
                                        break;
                                    }
                                    Err(error) => {
                                        result = Err(error);
                                        break;
                                    }
                                },
                                Err(error) => {
                                    result = Err(format!("cohort native extent: {error:?}"));
                                    break;
                                }
                            }
                        }
                        drop(frame);
                        let cleared = with_run(&mut owner, |run| {
                            run.clear_root_slots()
                                .map_err(|error| format!("cohort final clear roots: {error:?}"))
                        });
                        if let Err(error) = cleared {
                            result = Err(format!("cohort final clear: {error:?}; body={result:?}"));
                        }
                        let retired = retry_owned(
                            Deadline::contention(),
                            owner,
                            |owner| owner.retire_rooted(&[]),
                            |failed| match rendezvous(&failed.reason) {
                                Retry::Busy => ControlFlow::Continue(failed.into_owner()),
                                Retry::Refuse => ControlFlow::Break(failed),
                            },
                        );
                        if let Err(failed) = retired {
                            result = Err(format!(
                                "cohort retire: {:?}; body={result:?}",
                                failed.reason
                            ));
                        }
                        result
                    }));
                }
                drop(events_tx);
                let controller = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let mut previous = None;
                    for phase in [
                        CohortPhase::Initial,
                        CohortPhase::AfterFirstCollection,
                        CohortPhase::AfterSecondCollection,
                    ] {
                        let mut seen = Vec::new();
                        for _ in 0..size.count() {
                            match receive(&events_rx).expect("cohort owner publication") {
                                Event::Paused(index, observed) => {
                                    assert_eq!(observed, phase);
                                    seen.push(index);
                                }
                                Event::Failed(index, message) => {
                                    panic!("cohort owner {index}: {message}")
                                }
                            }
                        }
                        seen.sort_unstable();
                        seen.dedup();
                        assert_eq!(seen.len(), size.count());
                        let mut stopped = retry(Deadline::contention(), || era.stop(), rendezvous)
                            .expect("cohort stop");
                        retry(
                            Deadline::contention(),
                            || era.return_cons_storage(&stopped),
                            registration,
                        )
                        .expect("cohort return storage");
                        retry(
                            Deadline::contention(),
                            || {
                                era.reconcile_functions(
                                    &mut stopped.symbols().map_err(ReconcileError::Stop)?,
                                )
                                .map_err(ReconcileError::Function)
                            },
                            reconcile_retry,
                        )
                        .expect("cohort reconcile");
                        let report = retry(
                            Deadline::contention(),
                            || era.collect_stopped(&mut stopped),
                            rendezvous,
                        )
                        .expect("cohort collection");
                        assert!(report.major);
                        assert_eq!(report.frozen_owners, size.count());
                        if let Some((epoch, cycle)) = previous {
                            assert!(report.epoch > epoch);
                            assert!(report.cycle > cycle);
                        }
                        previous = Some((report.epoch, report.cycle));
                        if !matches!(phase, CohortPhase::AfterSecondCollection) {
                            retry(
                                Deadline::contention(),
                                || era.resume_cons_storage(&stopped),
                                registration,
                            )
                            .expect("cohort resume storage");
                        }
                        retry_owned(
                            Deadline::contention(),
                            stopped,
                            |stopped| stopped.complete(),
                            |failed| match rendezvous(&failed.reason) {
                                Retry::Busy => ControlFlow::Continue(failed.into_stopped()),
                                Retry::Refuse => ControlFlow::Break(failed),
                            },
                        )
                        .expect("cohort complete");
                        eprintln!(
                            "NMJIT_N4_COHORT_COLLECTION {size:?} {phase:?} epoch={} cycle={} owners={}",
                            report.epoch, report.cycle, report.frozen_owners
                        );
                        for sender in &commands.0 {
                            sender
                                .send(match phase {
                                    CohortPhase::Initial | CohortPhase::AfterFirstCollection => {
                                        Command::Resume
                                    }
                                    CohortPhase::AfterSecondCollection => Command::Retire,
                                })
                                .expect("cohort resume command");
                        }
                    }
                }));
                drop(commands);
                let workers = joins
                    .into_iter()
                    .map(|join| join.join())
                    .collect::<Vec<_>>();
                (controller, workers)
            });
            // Preserve the primary worker error even if terminal finish also
            // refuses: a secondary lifecycle diagnostic must not hide it.
            for (index, worker) in workers.iter().enumerate() {
                if let Ok(Err(error)) = worker {
                    eprintln!("NMJIT_N4_COHORT_WORKER_OUTCOME {index}: {error}");
                }
            }
            drop(admission);
            let (collector, keeper) = era.finish().expect("cohort finish after drained owners");
            drop(collector);
            drop(keeper);
            for worker in workers {
                match worker {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => panic!("cohort worker: {error}"),
                    Err(payload) => std::panic::resume_unwind(payload),
                }
            }
            if let Err(payload) = controller {
                std::panic::resume_unwind(payload);
            }
            eprintln!("NMJIT_N4_COHORT_TEARDOWN_OK {size:?} {operation:?} {backing:?}");
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
            eprintln!("NMJIT_N4_TELEMETRY_COHORT_ADMISSION_REFUSED_CLEAN");
            FixtureReceipt::TelemetryRefused
        }
    }
}
