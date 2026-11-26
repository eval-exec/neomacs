use std::collections::VecDeque;

use super::*;

#[derive(Debug)]
enum FakeRead {
    Data(&'static [u8]),
    Empty,
    Eof,
    Failure(io::ErrorKind),
    Interrupted,
    Cancel(Arc<Cancellation>),
    Sleep(Duration),
}

#[derive(Debug)]
enum FakeWrite {
    Full,
    Partial(TransportByteCount),
    Zero,
    Failure(io::ErrorKind),
    CancelAfter(Arc<Cancellation>),
}

#[derive(Debug, Default)]
struct FakeTransport {
    reads: VecDeque<FakeRead>,
    writes: VecDeque<FakeWrite>,
    committed: Vec<Box<[u8]>>,
    resizes: Vec<TerminalSize>,
}

impl Transport for FakeTransport {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<TransportByteCount> {
        match self.reads.pop_front().unwrap_or(FakeRead::Empty) {
            FakeRead::Data(bytes) => {
                buffer[..bytes.len()].copy_from_slice(bytes);
                Ok(TransportByteCount(bytes.len()))
            }
            FakeRead::Empty => Err(io::ErrorKind::WouldBlock.into()),
            FakeRead::Eof => Ok(TransportByteCount(0)),
            FakeRead::Failure(kind) => Err(kind.into()),
            FakeRead::Interrupted => Err(io::ErrorKind::Interrupted.into()),
            FakeRead::Cancel(cancellation) => {
                cancellation.cancel();
                Err(io::ErrorKind::WouldBlock.into())
            }
            FakeRead::Sleep(duration) => {
                std::thread::sleep(duration);
                buffer[0] = b'.';
                Ok(TransportByteCount(1))
            }
        }
    }

    fn write(&mut self, bytes: &[u8]) -> io::Result<TransportByteCount> {
        let length = match self.writes.pop_front().unwrap_or(FakeWrite::Full) {
            FakeWrite::Full => bytes.len(),
            FakeWrite::Partial(length) => length.as_usize(),
            FakeWrite::Zero => 0,
            FakeWrite::Failure(kind) => return Err(kind.into()),
            FakeWrite::CancelAfter(cancellation) => {
                cancellation.cancel();
                bytes.len()
            }
        };
        if length != 0 {
            self.committed.push(bytes[..length].into());
        }
        Ok(TransportByteCount(length))
    }

    fn resize(&mut self, size: TerminalSize) -> io::Result<()> {
        self.resizes.push(size);
        Ok(())
    }

    fn wait(&mut self, _: Duration) -> io::Result<Readiness> {
        Ok(Readiness::Idle)
    }

