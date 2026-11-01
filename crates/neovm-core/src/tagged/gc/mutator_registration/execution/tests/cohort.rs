//! Owned compiled entries return before every published stop; no JIT grace receipt.
use super::hof::{FixtureParameters, builtin, definition, spelling};
use super::*;
use crate::emacs_core::eval::register_global_subr_entry;
use crate::emacs_core::intern::intern;
use crate::tagged::gc::function_write_hooks::HofOperation;
use crate::tagged::gc::{PreparedHofBuiltin, PreparedUnaryFunction};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

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
        ReconcileError::Stop(error) => contention::rendezvous(error),
        ReconcileError::Function(error) => contention::function(error),
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
    receiver
        .recv_timeout(Duration::from_secs(60))
        .map_err(|error| format!("cohort wait: {error:?}"))
}
fn failure(payload: Box<dyn std::any::Any + Send>) -> String {
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|text| text.to_string()))
        .unwrap_or_else(|| "non-string cohort assertion".into())
}
fn seed<'era>(
    run: &mut RunAccess<'_, 'era>,
    mapper: PreparedHofBuiltin,
    callback: PreparedUnaryFunction,
    callback_symbol: SymbolHandle,
    replacement_key: &SymbolHandle,
    mapper_replacement_key: &SymbolHandle,
) -> Result<(OwnerRootSlot<'era>, HofTestInputs<'era>), String> {
    let destination = run
        .private_nil_root()
        .map_err(|e| format!("destination: {e:?}"))?;
    let mut expected = run
        .private_nil_root()
        .map_err(|e| format!("expected: {e:?}"))?;
    contention::retry(
        Deadline::contention(),
        || run.sym_function_into(&callback_symbol, &mut expected),
        contention::function,
    )
    .map_err(|e| format!("expected definition: {e:?}"))?;
    let mut replacement = run
        .private_nil_root()
        .map_err(|e| format!("replacement: {e:?}"))?;
    contention::retry(
        Deadline::contention(),
        || run.sym_value_into(replacement_key, &mut replacement),
        contention::global,
    )
    .map_err(|e| format!("replacement value: {e:?}"))?;
    let mut mapper_replacement = run
        .private_nil_root()
        .map_err(|e| format!("mapper replacement: {e:?}"))?;
    contention::retry(
        Deadline::contention(),
        || run.sym_value_into(mapper_replacement_key, &mut mapper_replacement),
        contention::global,
    )
    .map_err(|e| format!("mapper replacement value: {e:?}"))?;
    let singleton = HofSingleton::new(
        run,
        ImmediateResult::Fixnum(Fixnum::try_from(7).expect("small fixnum")),
    )
    .map_err(|e| format!("singleton: {e:?}"))?;
    let scratch = HofScratch::new(run).map_err(|e| format!("scratch: {e:?}"))?;
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

/// Cold compilation returns owned values, one moved to each actual owner.
/// The keeper alone retains the era; workers receive no Context or heap loan.
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
    for kind in [HofOperation::Mapc, HofOperation::Mapcar] {
        register_global_subr_entry(intern(spelling(kind)), builtin(kind));
    }
    let mapper_id = intern(spelling(operation));
    let canonical = TaggedValue::subr_from_sym_id(mapper_id);
    let parameter = intern("nmjit-native-cohort-ordinary-parameter");
    let replacement_id = intern("nmjit-native-cohort-replacement");
    let mapper_replacement_id = intern("nmjit-native-cohort-mapper-replacement");
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
        },
        |mut proof, id| {
            let frozen = proof
                .core
                .legacy_roots
                .take()
                .expect("cohort frozen setup roots");
            let (collector, keeper) = proof.finish().expect("cohort setup return");
            let admitted = match Gen1Admission::check(collector) {
                Ok(admitted) => admitted,
                Err(failed) => {
                    let reason = failed.reason;
                    drop(failed.collector);
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
            let callback = era
                .prepare_unary_function_symbol(&callback_symbol)
                .expect("cohort unary preparation");
            let mapper = era
                .prepare_hof_builtin(operation, &mapper_key)
                .expect("cohort builtin preparation");
            era.start_cons_storage().expect("cohort cons storage");
            let admission = era.execution_admission();
            let permits = (0..size.count())
                .map(|index| era.register(index as u64 + 1, 32).expect("cohort permit"))
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
                    let admission = &admission;
                    let heap = era.heap_identity();
                    joins.push(scope.spawn(move || -> Result<(), String> {
                        let mut owner = match retry_owned(
                            Deadline::contention(),
                            permit,
                            |permit| permit.activate(heap, index as u64 + 1),
                            |failed| match contention::registration(&failed.reason) {
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
                                match owner.run_access() {
                                    Ok(mut run) => {
                                        let body =
                                            std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                                                || -> Result<(), String> {
                                                    if frame.is_none() {
                                                        frame = Some(seed(
                                                            &mut run,
                                                            mapper,
                                                            callback,
                                                            callback_symbol,
                                                            &replacement_key,
                                                            &mapper_replacement_key,
                                                        )?);
                                                    }
                                                    let (destination, inputs) = frame
                                                        .as_mut()
                                                        .ok_or("cohort frame absent")?;
                                                    work(
                                                        &code,
                                                        admission,
                                                        &mut run,
                                                        destination,
                                                        inputs,
                                                        phase,
                                                    )
                                                },
                                            ));
                                        let closed = run.close_cons_region();
                                        drop(run);
                                        result = match body {
                                            Ok(result) => result,
                                            Err(payload) => Err(failure(payload)),
                                        };
                                        if let Err(error) = closed {
                                            result = Err(format!(
                                                "cohort close: {error:?}; body={result:?}"
                                            ));
                                        }
                                    }
                                    Err(error) => {
                                        result = Err(format!("cohort run admission: {error:?}"))
                                    }
                                }
                            }
                            if matches!(phase, CohortPhase::AfterSecondCollection) {
                                drop(frame.take());
                                let cleared = owner
                                    .run_access()
                                    .and_then(|mut run| run.clear_root_slots());
                                if let Err(error) = cleared {
                                    result =
                                        Err(format!("cohort clear: {error:?}; body={result:?}"));
                                }
                            }
                            let _probes = super::super::super::a8_probes::suppress();
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
                        let cleared = owner
                            .run_access()
                            .and_then(|mut run| run.clear_root_slots());
                        if let Err(error) = cleared {
                            result = Err(format!("cohort final clear: {error:?}; body={result:?}"));
                        }
                        let retired = retry_owned(
                            Deadline::contention(),
                            owner,
                            |owner| owner.retire_rooted(&[]),
                            |failed| match contention::rendezvous(&failed.reason) {
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
                        let mut stopped = contention::retry(
                            Deadline::contention(),
                            || era.stop(),
                            contention::rendezvous,
                        )
                        .expect("cohort stop");
                        contention::retry(
                            Deadline::contention(),
                            || era.return_cons_storage(&stopped),
                            contention::registration,
                        )
                        .expect("cohort return storage");
                        contention::retry(
                            Deadline::contention(),
                            || {
                                stopped
                                    .symbols()
                                    .map_err(ReconcileError::Stop)?
                                    .reconcile_functions()
                                    .map_err(ReconcileError::Function)
                            },
                            reconcile_retry,
                        )
                        .expect("cohort reconcile");
                        let report = contention::retry(
                            Deadline::contention(),
                            || era.collect_stopped(&mut stopped),
                            contention::rendezvous,
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
                            contention::retry(
                                Deadline::contention(),
                                || era.resume_cons_storage(&stopped),
                                contention::registration,
                            )
                            .expect("cohort resume storage");
                        }
                        retry_owned(
                            Deadline::contention(),
                            stopped,
                            |stopped| stopped.complete(),
                            |failed| match contention::rendezvous(&failed.reason) {
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
