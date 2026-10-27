//! Real request -> native publication -> keeper collection -> owner response.
//! No reader/run loan crosses a wait. These are minimal-World diagnostics,
//! not continuous VM-reader drain or representative editor-heap timings.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Round {
    First,
    Second,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct StopEpoch(NonZeroU64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Response {
    NoCollection(StopEpoch),
    Collected(StopEpoch),
}
impl Response {
    fn epoch(self) -> u64 {
        match self {
            Self::NoCollection(epoch) => epoch.0.get(),
            Self::Collected(epoch) => epoch.0.get(),
        }
    }
}
#[derive(Clone, Copy, Debug)]
enum Instruction {
    Resume { round: Round, response: Response },
    Abort,
}
struct StopCommands(Vec<Sender<Instruction>>);
impl Drop for StopCommands {
    fn drop(&mut self) {
        for command in &self.0 {
            let _ = command.send(Instruction::Abort);
        }
    }
}
#[derive(Clone, Copy, Debug)]
enum Notice {
    Ready {
        round: Round,
        owner: Ordinal,
    },
    Published {
        round: Round,
        owner: Ordinal,
        epoch: StopEpoch,
        at: Instant,
    },
    Restored {
        round: Round,
        owner: Ordinal,
        epoch: Response,
    },
    Failed(Ordinal),
}
#[derive(Clone, Copy, Debug)]
enum Scenario {
    Collect,
    Noop,
    ControllerUnwindAfterFirst,
    RefuseBeforeRequest(Ordinal),
}
enum RequestState {
    Idle,
    Active(StopEpoch),
}
#[derive(Debug)]
struct Restored {
    round: Round,
    epoch: Response,
    word: Checksum,
    prologue: ReadRecord,
}
#[derive(Debug)]
enum Job {
    Collected(CollectionReport),
    Noop,
}
#[derive(Debug)]
struct StopRecord {
    round: Round,
    epoch: StopEpoch,
    request_started: Instant,
    all_stopped: Instant,
    collection: Duration,
    completion: Duration,
    publications: Vec<(Ordinal, Instant)>,
    job: Job,
}
fn check_abort(commands: &Receiver<Instruction>) -> Result<(), Error> {
    match commands.try_recv() {
        Ok(Instruction::Abort) => Err(Error::Aborted),
        Ok(Instruction::Resume { .. }) => Err(Error::Protocol),
        Err(TryRecvError::Empty) => Ok(()),
        Err(TryRecvError::Disconnected) => Err(Error::Channel),
    }
}
fn requested(owner: &Mutator<'_>, commands: &Receiver<Instruction>) -> Result<StopEpoch, Error> {
    let deadline = Deadline::contention();
    loop {
        check_abort(commands)?;
        match owner.requested_epoch() {
            Ok(Some(epoch)) => return Ok(StopEpoch(epoch)),
            Ok(None) => {}
            Err(error) => match contention::rendezvous(&error) {
                Retry::Busy => {}
                Retry::Refuse => return Err(error.into()),
            },
        }
        deadline.check(Error::Deadline)?;
        thread::yield_now();
    }
}
fn wait_native(commands: &Receiver<Instruction>) -> Result<Instruction, Error> {
    #[cfg(not(miri))]
    {
        commands
            .recv_timeout(Duration::from_secs(60))
            .map_err(|_| Error::Channel)
    }
    #[cfg(miri)]
    {
        commands.recv().map_err(|_| Error::Channel)
    }
}
fn worker(
    permit: MutatorPermit<'_>,
    symbol: SymbolHandle,
    ordinal: Ordinal,
    owners: Owners,
    mode: DiagnosticMode,
    scenario: Scenario,
    commands: Receiver<Instruction>,
    events: Sender<Notice>,
    active: &ActiveOwners,
) -> Receipt<Vec<Restored>> {
    let affinity = match affinity::worker(mode, ordinal) {
        Ok(guard) => guard,
        Err(error) => {
            drop(permit);
            let _ = events.send(Notice::Failed(ordinal));
            return Receipt::ActivationFailed(error);
        }
    };
    let mut guard = match diagnostic_owner(permit) {
        Ok(guard) => guard,
        Err(error) => {
            let _ = events.send(Notice::Failed(ordinal));
            return Receipt::ActivationFailed(error);
        }
    };
    let cleanup = guard.cleanup.clone();
    let body = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let Resident { owner, home } = match &mut guard.resources {
            Resources::Resident(resident) => resident,
            Resources::Retired => return Err(Error::Aborted),
        };
        *home = Home::Seeded(contention::retry(
            Deadline::contention(),
            || owner.run_access()?.private_nil_root(),
            contention::registration,
        )?);
        if let Scenario::RefuseBeforeRequest(target) = scenario {
            if target == ordinal {
                return Err(Error::Aborted);
            }
        }
        let home = match home {
            Home::Seeded(home) => home,
            Home::Empty => return Err(Error::Protocol),
        };
        let mut observations = Vec::new();
        for round in [Round::First, Round::Second] {
            let prologue = reads::run(
                owner,
                home,
                &symbol,
                ordinal,
                owners,
                Workload::Readers,
                mode,
                active,
            )?;
            events
                .send(Notice::Ready {
                    round,
                    owner: ordinal,
                })
                .map_err(|_| Error::Channel)?;
            let epoch = requested(owner, &commands)?;
            let _suppress = a8_probes::suppress();
            let returned = owner.with_native_extent(&[], |_| {
                events
                    .send(Notice::Published {
                        round,
                        owner: ordinal,
                        epoch,
                        at: Instant::now(),
                    })
                    .map_err(|_| Error::Channel)?;
                match wait_native(&commands)? {
                    Instruction::Resume {
                        round: expected,
                        response,
                    } if expected == round => Ok(response),
                    Instruction::Resume { .. } => Err(Error::Protocol),
                    Instruction::Abort => Err(Error::Aborted),
                }
            })?;
            let response = returned.result?;
            if returned.epoch != response.epoch() {
                return Err(Error::Protocol);
            }
            // Inspect the restored home BEFORE another read can overwrite it.
            let word = contention::retry(
                Deadline::contention(),
                || {
                    let run = owner.run_access()?;
                    Ok(Checksum(home.word_for_test(&run)))
                },
                contention::registration,
            )?;
            observations.push(Restored {
                round,
                epoch: response,
                word,
                prologue,
            });
            events
                .send(Notice::Restored {
                    round,
                    owner: ordinal,
                    epoch: response,
                })
                .map_err(|_| Error::Channel)?;
        }
        Ok(observations)
    }));
    if !matches!(&body, Ok(Ok(_))) {
        let _ = events.send(Notice::Failed(ordinal));
        // An unexpected failure may precede this owner's ACK. Publish it
        // before root-bank cleanup so the controller can finish an active
        // request; an Abort/disconnect returns through the canonical wrapper.
        if let Resources::Resident(Resident { owner, .. }) = &mut guard.resources {
            let _suppress = a8_probes::suppress();
            if let Err(error) = owner.with_native_extent(&[], |_| {
                let _ = wait_native(&commands);
            }) {
                eprintln!("P3SYM_C6_STOP_FAILURE_PUBLICATION owner={ordinal:?} error={error:?}");
            }
        }
    }
    drop(commands);
    drop(guard);
    drop(affinity);
    let cleanup = std::mem::replace(&mut *cleanup.borrow_mut(), Cleanup::Pending);
    Receipt::Retired { body, cleanup }
}
#[derive(Debug)]
enum StopPoll {
    Pending,
    Refused(RendezvousError),
}
fn stopped<'a>(era: &'a ParallelHeapEra<'_>) -> Result<AllStopped<'a>, Error> {
    contention::retry(
        Deadline::contention(),
        || match era.try_stop() {
            Ok(Some(stopped)) => Ok(stopped),
            Ok(None) => Err(StopPoll::Pending),
            Err(error) => Err(StopPoll::Refused(error)),
        },
        |error| match error {
            StopPoll::Pending => Retry::Busy,
            StopPoll::Refused(error) => contention::rendezvous(error),
        },
    )
    .map_err(|error| match error {
        StopPoll::Pending => Error::Deadline,
        StopPoll::Refused(error) => Error::Rendezvous(error),
    })
}
fn ready(events: &[Receiver<Notice>], round: Round) -> Result<(), Error> {
    for (index, events) in events.iter().enumerate() {
        match receive(events)? {
            Notice::Ready {
                round: actual,
                owner,
            } if actual == round && owner == Ordinal(index) => {}
            Notice::Ready { .. }
            | Notice::Published { .. }
            | Notice::Restored { .. }
            | Notice::Failed(_) => return Err(Error::Protocol),
        }
    }
    Ok(())
}
fn round(
    era: &ParallelHeapEra<'_>,
    round: Round,
    scenario: Scenario,
    events: &[Receiver<Notice>],
    commands: &StopCommands,
    state: &mut RequestState,
) -> Result<StopRecord, Error> {
    ready(events, round)?;
    let request_started = Instant::now();
    let epoch = match contention::retry(
        Deadline::contention(),
        || era.rendezvous().request(),
        contention::rendezvous,
    )? {
        Request::Active(epoch) => StopEpoch(epoch),
        Request::Queued => return Err(Error::Protocol),
    };
    *state = RequestState::Active(epoch);
    let mut publications = Vec::new();
    for (index, events) in events.iter().enumerate() {
        match receive(events)? {
            Notice::Published {
                round: actual,
                owner,
                epoch: observed,
                at,
            } if actual == round && owner == Ordinal(index) && observed == epoch => {
                publications.push((owner, at))
            }
            Notice::Ready { .. }
            | Notice::Published { .. }
            | Notice::Restored { .. }
            | Notice::Failed(_) => return Err(Error::Protocol),
        }
    }
    let mut stopped = stopped(era)?;
    let all_stopped = Instant::now();
    let actual_epoch = StopEpoch(stopped.epoch());
    // A traced collector error is terminal; never retry or pretend rollback.
    let collection_started = Instant::now();
    let (job, response) = match scenario {
        Scenario::Collect
        | Scenario::ControllerUnwindAfterFirst
        | Scenario::RefuseBeforeRequest(_) => {
            let report = era.collect_stopped(&mut stopped)?;
            (Job::Collected(report), Response::Collected(epoch))
        }
        Scenario::Noop => {
            stopped.trace(|_, _| Ok(()))?;
            (Job::Noop, Response::NoCollection(epoch))
        }
    };
    let collection = collection_started.elapsed();
    let completion_started = Instant::now();
    stopped
        .complete()
        .map_err(|failed| Error::Rendezvous(failed.reason))?;
    let completion = completion_started.elapsed();
    *state = RequestState::Idle;
    for sender in &commands.0 {
        sender
            .send(Instruction::Resume { round, response })
            .map_err(|_| Error::Channel)?;
    }
    for (index, events) in events.iter().enumerate() {
        match receive(events)? {
            Notice::Restored {
                round: actual,
                owner,
                epoch: observed,
            } if actual == round && owner == Ordinal(index) && observed == response => {}
            Notice::Ready { .. }
            | Notice::Published { .. }
            | Notice::Restored { .. }
            | Notice::Failed(_) => return Err(Error::Protocol),
        }
    }
    if actual_epoch != epoch {
        return Err(Error::Protocol);
    }
    Ok(StopRecord {
        round,
        epoch,
        request_started,
        all_stopped,
        collection,
        completion,
        publications,
        job,
    })
}
fn scenario(backing: DiagnosticBacking, owners: Owners, scenario: Scenario) {
    let mode = DiagnosticMode::load();
    Context::with_test_symbol_backing_era(backing.initial(), |era, id| {
        let symbol = era.admit_plain_symbol(id).unwrap();
        let before = census(&era);
        let permits: Vec<_> = (0..owners.count())
            .map(|index| era.register(9000 + index as u64, 1).unwrap())
            .collect();
        let active = ActiveOwners::default();
        let (controller, receipts, recovery) = thread::scope(|scope| {
            let mut commands = StopCommands(Vec::new());
            let mut events = Vec::new();
            let mut jobs = Vec::new();
            for (index, permit) in permits.into_iter().enumerate() {
                let (sender, receive_commands) = mpsc::channel();
                let (send_events, receiver) = mpsc::channel();
                commands.0.push(sender);
                events.push(receiver);
                let active = &active;
                jobs.push(scope.spawn(move || {
                    worker(
                        permit,
                        symbol,
                        Ordinal(index),
                        owners,
                        mode,
                        scenario,
                        receive_commands,
                        send_events,
                        active,
                    )
                }));
            }
            let mut state = RequestState::Idle;
            let controller = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                || -> Result<Vec<StopRecord>, Error> {
                    let _affinity = affinity::controller(mode)?;
                    let mut records = Vec::new();
                    for cycle in [Round::First, Round::Second] {
                        records.push(round(
                            &era, cycle, scenario, &events, &commands, &mut state,
                        )?);
                        if matches!(
                            (scenario, cycle),
                            (Scenario::ControllerUnwindAfterFirst, Round::First)
                        ) {
                            panic!("controlled stop diagnostic controller unwind");
                        }
                    }
                    Ok(records)
                },
            ));
            drop(commands);
            let recovery = match state {
                RequestState::Idle => Ok(()),
                RequestState::Active(epoch) => (|| {
                    let mut stopped = stopped(&era)?;
                    if stopped.epoch() != epoch.0 {
                        return Err(Error::Protocol);
                    }
                    stopped.trace(|_, _| Ok(()))?;
                    stopped
                        .complete()
                        .map_err(|failed| Error::Rendezvous(failed.reason))
                })(),
            };
            let receipts: Vec<_> = jobs.into_iter().map(|job| job.join()).collect();
            (controller, receipts, recovery)
        });
        let after = census(&era);
        let finished = era.finish().unwrap();
        drop(finished);
        assert_eq!(active.0.load(Ordering::Acquire), 0);
        assert_eq!(recovery, Ok(()));
        let expected_collections = match scenario {
            Scenario::Collect => 2,
            Scenario::Noop | Scenario::RefuseBeforeRequest(_) => 0,
            Scenario::ControllerUnwindAfterFirst => 1,
        };
        assert_eq!(after.0, before.0 + expected_collections);
        for receipt in receipts {
            match receipt.unwrap_or_else(|panic| std::panic::resume_unwind(panic)) {
                Receipt::ActivationFailed(error) => panic!("stop activation: {error:?}"),
                Receipt::Retired { body, cleanup } => {
                    assert_eq!(cleanup, Cleanup::Complete(Ok(())));
                    match scenario {
                        Scenario::Collect | Scenario::Noop => {
                            let restored = body
                                .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
                                .unwrap();
                            assert_eq!(restored.len(), 2);
                            for (observation, expected) in
                                restored.iter().zip([Round::First, Round::Second])
                            {
                                assert_eq!(observation.round, expected);
                                assert_eq!(
                                    observation.word,
                                    Checksum(TaggedValue::fixnum(73).bits())
                                );
                                assert_eq!(
                                    observation.prologue.reads,
                                    Reads(mode.reads().0 / owners.count())
                                );
                                assert_eq!(
                                    observation.prologue.checksum,
                                    Checksum(
                                        TaggedValue::fixnum(73)
                                            .bits()
                                            .wrapping_mul(observation.prologue.reads.0)
                                    )
                                );
                                match (scenario, observation.epoch) {
                                    (Scenario::Collect, Response::Collected(_))
                                    | (Scenario::Noop, Response::NoCollection(_)) => {}
                                    (Scenario::Collect, Response::NoCollection(_))
                                    | (Scenario::Noop, Response::Collected(_))
                                    | (Scenario::ControllerUnwindAfterFirst, _)
                                    | (Scenario::RefuseBeforeRequest(_), _) => {
                                        panic!("wrong response epoch")
                                    }
                                }
                            }
                        }
                        Scenario::ControllerUnwindAfterFirst | Scenario::RefuseBeforeRequest(_) => {
                            assert_eq!(body.unwrap().unwrap_err(), Error::Aborted)
                        }
                    }
                }
            }
        }
        let records = match scenario {
            Scenario::Collect | Scenario::Noop => controller
                .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
                .unwrap(),
            Scenario::ControllerUnwindAfterFirst => {
                let panic = controller.expect_err("controlled controller unwind");
                assert_eq!(
                    panic.downcast_ref::<&str>().copied(),
                    Some("controlled stop diagnostic controller unwind")
                );
                Vec::new()
            }
            Scenario::RefuseBeforeRequest(_) => {
                assert_eq!(controller.unwrap().unwrap_err(), Error::Protocol);
                Vec::new()
            }
        };
        for (index, record) in records.iter().enumerate() {
            assert_eq!(
                record.round,
                if index == 0 {
                    Round::First
                } else {
                    Round::Second
                }
            );
            if index > 0 {
                assert!(record.epoch.0 > records[index - 1].epoch.0);
            }
            assert_eq!(record.publications.len(), owners.count());
            match record.job {
                Job::Collected(report) => {
                    assert!(report.major);
                    assert_eq!(report.frozen_owners, owners.count());
                    assert_eq!(report.epoch, record.epoch.0.get());
                    assert_eq!(report.cycle, before.0 + index + 1);
                }
                Job::Noop => {}
            }
            eprintln!(
                "P3SYM_C6_DIAGNOSTIC_STOP mode={mode:?} backing={backing:?} owners={owners:?} round={:?} epoch={:?} drain_ns={} collector_call_ns={} complete_ns={} publications={:?} job={:?} scope=cooperative_publication_channel_drain_minimal_world",
                record.round,
                record.epoch,
                record
                    .all_stopped
                    .duration_since(record.request_started)
                    .as_nanos(),
                record.collection.as_nanos(),
                record.completion.as_nanos(),
                record
                    .publications
                    .iter()
                    .map(|(owner, at)| (*owner, at.duration_since(record.request_started)))
                    .collect::<Vec<_>>(),
                record.job
            );
        }
        eprintln!(
            "P3SYM_C6_DIAGNOSTIC_STOP_TEARDOWN_OK mode={mode:?} backing={backing:?} owners={owners:?} scenario={scenario:?} actual_full_jobs={expected_collections}"
        );
    });
}
macro_rules! row {
    ($name:ident,$backing:ident,$owners:ident) => {
        #[test]
        fn $name() {
            scenario(
                DiagnosticBacking::$backing,
                Owners::$owners,
                Scenario::Collect,
            );
        }
    };
}
row!(p3sym_c6_diagnostic_stop_plain_one, Plain, One);
row!(p3sym_c6_diagnostic_stop_plain_two, Plain, Two);
row!(p3sym_c6_diagnostic_stop_plain_four, Plain, Four);
row!(p3sym_c6_diagnostic_stop_projected_one, ProjectedPlain, One);
row!(p3sym_c6_diagnostic_stop_projected_two, ProjectedPlain, Two);
row!(
    p3sym_c6_diagnostic_stop_projected_four,
    ProjectedPlain,
    Four
);
#[test]
fn p3sym_c6_diagnostic_stop_noop_teardown() {
    scenario(
        DiagnosticBacking::ProjectedPlain,
        Owners::Four,
        Scenario::Noop,
    );
}
#[test]
fn p3sym_c6_diagnostic_stop_unwind_teardown() {
    scenario(
        DiagnosticBacking::ProjectedPlain,
        Owners::Two,
        Scenario::ControllerUnwindAfterFirst,
    );
}
#[test]
fn p3sym_c6_diagnostic_stop_refusal_teardown() {
    scenario(
        DiagnosticBacking::Plain,
        Owners::Two,
        Scenario::RefuseBeforeRequest(Ordinal(1)),
    );
}