    fn wait_writable(&mut self, _: Duration) -> io::Result<WriteReadiness> {
        Ok(WriteReadiness::Ready)
    }
}

fn owner(transport: FakeTransport) -> (Owner<FakeTransport>, Receiver<PtyOutputEvent>) {
    let (sender, receiver) = mpsc::channel();
    (
        Owner {
            transport,
            sender,
            sequence: SequenceIssuer::default(),
            read_state: ReadState::Open,
        },
        receiver,
    )
}

fn request(action: Action) -> Request {
    Request {
        id: InputRequestId(NonZeroU64::new(1).unwrap()),
        action,
        cancellation: Arc::new(Cancellation::default()),
        deadline: Instant::now() + Duration::from_secs(10),
    }
}

fn collected(receiver: &Receiver<PtyOutputEvent>) -> Vec<PtyOutputEvent> {
    let events: Vec<_> = receiver.try_iter().collect();
    let mut cursor = ObservationCursor::default();
    for event in &events {
        cursor.accept(event.sequence()).unwrap();
    }
    for pair in events.windows(2) {
        assert!(pair[0].observed_at() <= pair[1].observed_at());
    }
    events
}

#[test]
fn already_available_output_precedes_the_committed_input_fence() {
    let (mut owner, receiver) = owner(FakeTransport {
        reads: [
            FakeRead::Data(b"\x1b[31mold"),
            FakeRead::Data(b" text"),
            FakeRead::Empty,
        ]
        .into(),
        ..Default::default()
    });
    owner
        .perform(
            request(Action::Input(b"key".as_slice().into())),
            &Cancellation::default(),
        )
        .unwrap();
    owner
        .transport
        .reads
        .push_back(FakeRead::Data(b" response"));
    owner
        .drain_available(Instant::now() + Duration::from_secs(1))
        .unwrap();
    let events = collected(&receiver);
    assert!(
        matches!(events[0].kind(), PtyOutputKind::Data(bytes) if bytes.as_ref() == b"\x1b[31mold")
    );
    assert!(matches!(events[1].kind(), PtyOutputKind::Data(bytes) if bytes.as_ref() == b" text"));
    assert!(
        matches!(events[2].kind(), PtyOutputKind::InputCommitted {bytes,..} if bytes.as_ref() == b"key")
    );
    assert!(matches!(
        events[3].kind(),
        PtyOutputKind::Completed { result: Ok(_), .. }
    ));
    assert!(
        matches!(events[4].kind(), PtyOutputKind::Data(bytes) if bytes.as_ref() == b" response")
    );
    let mut parser = vt100::Parser::new(2, 30, 0);
    for event in events.iter().take(2) {
        if let PtyOutputKind::Data(bytes) = event.kind() {
            parser.process(bytes);
        }
    }
    assert!(parser.screen().contents().starts_with("old text"));
}

#[test]
fn short_writes_continue_across_would_block_and_interleaved_output() {
    let (mut owner, receiver) = owner(FakeTransport {
        reads: [
            FakeRead::Empty,
            FakeRead::Data(b"mid"),
            FakeRead::Empty,
            FakeRead::Empty,
        ]
        .into(),
        writes: [
            FakeWrite::Partial(TransportByteCount(2)),
            FakeWrite::Failure(io::ErrorKind::WouldBlock),
            FakeWrite::Partial(TransportByteCount(1)),
            FakeWrite::Full,
        ]
        .into(),
        ..Default::default()
    });
    owner
        .perform(
            request(Action::Input(b"four".as_slice().into())),
            &Cancellation::default(),
        )
        .unwrap();
    let events = collected(&receiver);
    assert_eq!(
        owner
            .transport
            .committed
            .iter()
            .map(AsRef::as_ref)
            .collect::<Vec<_>>(),
        vec![b"fo".as_slice(), b"u".as_slice(), b"r".as_slice()]
    );
    assert!(
        matches!(events[0].kind(), PtyOutputKind::InputCommitted { bytes, .. } if bytes.as_ref() == b"fo")
    );
    assert!(matches!(events[1].kind(), PtyOutputKind::Data(bytes) if bytes.as_ref() == b"mid"));
    assert!(
        matches!(events[2].kind(), PtyOutputKind::InputCommitted { bytes, .. } if bytes.as_ref() == b"u")
    );
    assert!(
        matches!(events[3].kind(), PtyOutputKind::InputCommitted { bytes, .. } if bytes.as_ref() == b"r")
    );
    assert!(matches!(
        events[4].kind(),
        PtyOutputKind::Completed {
            result: Ok(InputReceipt {
                committed: InputByteCount(4),
                ..
            }),
            ..
        }
    ));
}

#[test]
fn a_partial_write_followed_by_failure_records_only_its_committed_prefix() {
    for (next_write, expected_kind) in [
        (
            FakeWrite::Failure(io::ErrorKind::BrokenPipe),
            Some(io::ErrorKind::BrokenPipe),
        ),
        (
            FakeWrite::Failure(io::ErrorKind::Other),
            Some(io::ErrorKind::Other),
        ),
        (FakeWrite::Zero, None),
    ] {
        let (mut owner, receiver) = owner(FakeTransport {
            writes: [FakeWrite::Partial(TransportByteCount(2)), next_write].into(),
            ..Default::default()
        });
        assert!(
            owner
                .perform(
                    request(Action::Input(b"four".as_slice().into())),
                    &Cancellation::default()
                )
                .is_err()
        );
        let events = collected(&receiver);
        assert_eq!(owner.transport.committed.len(), 1);
        assert_eq!(owner.transport.committed[0].as_ref(), b"fo");
        assert!(
            matches!(events[0].kind(), PtyOutputKind::InputCommitted { bytes, .. } if bytes.as_ref() == b"fo")
        );
        match (events[1].kind(), expected_kind) {
            (
                PtyOutputKind::Completed {
                    result:
                        Err(TransportError::Input {
                            committed: InputByteCount(2),
                            requested: InputByteCount(4),
                            reason: InputFailure::Io(error),
                        }),
                    ..
                },
                Some(kind),
            ) => assert_eq!(error.kind(), kind),
            (
                PtyOutputKind::Completed {
                    result:
                        Err(TransportError::Input {
                            committed: InputByteCount(2),
                            requested: InputByteCount(4),
                            reason: InputFailure::ZeroWrite,
                        }),
                    ..
                },
                None,
            ) => {}
            (other, _) => panic!("unexpected completion after partial write: {other:?}"),
        }
    }
}

#[test]
fn output_eof_after_partial_write_reports_committed_prefix_and_broken_pipe() {
    let (mut owner, receiver) = owner(FakeTransport {
        reads: [FakeRead::Empty, FakeRead::Eof].into(),
        writes: [FakeWrite::Partial(TransportByteCount(2)), FakeWrite::Full].into(),
        ..Default::default()
    });
    assert!(
        owner
            .perform(
                request(Action::Input(b"four".as_slice().into())),
                &Cancellation::default()
            )
            .is_err()
    );
    let events = collected(&receiver);
    assert_eq!(owner.transport.committed.len(), 1);
    assert_eq!(owner.transport.committed[0].as_ref(), b"fo");
    assert!(
        matches!(events[0].kind(), PtyOutputKind::InputCommitted { bytes, .. } if bytes.as_ref() == b"fo")
    );
    assert!(matches!(events[1].kind(), PtyOutputKind::OutputClosed));
    assert!(matches!(
        events[2].kind(),
        PtyOutputKind::Completed {
            result: Err(TransportError::Input {
                committed: InputByteCount(2),
                requested: InputByteCount(4),
                reason: InputFailure::Io(error),
            }),
            ..
        } if error.kind() == io::ErrorKind::BrokenPipe
    ));
}

#[test]
fn partial_write_then_drain_deadline_preserves_committed_prefix() {
    let mut req = request(Action::Input(b"four".as_slice().into()));
    req.deadline = Instant::now() + Duration::from_millis(25);
    let (mut owner, receiver) = owner(FakeTransport {
        reads: [
            FakeRead::Empty,
            FakeRead::Sleep(Duration::from_millis(40)),
            FakeRead::Data(b"late"),
        ]
        .into(),
        writes: [FakeWrite::Partial(TransportByteCount(2)), FakeWrite::Full].into(),
        ..Default::default()
    });
    assert!(owner.perform(req, &Cancellation::default()).is_err());
    let events = collected(&receiver);
    assert_eq!(owner.transport.committed.len(), 1);
    assert_eq!(owner.transport.committed[0].as_ref(), b"fo");
    assert!(
        matches!(events[0].kind(), PtyOutputKind::InputCommitted { bytes, .. } if bytes.as_ref() == b"fo")
    );
    assert!(matches!(
        events.last().unwrap().kind(),
        PtyOutputKind::Completed {
            result: Err(TransportError::Input {
                committed: InputByteCount(2),
                requested: InputByteCount(4),
                reason: InputFailure::Deadline,
            }),
            ..
        }
    ));
}

#[test]
fn write_all_nonblocking_pty_handles_short_writes_eagain_and_timeout() {
    use std::os::fd::{FromRawFd as _, OwnedFd};

    let mut fds = [-1; 2];
    // SAFETY: valid 2-element array for pipe(2).
    assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
    // SAFETY: pipe(2) succeeded and returned two distinct owned fds.
    let read_fd = unsafe { OwnedFd::from_raw_fd(fds[0]) };
    let write_fd = unsafe { OwnedFd::from_raw_fd(fds[1]) };
    set_nonblocking(write_fd.as_fd()).unwrap();
    let mut reader = std::fs::File::from(read_fd);
    let mut writer = std::fs::File::from(write_fd);

    // 1. Large payload (>256 KiB) exceeds pipe buffer -> exercises short writes + EAGAIN + POLLOUT.
    let payload: Vec<u8> = (0..300_000usize).map(|i| (i % 251) as u8).collect();
    let expected_len = payload.len();
    let drain_thread = std::thread::spawn(move || {
        let mut received = Vec::with_capacity(expected_len);
        let mut buf = [0u8; 4096];
        while received.len() < expected_len {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => received.extend_from_slice(&buf[..n]),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => panic!("read pipe: {error}"),
            }
        }
        (received, reader)
    });
    write_all_nonblocking_pty(
        &mut writer,
        &payload,
        Instant::now() + Duration::from_secs(5),
    )
    .expect("looping non-blocking write must deliver full payload");
    let (received, reader) = drain_thread.join().unwrap();
    assert_eq!(received, payload);

