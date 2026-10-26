//! The one owner of PTY reads, writes, resizes and observation order.
//!
//! A transport fence drains output available *now*. It is not a redisplay or
//! startup-completion certificate; those belong to the editor protocol.

use std::io::{self, Read as _, Write as _};
use std::num::NonZeroU64;
use std::os::fd::{AsFd as _, AsRawFd as _, BorrowedFd};
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::recording::TerminalSize;

mod owner;

/// IDs are issued by one exclusively borrowed session and cannot be invented
/// by callers. The pending ticket keeps the owning session borrowed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub(crate) struct InputRequestId(NonZeroU64);

#[derive(Debug, Default)]
struct RequestCounter(u64);

impl RequestCounter {
    fn next(&mut self) -> Result<InputRequestId, TransportError> {
        self.0 = self
            .0
            .checked_add(1)
            .ok_or(TransportError::SequenceExhausted)?;
        Ok(InputRequestId(
            NonZeroU64::new(self.0).ok_or(TransportError::SequenceExhausted)?,
        ))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub(crate) struct ObservationSequence(NonZeroU64);

/// A consumer must see every observation exactly once, including completions.
#[derive(Debug, Default)]
pub(crate) struct ObservationCursor(u64);

impl ObservationCursor {
    pub(crate) fn accept(&mut self, sequence: ObservationSequence) -> Result<(), TransportError> {
        if self.0.checked_add(1) != Some(sequence.0.get()) {
            return Err(TransportError::ObservationOrder);
        }
        self.0 = sequence.0.get();
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub(crate) struct InputByteCount(usize);

impl InputByteCount {
    pub(crate) const fn as_usize(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct InputReceipt {
    request: InputRequestId,
    committed: InputByteCount,
}

impl InputReceipt {
    pub(crate) const fn committed(self) -> InputByteCount {
        self.committed
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum InputFailure {
    #[error("partial PTY write")]
    PartialWrite,
    #[error("PTY write returned zero")]
    ZeroWrite,
    #[error("input deadline expired")]
    Deadline,
    #[error("input request cancelled")]
    Cancelled,
    #[error("PTY write: {0}")]
    Io(#[source] io::Error),
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum TransportError {
    #[error("PTY transport closed")]
    Closed,
    #[error("PTY transport request cancelled")]
    Cancelled,
    #[error("PTY output drain deadline expired")]
    DrainDeadline,
    #[error("PTY operation deadline expired")]
    RequestDeadline,
    #[error("PTY read: {0}")]
    Read(#[source] io::Error),
    #[error("PTY resize: {0}")]
    Resize(#[source] io::Error),
    #[error("input failed after {committed:?}/{requested:?} bytes: {reason}")]
    Input {
        committed: InputByteCount,
        requested: InputByteCount,
        reason: InputFailure,
    },
    #[error("PTY event observer: {0}")]
    Observer(#[source] io::Error),
    #[error("PTY worker wakeup: {0}")]
    Wake(#[source] io::Error),
    #[error("PTY worker panicked")]
    WorkerPanicked,
    #[error("PTY request completion did not match its consumed ticket")]
    RequestMismatch,
    #[error("PTY observation sequence is not the next sequence")]
    ObservationOrder,
    #[error("PTY request or observation sequence exhausted")]
    SequenceExhausted,
}

#[derive(Debug)]
pub(crate) enum PtyOutputKind {
    Data(Box<[u8]>),
    InputCommitted {
        request: InputRequestId,
        bytes: Box<[u8]>,
    },
    Resized(TerminalSize),
    Marked(String),
    Completed {
        request: InputRequestId,
        result: Result<InputReceipt, TransportError>,
    },
    OutputClosed,
    Closed,
    Failed(TransportError),
}

#[derive(Debug)]
pub(crate) struct PtyOutputEvent {
    sequence: ObservationSequence,
    observed_at: Instant,
    kind: PtyOutputKind,
}

impl PtyOutputEvent {
    pub(crate) const fn sequence(&self) -> ObservationSequence {
        self.sequence
    }

    pub(crate) const fn observed_at(&self) -> Instant {
        self.observed_at
    }

    pub(crate) fn kind(&self) -> &PtyOutputKind {
        &self.kind
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CancellationState {
    Active,
    Cancelled,
}

#[derive(Debug, Default)]
struct Cancellation(AtomicBool);

impl Cancellation {
    fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    fn state(&self) -> CancellationState {
        match self.0.load(Ordering::Acquire) {
            false => CancellationState::Active,
            true => CancellationState::Cancelled,
        }
    }
}

#[derive(Debug)]
enum Action {
    Input(Box<[u8]>),
    Resize(TerminalSize),
    Mark(String),
    Fence,
}

#[derive(Debug)]
struct Request {
    id: InputRequestId,
    action: Action,
    cancellation: Arc<Cancellation>,
    deadline: Instant,
}

impl Request {
    fn ensure_active(&self, stop: &Cancellation) -> Result<(), TransportError> {
        if self.cancellation.state() == CancellationState::Cancelled
            || stop.state() == CancellationState::Cancelled
        {
            return Err(TransportError::Cancelled);
        }
        if Instant::now() >= self.deadline {
            return Err(TransportError::RequestDeadline);
        }
        Ok(())
    }
}

#[derive(Debug)]
enum WorkerState {
    Running(JoinHandle<()>),
    Joined(WorkerCompletion),
}

#[derive(Clone, Copy, Debug)]
enum WorkerCompletion {
    Clean,
    Panicked,
}

/// Continuously drains the PTY, even while the scenario is idle. GNU can make
/// its slave nonblocking, so a test-driver-only reader loses real output.
#[derive(Debug)]
pub(crate) struct PtyOutputPump {
    receiver: Receiver<PtyOutputEvent>,
    requests: Sender<Request>,
    wake: UnixStream,
    stop: Arc<Cancellation>,
    worker: WorkerState,
    request_counter: RequestCounter,
}

impl PtyOutputPump {
    pub(crate) fn start(pty: pty_process::blocking::Pty, name: &str) -> io::Result<Self> {
        let (wake, wake_reader) = UnixStream::pair()?;
        wake.set_nonblocking(true)?;
        wake_reader.set_nonblocking(true)?;
        set_nonblocking(pty.as_fd())?;
        let (sender, receiver) = mpsc::channel();
        let (requests, request_receiver) = mpsc::channel();
        let stop = Arc::new(Cancellation::default());
        let worker_stop = Arc::clone(&stop);
        let worker = std::thread::Builder::new()
            .name(format!("tui-pty-owner-{name}"))
            .spawn(move || {
                let transport = owner::PtyTransport::new(pty, wake_reader);
                owner::run(transport, request_receiver, sender, worker_stop);
            })?;
        Ok(Self {
            receiver,
            requests,
            wake,
            stop,
            worker: WorkerState::Running(worker),
            request_counter: RequestCounter::default(),
        })
    }

    pub(crate) fn input(&mut self, bytes: &[u8]) -> Result<PendingInput<'_>, TransportError> {
        self.request(Action::Input(bytes.into()))
    }

    pub(crate) fn resize(
        &mut self,
        size: TerminalSize,
    ) -> Result<PendingInput<'_>, TransportError> {
        self.request(Action::Resize(size))
    }

    pub(crate) fn mark(&mut self, label: &str) -> Result<PendingInput<'_>, TransportError> {
        self.request(Action::Mark(label.into()))
    }

    /// Drain currently available output without claiming semantic readiness.
    pub(crate) fn fence(&mut self) -> Result<PendingInput<'_>, TransportError> {
        self.request(Action::Fence)
    }

    fn request(&mut self, action: Action) -> Result<PendingInput<'_>, TransportError> {
        if self.stop.state() == CancellationState::Cancelled {
            return Err(TransportError::Cancelled);
        }
        let id = self.request_counter.next()?;
        let cancellation = Arc::new(Cancellation::default());
        self.requests
            .send(Request {
                id,
                action,
                cancellation: Arc::clone(&cancellation),
                deadline: Instant::now() + Duration::from_secs(10),
            })
            .map_err(|_| TransportError::Closed)?;
        if let Err(error) = self.wake_worker() {
            cancellation.cancel();
            self.stop.cancel();
            return Err(error);
        }
        Ok(PendingInput {
            pump: self,
            id,
            state: TicketState::Pending(cancellation),
        })
    }

    fn wake_worker(&mut self) -> Result<(), TransportError> {
        match self.wake.write(&[1]) {
            Ok(0) => Err(TransportError::Wake(io::Error::new(
                io::ErrorKind::WriteZero,
                "PTY worker wake socket accepted no byte",
            ))),
            Ok(_) => Ok(()),
            // A full wake socket already makes the worker runnable.
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(()),
            Err(error) => Err(TransportError::Wake(error)),
        }
    }

    pub(crate) fn try_recv(&self) -> Result<PtyOutputEvent, TryRecvError> {
        self.receiver.try_recv()
    }

    pub(crate) fn recv_timeout(
        &self,
        timeout: Duration,
    ) -> Result<PtyOutputEvent, RecvTimeoutError> {
        self.receiver.recv_timeout(timeout)
    }

    /// Stop the transport, retain its final observations, and join explicitly.
    /// The caller must consume the final event queue before finishing the cast.
    pub(crate) fn finish(&mut self) -> Result<(), TransportError> {
        self.stop.cancel();
        let _ = self.wake_worker();
        let completion = match std::mem::replace(
            &mut self.worker,
            WorkerState::Joined(WorkerCompletion::Clean),
        ) {
            WorkerState::Running(worker) => match worker.join() {
                Ok(()) => WorkerCompletion::Clean,
                Err(_) => WorkerCompletion::Panicked,
            },
            WorkerState::Joined(completion) => completion,
        };
        self.worker = WorkerState::Joined(completion);
        match completion {
            WorkerCompletion::Clean => Ok(()),
            WorkerCompletion::Panicked => Err(TransportError::WorkerPanicked),
        }
    }
}

impl Drop for PtyOutputPump {
    fn drop(&mut self) {
        self.stop.cancel();
        let _ = self.wake_worker();
        // No join in Drop. The explicit finish path owns positive completion.
    }
}

#[derive(Debug)]
enum TicketState {
    Pending(Arc<Cancellation>),
    Consumed,
}

/// A non-cloneable, consumed ticket keeps a request bound to its borrowed pump.
/// Dropping the ticket cancels it; already committed bytes remain observations.
#[must_use = "finish the request or dropping it cancels the transport operation"]
#[derive(Debug)]
pub(crate) struct PendingInput<'pump> {
    pump: &'pump mut PtyOutputPump,
    id: InputRequestId,
    state: TicketState,
}

impl PendingInput<'_> {
    pub(crate) fn finish(
        mut self,
        mut observe: impl FnMut(&PtyOutputEvent) -> io::Result<()>,
    ) -> Result<InputReceipt, TransportError> {
        loop {
            let event = self
                .pump
                .receiver
                .recv()
                .map_err(|_| TransportError::Closed)?;
            match event.kind() {
                PtyOutputKind::InputCommitted { request, .. }
                | PtyOutputKind::Completed { request, .. } => {
                    if *request != self.id {
                        return Err(TransportError::RequestMismatch);
                    }
                }
                PtyOutputKind::Data(_)
                | PtyOutputKind::Resized(_)
                | PtyOutputKind::Marked(_)
                | PtyOutputKind::OutputClosed
                | PtyOutputKind::Closed
                | PtyOutputKind::Failed(_) => {}
            }
            observe(&event).map_err(TransportError::Observer)?;
            match event.kind {
                PtyOutputKind::Completed { request, result } => {
                    if request != self.id {
                        return Err(TransportError::RequestMismatch);
                    }
                    if let Ok(receipt) = &result
                        && receipt.request != self.id
                    {
                        return Err(TransportError::RequestMismatch);
                    }
                    self.state = TicketState::Consumed;
                    if result.is_err() {
                        self.pump.stop.cancel();
                        let _ = self.pump.wake_worker();
                    }
                    return result;
                }
                PtyOutputKind::Closed => return Err(TransportError::Closed),
                PtyOutputKind::Failed(error) => return Err(error),
                PtyOutputKind::Data(_)
                | PtyOutputKind::InputCommitted { .. }
                | PtyOutputKind::Resized(_)
                | PtyOutputKind::Marked(_)
                | PtyOutputKind::OutputClosed => {}
            }
        }
    }
}

impl Drop for PendingInput<'_> {
    fn drop(&mut self) {
        match &self.state {
            TicketState::Pending(cancellation) => {
                cancellation.cancel();
                self.pump.stop.cancel();
                let _ = self.pump.wake_worker();
            }
            TicketState::Consumed => {}
        }
    }
}

fn set_nonblocking(fd: BorrowedFd<'_>) -> io::Result<()> {
    // SAFETY: the live PTY owner retains this descriptor throughout the calls.
    let flags = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETFL) };
    if flags < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: F_SETFL takes an integer bitmask, preserving all existing flags.
    if unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

static_assertions::assert_impl_all!(InputRequestId: Send, Sync);
static_assertions::assert_impl_all!(ObservationSequence: Send, Sync);
static_assertions::assert_impl_all!(PtyOutputEvent: Send);
static_assertions::assert_impl_all!(PtyOutputPump: Send);
static_assertions::assert_not_impl_any!(PtyOutputPump: Sync);
static_assertions::assert_not_impl_any!(PendingInput<'static>: Clone, Copy, Sync);
static_assertions::assert_eq_size!(InputRequestId, NonZeroU64);
static_assertions::assert_eq_size!(ObservationSequence, NonZeroU64);
static_assertions::assert_eq_size!(InputByteCount, usize);
