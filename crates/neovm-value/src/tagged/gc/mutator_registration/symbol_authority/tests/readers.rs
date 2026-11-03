//! Metadata-only overlap: no stop or collection is requested while readers
//! are held. Polling uses the shared elapsed deadline; no blocking primitive
//! or native transition spans a live symbol loan. Disconnect aborts the peer.
use super::*;
use crate::tagged::gc::mutator_registration::symbol_functions::retry::{
    self as contention, Deadline, Retry, retry_owned,
};
use std::ops::ControlFlow;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};

#[derive(Clone, Copy)]
pub(super) enum Scenario {
    BothReaders,
    RefuseSecond,
}
#[derive(Clone, Copy)]
enum Role {
    ReadOverlap,
    ReadWithRefusingPeer,
    Refuse,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Message {
    Ready,
    Checked,
}
struct Endpoint {
    send: Sender<Message>,
    receive: Receiver<Message>,
}
#[derive(Debug, PartialEq, Eq)]
enum Error {
    Registration(RegistrationError),
    PeerClosed,
    Deadline,
    Phase,
    ControlledRefusal,
}
impl From<RegistrationError> for Error {
    fn from(error: RegistrationError) -> Self {
        Self::Registration(error)
    }
}
#[derive(Debug, PartialEq, Eq)]
struct PayloadWord(usize);
#[derive(Debug, PartialEq, Eq)]
struct ReaderCount(u64);
#[derive(Debug, PartialEq, Eq)]
enum WriterAttempt {
    Refused,
    Admitted,
}
#[derive(Debug, PartialEq, Eq)]
struct Observation {
    payload: PayloadWord,
    count: ReaderCount,
    writer: WriterAttempt,
}
enum WorkerReceipt {
    ActivationFailed(RegistrationError),
    Finished {
        body: std::thread::Result<Result<Observation, Error>>,
        cleanup: Result<(), RendezvousError>,
    },
}
fn exchange(endpoint: &Endpoint, expected: Message) -> Result<(), Error> {
    endpoint
        .send
        .send(expected)
        .map_err(|_| Error::PeerClosed)?;
    let message = contention::retry(
        Deadline::contention(),
        || endpoint.receive.try_recv(),
        |error| match error {
            TryRecvError::Empty => Retry::Busy,
            TryRecvError::Disconnected => Retry::Refuse,
        },
    )
    .map_err(|error| match error {
        TryRecvError::Empty => Error::Deadline,
        TryRecvError::Disconnected => Error::PeerClosed,
    })?;
    match (expected, message) {
        (Message::Ready, Message::Ready) | (Message::Checked, Message::Checked) => Ok(()),
        (Message::Ready, Message::Checked) | (Message::Checked, Message::Ready) => {
            Err(Error::Phase)
        }
    }
}
fn observe(
    owner: &mut Mutator<'_>,
    handle: &SymbolHandle,
    id: SymId,
    endpoint: &Endpoint,
    role: Role,
) -> Result<Observation, Error> {
    match role {
        Role::Refuse => return Err(Error::ControlledRefusal),
        Role::ReadOverlap | Role::ReadWithRefusingPeer => {}
    }
    contention::retry(
        Deadline::contention(),
        || {
            // Only these two acquisitions may return a retryable error. No
            // message or observation has happened on a refused acquisition.
            let mut access = owner.run_access()?;
            let reader = access.symbol_ref(handle)?;
            exchange(endpoint, Message::Ready)?;
            // SAFETY: this exact retained World is read under the real counted
            // lease, and only immediate diagnostic bits escape this scope.
            let payload = unsafe { reader.run.owner.registry.symbols.backing.probe(id) };
            let word = &reader.run.owner.registry.symbols.word;
            let count = ReaderCount(word.load(Ordering::Acquire));
            let writer = match word::writer_enter(word) {
                false => WriterAttempt::Refused,
                true => WriterAttempt::Admitted,
            };
            // Both snapshots and writer attempts finish before either reader
            // can drop. Channel failures are terminal and never replay Ready.
            exchange(endpoint, Message::Checked)?;
            Ok(Observation {
                payload: PayloadWord(payload),
                count,
                writer,
            })
        },
        |error| match error {
            Error::Registration(error) => contention::registration(error),
            Error::PeerClosed | Error::Deadline | Error::Phase | Error::ControlledRefusal => {
                Retry::Refuse
            }
        },
    )
}
pub(super) fn check(scenario: Scenario) {
    Context::with_test_symbol_era(|era, id| {
        let handle = era.admit_plain_symbol(id).unwrap();
        let heap = era.heap_identity();
        let permits = [era.register(1, 8).unwrap(), era.register(2, 8).unwrap()];
        let roles = match scenario {
            Scenario::BothReaders => [Role::ReadOverlap, Role::ReadOverlap],
            Scenario::RefuseSecond => [Role::ReadWithRefusingPeer, Role::Refuse],
        };
        let (left_tx, right_rx) = mpsc::channel();
        let (right_tx, left_rx) = mpsc::channel();
        let endpoints = [
            Endpoint {
                send: left_tx,
                receive: left_rx,
            },
            Endpoint {
                send: right_tx,
                receive: right_rx,
            },
        ];
        let workers = std::thread::scope(|scope| {
            let mut joins = Vec::new();
            for ((permit, endpoint), role) in permits.into_iter().zip(endpoints).zip(roles) {
                joins.push(scope.spawn(move || {
                    let key = permit.key();
                    let activation = retry_owned(
                        Deadline::contention(),
                        permit,
                        |permit| permit.activate(heap, key.owner()),
                        |failed| match contention::registration(&failed.reason) {
                            Retry::Busy => ControlFlow::Continue(failed.into_permit()),
                            Retry::Refuse => ControlFlow::Break(failed),
                        },
                    );
                    let mut owner = match activation {
                        Ok(owner) => owner,
                        Err(failed) => return WorkerReceipt::ActivationFailed(failed.reason),
                    };
                    let body = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        observe(&mut owner, &handle, id, &endpoint, role)
                    }));
                    // A refusal/unwind must disconnect the peer BEFORE any
                    // retirement contention; no endpoint clone remains here.
                    drop(endpoint);
                    // No bank home is taken by this fixture; both reader and
                    // RunAccess loans have dropped even on a body unwind.
                    let cleanup = retry_owned(
                        Deadline::contention(),
                        owner,
                        |owner| owner.retire_rooted(&[]),
                        |failed| match contention::rendezvous(&failed.reason) {
                            Retry::Busy => ControlFlow::Continue(failed.into_owner()),
                            Retry::Refuse => ControlFlow::Break(failed),
                        },
                    )
                    .map_err(|failed| failed.reason);
                    WorkerReceipt::Finished { body, cleanup }
                }));
            }
            joins
                .into_iter()
                .map(|join| join.join())
                .collect::<Vec<_>>()
        });
        let final_count = ReaderCount(era.registry().symbols.word.load(Ordering::Acquire));
        finish(era);
        assert_eq!(final_count, ReaderCount(0));
        for (worker, role) in workers.into_iter().zip(roles) {
            let receipt = worker.unwrap_or_else(|panic| std::panic::resume_unwind(panic));
            let body = match receipt {
                WorkerReceipt::ActivationFailed(error) => {
                    panic!("reader activation refused: {error:?}")
                }
                WorkerReceipt::Finished { body, cleanup } => {
                    assert_eq!(cleanup, Ok(()), "reader fixture independent retirement");
                    body.unwrap_or_else(|panic| std::panic::resume_unwind(panic))
                }
            };
            match role {
                Role::ReadOverlap => assert_eq!(
                    body,
                    Ok(Observation {
                        payload: PayloadWord(Value::fixnum(73).bits()),
                        count: ReaderCount(2),
                        writer: WriterAttempt::Refused,
                    })
                ),
                Role::ReadWithRefusingPeer => assert_eq!(body, Err(Error::PeerClosed)),
                Role::Refuse => assert_eq!(body, Err(Error::ControlledRefusal)),
            }
        }
        eprintln!("P3SYM_READER_OVERLAP_TEARDOWN_OK");
    });
}