    // 2. Undrained pipe with a short deadline must fail with TimedOut rather than hanging.
    let huge = vec![b'z'; 2 * 1024 * 1024];
    let err = write_all_nonblocking_pty(
        &mut writer,
        &huge,
        Instant::now() + Duration::from_millis(60),
    )
    .expect_err("undrained full non-blocking fd must time out");
    assert_eq!(err.kind(), io::ErrorKind::TimedOut);

    // 3. Closed peer must fail immediately with BrokenPipe.
    drop(reader);
    let err = write_all_nonblocking_pty(&mut writer, b"x", Instant::now() + Duration::from_secs(1))
        .expect_err("closed peer must fail with BrokenPipe");
    assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
}

#[test]
fn zero_and_failed_writes_never_fabricate_input_events() {
    for write in [
        FakeWrite::Zero,
        FakeWrite::Failure(io::ErrorKind::BrokenPipe),
    ] {
        let (mut owner, receiver) = owner(FakeTransport {
            writes: [write].into(),
            ..Default::default()
        });
        assert!(
            owner
                .perform(
                    request(Action::Input(b"key".as_slice().into())),
                    &Cancellation::default()
                )
                .is_err()
        );
        let events = collected(&receiver);
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0].kind(),
            PtyOutputKind::Completed {
                result: Err(TransportError::Input {
                    committed: InputByteCount(0),
                    ..
                }),
                ..
            }
        ));
        assert!(owner.transport.committed.is_empty());
    }
}

