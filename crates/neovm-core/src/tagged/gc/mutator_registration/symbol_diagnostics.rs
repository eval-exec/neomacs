//! Small-World sealed-read and writer-admission diagnostics, never VM timing.
//! Ordinary cases are small semantic tests. Explicit cfg(test) measurement
//! mode changes only work/affinity, retains every assertion, and requires the
//! external island lock. There is no production setting or authority change.
mod affinity;
mod reads;
mod stopped;
mod types;

use super::symbol_functions::retry::{self as contention, Deadline, Retry, retry_owned};
use super::*;
use crate::emacs_core::eval::Context;
use crate::tagged::value::TaggedValue;
use std::ops::ControlFlow;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};
use types::*;

#[derive(Clone, Copy)]
enum Command {
    Go,
    Abort,
}
#[derive(Debug)]
enum Event {
    Armed(Ordinal),
    Finished(Ordinal),
    Failed(Ordinal),
}
struct Commands(Vec<Sender<Command>>);
impl Drop for Commands {
    fn drop(&mut self) {
        for sender in &self.0 {
            let _ = sender.send(Command::Abort);
        }
    }
}
fn receive<T>(receiver: &Receiver<T>) -> Result<T, Error> {
    contention::retry(
        Deadline::contention(),
        || receiver.try_recv(),
        |error| match error {
            TryRecvError::Empty => Retry::Busy,
            TryRecvError::Disconnected => Retry::Refuse,
        },
    )
    .map_err(|error| match error {
        TryRecvError::Empty => Error::Deadline,
        TryRecvError::Disconnected => Error::Channel,
    })
}
#[derive(Clone, Copy)]
enum Action {
    Run,
    Noop,
    Refuse(Ordinal),
    Unwind(Ordinal),
}
enum Home<'era> {
    Empty,
    Seeded(OwnerRootSlot<'era>),
}
struct Resident<'era> {
    owner: Mutator<'era>,
    home: Home<'era>,
}
enum Resources<'era> {
    Resident(Resident<'era>),
    Retired,
}
#[derive(Debug, PartialEq, Eq)]
enum Cleanup {
    Pending,
    Complete(Result<(), Error>),
}
#[must_use = "clear the canonical bank and retire before finishing the era"]
struct Guard<'era> {
    resources: Resources<'era>,
    cleanup: Rc<RefCell<Cleanup>>,
}
impl Drop for Guard<'_> {
    fn drop(&mut self) {
        let result = match std::mem::replace(&mut self.resources, Resources::Retired) {
            Resources::Retired => return,
            Resources::Resident(Resident { mut owner, home }) => (|| {
                drop(home);
                contention::retry(
                    Deadline::contention(),
                    || owner.run_access()?.clear_root_slots(),
                    contention::registration,
                )?;
                retry_owned(
                    Deadline::contention(),
                    owner,
                    |owner| owner.retire_rooted(&[]),
                    |failed| match contention::rendezvous(&failed.reason) {
                        Retry::Busy => ControlFlow::Continue(failed.into_owner()),
                        Retry::Refuse => ControlFlow::Break(failed),
                    },
                )
                .map_err(|failed| Error::Rendezvous(failed.reason))
            })(),
        };
        *self.cleanup.borrow_mut() = Cleanup::Complete(result);
    }
}
enum Receipt<T> {
    ActivationFailed(Error),
    Retired {
        body: thread::Result<Result<T, Error>>,
        cleanup: Cleanup,
    },
}
fn diagnostic_owner(permit: MutatorPermit<'_>) -> Result<Guard<'_>, Error> {
    let key = permit.key();
    let owner = retry_owned(
        Deadline::contention(),
        permit,
        |permit| permit.activate(key.heap_identity(), key.owner()),
        |failed| match contention::registration(&failed.reason) {
            Retry::Busy => ControlFlow::Continue(failed.into_permit()),
            Retry::Refuse => ControlFlow::Break(failed),
        },
    )
    .map_err(|failed| Error::Registration(failed.reason))?;
    Ok(Guard {
        resources: Resources::Resident(Resident {
            owner,
            home: Home::Empty,
        }),
        cleanup: Rc::new(RefCell::new(Cleanup::Pending)),
    })
}
fn worker<'era>(
    permit: MutatorPermit<'era>,
    symbol: SymbolHandle,
    ordinal: Ordinal,
    owners: Owners,
    workload: Workload,
    action: Action,
    mode: DiagnosticMode,
    active: &ActiveOwners,
    commands: Receiver<Command>,
    events: Sender<Event>,
) -> Receipt<ReadRecord> {
    // Spawn while the controller still has its broad allowance; the parent
    // pins itself only after every worker has inherited that original mask.
    let affinity = match affinity::worker(mode, ordinal) {
        Ok(guard) => guard,
        Err(error) => {
            drop(permit);
            let _ = events.send(Event::Failed(ordinal));
            return Receipt::ActivationFailed(error);
        }
    };
    let mut guard = match diagnostic_owner(permit) {
        Ok(guard) => guard,
        Err(error) => {
            let _ = events.send(Event::Failed(ordinal));
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
        // Publication completes BEFORE Armed; the callback can wait without
        // a run/symbol/root-reader loan. The wrapper restores Running itself.
        let _suppress = a8_probes::suppress();
        owner
            .with_native_extent(&[], |_| {
                events
                    .send(Event::Armed(ordinal))
                    .map_err(|_| Error::Channel)?;
                #[cfg(not(miri))]
                let command = commands
                    .recv_timeout(Duration::from_secs(60))
                    .map_err(|_| Error::Channel)?;
                #[cfg(miri)]
                let command = commands.recv().map_err(|_| Error::Channel)?;
                match command {
                    Command::Go => Ok(()),
                    Command::Abort => Err(Error::Aborted),
                }
            })?
            .result?;
        match action {
            Action::Run => {}
            Action::Noop => {
                let now = Instant::now();
                return Ok(ReadRecord {
                    owner: ordinal,
                    started: now,
                    ended: now,
                    reads: Reads(0),
                    checksum: Checksum(0),
                    run_busy: Busy(0),
                    read_busy: Busy(0),
                    writers: Vec::new(),
                });
            }
            Action::Refuse(target) if target == ordinal => return Err(Error::Aborted),
            Action::Unwind(target) if target == ordinal => {
                panic!("controlled diagnostic body unwind")
            }
            Action::Refuse(_) | Action::Unwind(_) => {}
        }
        match home {
            Home::Empty => Err(Error::Aborted),
            Home::Seeded(home) => reads::run(
                owner, home, &symbol, ordinal, owners, workload, mode, active,
            ),
        }
    }));
    let event = match &body {
        Ok(Ok(_)) => Event::Finished(ordinal),
        Ok(Err(_)) | Err(_) => Event::Failed(ordinal),
    };
    let _ = events.send(event);
    drop(commands);
    drop(guard);
    drop(affinity);
    let cleanup = std::mem::replace(&mut *cleanup.borrow_mut(), Cleanup::Pending);
    Receipt::Retired { body, cleanup }
}
fn census(era: &ParallelHeapEra<'_>) -> Collections {
    let mut stopped = contention::retry(
        Deadline::contention(),
        || era.stop(),
        contention::rendezvous,
    )
    .unwrap();
    let collections = era
        .with_stopped_heap_for_test(&stopped, |heap| Collections(heap.gc_collections()))
        .unwrap();
    stopped.trace(|_, _| Ok(())).unwrap();
    stopped.complete().unwrap();
    collections
}
fn scenario(backing: DiagnosticBacking, owners: Owners, workload: Workload, action: Action) {
    let mode = DiagnosticMode::load();
    Context::with_test_symbol_backing_era(backing.initial(), |era, id| {
        let handle = era.admit_plain_symbol(id).unwrap();
        let before = census(&era);
        let permits: Vec<_> = (0..owners.count())
            .map(|index| era.register(8000 + index as u64, 1).unwrap())
            .collect();
        let active = ActiveOwners::default();
        let (controller, receipts) = thread::scope(|scope| {
            let mut commands = Commands(Vec::new());
            let mut events = Vec::new();
            let mut jobs = Vec::new();
            for (index, permit) in permits.into_iter().enumerate() {
                let (send, receive_commands) = mpsc::channel();
                let (send_events, receive_events) = mpsc::channel();
                commands.0.push(send);
                events.push(receive_events);
                let active = &active;
                jobs.push(scope.spawn(move || {
                    worker(
                        permit,
                        handle,
                        Ordinal(index),
                        owners,
                        workload,
                        action,
                        mode,
                        active,
                        receive_commands,
                        send_events,
                    )
                }));
            }
            let controller = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                || -> Result<Duration, Error> {
                    let _affinity = affinity::controller(mode)?;
                    for (index, receiver) in events.iter().enumerate() {
                        match receive(receiver)? {
                            Event::Armed(ordinal) if ordinal == Ordinal(index) => {}
                            Event::Armed(_) | Event::Finished(_) | Event::Failed(_) => {
                                return Err(Error::Aborted);
                            }
                        }
                    }
                    let released = Instant::now();
                    for send in &commands.0 {
                        send.send(Command::Go).map_err(|_| Error::Channel)?;
                    }
                    for (index, receiver) in events.iter().enumerate() {
                        match receive(receiver)? {
                            Event::Finished(ordinal) | Event::Failed(ordinal)
                                if ordinal == Ordinal(index) => {}
                            Event::Armed(_) | Event::Finished(_) | Event::Failed(_) => {
                                return Err(Error::Aborted);
                            }
                        }
                    }
                    Ok(released.elapsed())
                },
            ));
            // Always release every native peer before joining, including a
            // pinning/activation/seed/controller error. No Barrier can strand it.
            drop(commands);
            let receipts: Vec<_> = jobs.into_iter().map(|job| job.join()).collect();
            (controller, receipts)
        });
        let after = census(&era);
        let finished = era.finish().unwrap();
        drop(finished);
        // Every semantic/timing assertion follows independent root teardown.
        assert_eq!(before, after, "reader/writer rows perform zero collections");
        assert_eq!(active.0.load(Ordering::Acquire), 0);
        let elapsed = controller
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
            .unwrap();
        let mut records = Vec::new();
        for (index, receipt) in receipts.into_iter().enumerate() {
            match receipt.unwrap_or_else(|panic| std::panic::resume_unwind(panic)) {
                Receipt::ActivationFailed(error) => panic!("diagnostic activation: {error:?}"),
                Receipt::Retired { body, cleanup } => {
                    assert_eq!(cleanup, Cleanup::Complete(Ok(())));
                    match action {
                        Action::Refuse(target) if target == Ordinal(index) => {
                            assert_eq!(body.unwrap().unwrap_err(), Error::Aborted)
                        }
                        Action::Unwind(target) if target == Ordinal(index) => {
                            let payload = body.expect_err("controlled unwind must remain visible");
                            assert_eq!(
                                payload.downcast_ref::<&str>().copied(),
                                Some("controlled diagnostic body unwind")
                            );
                        }
                        Action::Run | Action::Noop | Action::Refuse(_) | Action::Unwind(_) => {
                            records.push(
                                body.unwrap_or_else(|panic| std::panic::resume_unwind(panic))
                                    .unwrap(),
                            )
                        }
                    }
                }
            }
        }
        eprintln!(
            "P3SYM_C6_DIAGNOSTIC_TEARDOWN_OK mode={mode:?} backing={backing:?} owners={owners:?} workload={workload:?} collections=0 controller_ns={}",
            elapsed.as_nanos()
        );
        validate(mode, owners, workload, action, &records);
    });
}
fn validate(
    mode: DiagnosticMode,
    owners: Owners,
    workload: Workload,
    action: Action,
    records: &[ReadRecord],
) {
    let expected = match action {
        Action::Noop => Reads(0),
        Action::Run | Action::Refuse(_) | Action::Unwind(_) => {
            Reads(mode.reads().0 / owners.count())
        }
    };
    let earliest = records.iter().map(|row| row.started).min().unwrap();
    let latest = records.iter().map(|row| row.ended).max().unwrap();
    let skew = records
        .iter()
        .map(|row| row.started)
        .max()
        .unwrap()
        .duration_since(earliest);
    for row in records {
        assert_eq!(row.reads, expected);
        assert_eq!(
            row.checksum,
            Checksum(TaggedValue::fixnum(73).bits().wrapping_mul(expected.0))
        );
        let writer_count = match (workload, row.owner, action) {
            (Workload::PeriodicWriter, Ordinal(0), Action::Run) => expected.0 / mode.stride().0,
            (Workload::Readers, _, _) | (Workload::PeriodicWriter, _, _) => 0,
        };
        assert_eq!(row.writers.len(), writer_count);
        assert!(
            row.writers
                .iter()
                .all(|sample| sample.active_peers.0 < owners.count() as u32)
        );
        eprintln!(
            "P3SYM_C6_DIAGNOSTIC_OWNER owner={:?} reads={:?} checksum={:?} elapsed_ns={} start_offset_ns={} run_busy={:?} read_busy={:?} writer_samples={:?}",
            row.owner,
            row.reads,
            row.checksum,
            row.ended.duration_since(row.started).as_nanos(),
            row.started.duration_since(earliest).as_nanos(),
            row.run_busy,
            row.read_busy,
            row.writers
        );
        // Read these fields explicitly so a receipt cannot silently drop the
        // raw latency or refusal count in a future presentation refactor.
        for sample in &row.writers {
            std::hint::black_box((sample.wait, sample.busy));
        }
    }
    eprintln!(
        "P3SYM_C6_DIAGNOSTIC_ENVELOPE mode={mode:?} workload={workload:?} owners={owners:?} completed={} envelope_ns={} startup_skew_ns={} scope=sealed_read_root_install_checked_consumer_run_admission_included",
        records.iter().map(|row| row.reads.0).sum::<usize>(),
        latest.duration_since(earliest).as_nanos(),
        skew.as_nanos()
    );
}
macro_rules! row {
    ($name:ident,$backing:ident,$owners:ident,$workload:ident) => {
        #[test]
        fn $name() {
            scenario(
                DiagnosticBacking::$backing,
                Owners::$owners,
                Workload::$workload,
                Action::Run,
            );
        }
    };
}
row!(p3sym_c6_diagnostic_read_plain_one, Plain, One, Readers);
row!(p3sym_c6_diagnostic_read_plain_two, Plain, Two, Readers);
row!(p3sym_c6_diagnostic_read_plain_four, Plain, Four, Readers);
row!(
    p3sym_c6_diagnostic_read_projected_one,
    ProjectedPlain,
    One,
    Readers
);
row!(
    p3sym_c6_diagnostic_read_projected_two,
    ProjectedPlain,
    Two,
    Readers
);
row!(
    p3sym_c6_diagnostic_read_projected_four,
    ProjectedPlain,
    Four,
    Readers
);
row!(
    p3sym_c6_diagnostic_writer_plain_one,
    Plain,
    One,
    PeriodicWriter
);
row!(
    p3sym_c6_diagnostic_writer_plain_two,
    Plain,
    Two,
    PeriodicWriter
);
row!(
    p3sym_c6_diagnostic_writer_plain_four,
    Plain,
    Four,
    PeriodicWriter
);
row!(
    p3sym_c6_diagnostic_writer_projected_one,
    ProjectedPlain,
    One,
    PeriodicWriter
);
row!(
    p3sym_c6_diagnostic_writer_projected_two,
    ProjectedPlain,
    Two,
    PeriodicWriter
);
row!(
    p3sym_c6_diagnostic_writer_projected_four,
    ProjectedPlain,
    Four,
    PeriodicWriter
);
#[test]
fn p3sym_c6_diagnostic_noop_teardown() {
    scenario(
        DiagnosticBacking::ProjectedPlain,
        Owners::Four,
        Workload::Readers,
        Action::Noop,
    );
}
#[test]
fn p3sym_c6_diagnostic_refusal_teardown() {
    scenario(
        DiagnosticBacking::Plain,
        Owners::Two,
        Workload::Readers,
        Action::Refuse(Ordinal(1)),
    );
}
#[test]
fn p3sym_c6_diagnostic_unwind_teardown() {
    scenario(
        DiagnosticBacking::ProjectedPlain,
        Owners::Two,
        Workload::Readers,
        Action::Unwind(Ordinal(1)),
    );
}