#[test]
fn would_block_and_interrupted_writes_retry_without_duplicate_commit() {
    let (mut owner, receiver) = owner(FakeTransport {
        reads: [FakeRead::Interrupted, FakeRead::Empty].into(),
        writes: [
            FakeWrite::Failure(io::ErrorKind::WouldBlock),
            FakeWrite::Failure(io::ErrorKind::Interrupted),
            FakeWrite::Full,
        ]
        .into(),
        ..Default::default()
    });
    owner
        .perform(
            request(Action::Input(b"key".as_slice().into())),
            &Cancellation::default(),
        )
        .unwrap();
    assert_eq!(owner.transport.committed.len(), 1);
    assert_eq!(owner.transport.committed[0].as_ref(), b"key");
    let events = collected(&receiver);
    assert_eq!(events.len(), 2);
    assert!(matches!(
        events[1].kind(),
        PtyOutputKind::Completed {
            result: Ok(InputReceipt {
                committed: InputByteCount(3),
                ..
            }),
            ..
        }
    ));
}

#[test]
fn cancelled_before_write_has_no_committed_prefix() {
    let (mut owner, receiver) = owner(FakeTransport::default());
    let request = request(Action::Input(b"key".as_slice().into()));
    request.cancellation.cancel();
    assert!(owner.perform(request, &Cancellation::default()).is_err());
    assert!(owner.transport.committed.is_empty());
    assert!(matches!(
        collected(&receiver)[0].kind(),
        PtyOutputKind::Completed {
            result: Err(TransportError::Cancelled),
            ..
        }
    ));
}

#[test]
fn cancelled_after_write_retains_the_complete_committed_prefix() {
    let request = request(Action::Input(b"key".as_slice().into()));
    let (mut owner, receiver) = owner(FakeTransport {
        writes: [FakeWrite::CancelAfter(Arc::clone(&request.cancellation))].into(),
        ..Default::default()
    });
    assert!(owner.perform(request, &Cancellation::default()).is_err());
    let events = collected(&receiver);
    assert!(
        matches!(events[0].kind(), PtyOutputKind::InputCommitted{bytes,..} if bytes.as_ref() == b"key")
    );
    assert!(matches!(
        events[1].kind(),
        PtyOutputKind::Completed {
            result: Err(TransportError::Input {
                committed: InputByteCount(3),
                reason: InputFailure::Cancelled,
                ..
            }),
            ..
        }
    ));
}

#[test]
fn an_expired_request_never_writes_or_completes_successfully() {
    let (mut owner, receiver) = owner(FakeTransport::default());
    let mut request = request(Action::Input(b"key".as_slice().into()));
    request.deadline = Instant::now();
    assert!(owner.perform(request, &Cancellation::default()).is_err());
    assert!(owner.transport.committed.is_empty());
    assert!(matches!(
        collected(&receiver)[0].kind(),
        PtyOutputKind::Completed {
            result: Err(TransportError::Input {
                reason: InputFailure::Deadline,
                ..
            }),
            ..
        }
    ));
}

#[test]
fn read_failure_prevents_write_and_has_a_typed_completion() {
    let (mut owner, receiver) = owner(FakeTransport {
        reads: [FakeRead::Failure(io::ErrorKind::InvalidData)].into(),
        ..Default::default()
    });
    assert!(
        owner
            .perform(
                request(Action::Input(b"key".as_slice().into())),
                &Cancellation::default()
            )
            .is_err()
    );
    assert!(owner.transport.committed.is_empty());
    assert!(matches!(
        collected(&receiver)[0].kind(),
        PtyOutputKind::Completed {
            result: Err(TransportError::Read(_)),
            ..
        }
    ));
}

#[test]
fn resize_marker_and_fence_share_the_output_order_without_input() {
    let (mut owner, receiver) = owner(FakeTransport {
        reads: [FakeRead::Data(b"earlier"), FakeRead::Empty].into(),
        ..Default::default()
    });
    let size = TerminalSize::new(4, 17);
    owner
        .perform(request(Action::Resize(size)), &Cancellation::default())
        .unwrap();
    owner
        .perform(
            request(Action::Mark("ready".into())),
            &Cancellation::default(),
        )
        .unwrap();
    owner
        .perform(request(Action::Fence), &Cancellation::default())
        .unwrap();
    let events = collected(&receiver);
    assert_eq!(owner.transport.resizes, [size]);
    assert!(matches!(events[0].kind(), PtyOutputKind::Data(_)));
    assert!(matches!(events[1].kind(), PtyOutputKind::Resized(actual) if *actual == size));
    assert!(matches!(
        events[2].kind(),
        PtyOutputKind::Completed { result: Ok(_), .. }
    ));
    assert!(matches!(events[3].kind(), PtyOutputKind::Marked(label) if label == "ready"));
    assert_eq!(events.len(), 6);
    assert!(
        events
            .iter()
            .all(|event| !matches!(event.kind(), PtyOutputKind::InputCommitted { .. }))
    );
}

#[test]
fn output_eof_retains_resize_marker_capability_but_refuses_input() {
    let (mut owner, receiver) = owner(FakeTransport {
        reads: [FakeRead::Data(b"last"), FakeRead::Eof].into(),
        ..Default::default()
    });
    owner
        .perform(
            request(Action::Mark("after".into())),
            &Cancellation::default(),
        )
        .unwrap();
    owner
        .perform(
            request(Action::Resize(TerminalSize::new(5, 20))),
            &Cancellation::default(),
        )
        .unwrap();
    assert!(
        owner
            .perform(
                request(Action::Input(b"key".as_slice().into())),
                &Cancellation::default()
            )
            .is_err()
    );
    let events = collected(&receiver);
    assert!(matches!(events[0].kind(), PtyOutputKind::Data(_)));
    assert!(matches!(events[1].kind(), PtyOutputKind::OutputClosed));
    assert!(matches!(events[2].kind(), PtyOutputKind::Marked(_)));
    assert!(matches!(
        events.last().unwrap().kind(),
        PtyOutputKind::Completed {
            result: Err(TransportError::Closed),
            ..
        }
    ));
    assert!(owner.transport.committed.is_empty());
}

#[test]
fn consumer_sequence_rejects_duplicates_and_gaps() {
    let mut cursor = ObservationCursor::default();
    let first = ObservationSequence(NonZeroU64::new(1).unwrap());
    cursor.accept(first).unwrap();
    assert!(matches!(
        cursor.accept(first),
        Err(TransportError::ObservationOrder)
    ));
    assert!(matches!(
        cursor.accept(ObservationSequence(NonZeroU64::new(3).unwrap())),
        Err(TransportError::ObservationOrder)
    ));
    cursor
        .accept(ObservationSequence(NonZeroU64::new(2).unwrap()))
        .unwrap();
}

#[test]
fn cancelled_while_draining_cannot_resize_mark_or_certify_a_fence() {
    for action in [
        Action::Resize(TerminalSize::new(4, 17)),
        Action::Mark("not committed".into()),
        Action::Fence,
    ] {
        let request = request(action);
        let (mut owner, receiver) = owner(FakeTransport {
            reads: [
                FakeRead::Data(b"earlier"),
                FakeRead::Cancel(Arc::clone(&request.cancellation)),
            ]
            .into(),
            ..Default::default()
        });
        assert!(owner.perform(request, &Cancellation::default()).is_err());
        let events = collected(&receiver);
        assert_eq!(events.len(), 2);
        assert!(
            matches!(events[0].kind(), PtyOutputKind::Data(bytes) if bytes.as_ref() == b"earlier")
        );
        assert!(matches!(
            events[1].kind(),
            PtyOutputKind::Completed {
                result: Err(TransportError::Cancelled),
                ..
            }
        ));
        assert!(owner.transport.resizes.is_empty());
    }
}

#[test]
fn an_expired_drain_refuses_without_sampling_output_or_writing() {
    let (mut owner, receiver) = owner(FakeTransport {
        reads: [FakeRead::Data(b"not observed")].into(),
        ..Default::default()
    });
    assert!(matches!(
        owner.drain_available(Instant::now()),
        Err(TransportError::DrainDeadline)
    ));
    assert!(collected(&receiver).is_empty());
    assert!(owner.transport.committed.is_empty());
}

#[test]
fn closed_worker_drains_final_observations_and_closes_exactly_once() {
    let (requests, request_receiver) = mpsc::channel();
    let (sender, receiver) = mpsc::channel();
    let stop = Arc::new(Cancellation::default());
    stop.cancel();
    run_transport(
        FakeTransport {
            reads: [FakeRead::Data(b"last"), FakeRead::Eof].into(),
            ..Default::default()
        },
        request_receiver,
        sender,
        stop,
    );
    drop(requests);
    let events = collected(&receiver);
    assert_eq!(events.len(), 3);
    assert!(matches!(events[0].kind(), PtyOutputKind::Data(_)));
    assert!(matches!(events[1].kind(), PtyOutputKind::OutputClosed));
    assert!(matches!(events[2].kind(), PtyOutputKind::Closed));
}

fn fake_pump() -> (
    PtyOutputPump,
    Receiver<Request>,
    Sender<PtyOutputEvent>,
    UnixStream,
) {
    let (wake, wake_reader) = UnixStream::pair().unwrap();
    let (requests, request_receiver) = mpsc::channel();
    let (sender, receiver) = mpsc::channel();
    (
        PtyOutputPump {
            receiver,
            requests,
            wake,
            stop: Arc::new(Cancellation::default()),
            worker: WorkerState::Joined(WorkerCompletion::Clean),
            request_counter: RequestCounter::default(),
        },
        request_receiver,
        sender,
        wake_reader,
    )
}

#[test]
fn dropped_ticket_cancels_its_request_and_invalidates_the_session() {
    let (mut pump, requests, _sender, _wake_reader) = fake_pump();
    let ticket = pump.input(b"key").unwrap();
    drop(ticket);
    let request = requests.try_recv().unwrap();
    assert_eq!(request.cancellation.state(), CancellationState::Cancelled);
    assert_eq!(pump.stop.state(), CancellationState::Cancelled);
    assert!(matches!(
        pump.input(b"second"),
        Err(TransportError::Cancelled)
    ));
    assert!(matches!(
        pump.mark("second"),
        Err(TransportError::Cancelled)
    ));
    assert!(matches!(
        pump.resize(TerminalSize::new(4, 17)),
        Err(TransportError::Cancelled)
    ));
    assert!(matches!(pump.fence(), Err(TransportError::Cancelled)));
    assert!(matches!(requests.try_recv(), Err(TryRecvError::Empty)));
}

#[test]
fn observer_failure_cancels_without_erasing_an_already_committed_prefix() {
    let (mut pump, requests, sender, _wake_reader) = fake_pump();
    let ticket = pump.input(b"key").unwrap();
    let request = requests.try_recv().unwrap();
    sender
        .send(PtyOutputEvent {
            sequence: ObservationSequence(NonZeroU64::new(1).unwrap()),
            observed_at: Instant::now(),
            kind: PtyOutputKind::InputCommitted {
                request: request.id,
                bytes: b"key".as_slice().into(),
            },
        })
        .unwrap();
    let mut committed = Vec::new();
    let result = ticket.finish(|event| {
        if let PtyOutputKind::InputCommitted { bytes, .. } = event.kind() {
            committed.extend_from_slice(bytes);
        }
        Err(io::Error::new(io::ErrorKind::WriteZero, "recording failed"))
    });
    assert!(matches!(result, Err(TransportError::Observer(_))));
    assert_eq!(committed, b"key");
    assert_eq!(request.cancellation.state(), CancellationState::Cancelled);
    assert_eq!(pump.stop.state(), CancellationState::Cancelled);
}

#[test]
fn a_panicked_worker_disconnects_the_ticket_and_explicit_join_reports_panic() {
    let (mut pump, _requests, sender, _wake_reader) = fake_pump();
    pump.worker = WorkerState::Running(std::thread::spawn(move || {
        // This deliberate test panic drops the worker's sole event sender.
        let _owned_sender = sender;
        panic!("deterministic PTY worker panic control");
    }));
    let ticket = pump.input(b"key").unwrap();
    assert!(matches!(
        ticket.finish(|_| Ok(())),
        Err(TransportError::Closed)
    ));
    assert!(matches!(pump.finish(), Err(TransportError::WorkerPanicked)));
    assert!(matches!(pump.finish(), Err(TransportError::WorkerPanicked)));
}

#[test]
fn a_disconnected_request_queue_cannot_issue_a_successful_ticket() {
    let (mut pump, requests, _sender, _wake_reader) = fake_pump();
    drop(requests);
    assert!(matches!(pump.input(b"key"), Err(TransportError::Closed)));
}

#[test]
fn a_foreign_completion_is_rejected_before_the_observer_can_apply_it() {
    let (mut pump, requests, sender, _wake_reader) = fake_pump();
    let ticket = pump.input(b"key").unwrap();
    let request = requests.try_recv().unwrap();
    let mut ids = RequestCounter::default();
    assert_eq!(ids.next().unwrap(), request.id);
    let foreign = ids.next().unwrap();
    sender
        .send(PtyOutputEvent {
            sequence: ObservationSequence(NonZeroU64::new(1).unwrap()),
            observed_at: Instant::now(),
            kind: PtyOutputKind::Completed {
                request: foreign,
                result: Ok(InputReceipt {
                    request: foreign,
                    committed: InputByteCount(3),
                }),
            },
        })
        .unwrap();
    let mut applied = Vec::new();
    assert!(matches!(
        ticket.finish(|event| {
            applied.push(event.sequence());
            Ok(())
        }),
        Err(TransportError::RequestMismatch)
    ));
    assert!(applied.is_empty());
    assert_eq!(request.cancellation.state(), CancellationState::Cancelled);
    assert_eq!(pump.stop.state(), CancellationState::Cancelled);
}

#[test]
fn a_failed_completion_invalidates_later_requests_before_they_are_queued() {
    let (mut pump, requests, sender, _wake_reader) = fake_pump();
    let ticket = pump.input(b"key").unwrap();
    let request = requests.try_recv().unwrap();
    sender
        .send(PtyOutputEvent {
            sequence: ObservationSequence(NonZeroU64::new(1).unwrap()),
            observed_at: Instant::now(),
            kind: PtyOutputKind::Completed {
                request: request.id,
                result: Err(TransportError::Input {
                    committed: InputByteCount(0),
                    requested: InputByteCount(3),
                    reason: InputFailure::ZeroWrite,
                }),
            },
        })
        .unwrap();
    assert!(matches!(
        ticket.finish(|_| Ok(())),
        Err(TransportError::Input {
            reason: InputFailure::ZeroWrite,
            ..
        })
    ));
    assert!(matches!(
        pump.input(b"second"),
        Err(TransportError::Cancelled)
    ));
    assert!(matches!(requests.try_recv(), Err(TryRecvError::Empty)));
}
