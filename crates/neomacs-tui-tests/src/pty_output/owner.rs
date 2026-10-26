use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Readiness {
    Available,
    Idle,
    Closed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WriteReadiness {
    Ready,
    Retry,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PollInterest {
    Output,
    OutputAndInput,
}

/// Kernel poll bits are decoded only at the transport boundary.
#[derive(Clone, Copy, Debug)]
struct PollEvents(libc::c_short);

impl PollEvents {
    fn output_readiness(self) -> Readiness {
        if self.0 & libc::POLLIN != 0 {
            Readiness::Available
        } else if self.0 & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0 {
            Readiness::Closed
        } else {
            Readiness::Idle
        }
    }

    fn input_readiness(self) -> io::Result<WriteReadiness> {
        if self.0 & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0 {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "PTY closed"));
        }
        if self.0 & libc::POLLOUT != 0 {
            Ok(WriteReadiness::Ready)
        } else {
            Ok(WriteReadiness::Retry)
        }
    }
}

/// Bytes transferred by one transport read or write, before input accounting.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
struct TransportByteCount(usize);

impl TransportByteCount {
    const fn as_usize(self) -> usize {
        self.0
    }
}

impl From<TransportByteCount> for InputByteCount {
    fn from(bytes: TransportByteCount) -> Self {
        Self(bytes.0)
    }
}

static_assertions::assert_eq_size!(TransportByteCount, usize);

/// Private and sealed by this module: fake controls exercise the same owner as
/// the real PTY. A WouldBlock read is the only ordinary empty-drain boundary.
trait Transport {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<TransportByteCount>;
    fn write(&mut self, bytes: &[u8]) -> io::Result<TransportByteCount>;
    fn resize(&mut self, size: TerminalSize) -> io::Result<()>;
    fn wait(&mut self, timeout: Duration) -> io::Result<Readiness>;
    fn wait_writable(&mut self, timeout: Duration) -> io::Result<WriteReadiness>;
}

pub(super) struct PtyTransport {
    pty: pty_process::blocking::Pty,
    wake: UnixStream,
}

impl std::fmt::Debug for PtyTransport {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PtyTransport")
            .field("pty_fd", &self.pty.as_raw_fd())
            .field("wake", &self.wake)
            .finish()
    }
}

impl PtyTransport {
    pub(super) fn new(pty: pty_process::blocking::Pty, wake: UnixStream) -> Self {
        Self { pty, wake }
    }

    fn poll(&mut self, interest: PollInterest, timeout: Duration) -> io::Result<PollEvents> {
        let timeout_ms = timeout.as_millis().min(i32::MAX as u128) as i32;
        let events = match interest {
            PollInterest::Output => libc::POLLIN,
            PollInterest::OutputAndInput => libc::POLLIN | libc::POLLOUT,
        };
        let mut descriptors = [
            libc::pollfd {
                fd: self.pty.as_raw_fd(),
                events,
                revents: 0,
            },
            libc::pollfd {
                fd: self.wake.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        // SAFETY: both descriptors remain owned for the entire poll and the
        // array is initialized, writable, and has exactly the supplied length.
        let result = unsafe { libc::poll(descriptors.as_mut_ptr(), 2, timeout_ms) };
        if result < 0 {
            return Err(io::Error::last_os_error());
        }
        if descriptors[1].revents & libc::POLLIN != 0 {
            let mut bytes = [0; 256];
            loop {
                match self.wake.read(&mut bytes) {
                    Ok(0) => break,
                    Ok(_) => {}
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                    Err(error) => return Err(error),
                }
            }
        }
        Ok(PollEvents(descriptors[0].revents))
    }
}

impl Transport for PtyTransport {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<TransportByteCount> {
        match self.pty.read(buffer) {
            // Linux PTY masters report EIO when the last slave closes. This is
            // terminal EOF, not a fabricated successful input completion.
            Err(error) if error.raw_os_error() == Some(libc::EIO) => Ok(TransportByteCount(0)),
            result => result.map(TransportByteCount),
        }
    }

    fn write(&mut self, bytes: &[u8]) -> io::Result<TransportByteCount> {
        self.pty.write(bytes).map(TransportByteCount)
    }

    fn resize(&mut self, size: TerminalSize) -> io::Result<()> {
        self.pty
            .resize(pty_process::Size::new(size.rows(), size.columns()))
            .map_err(io::Error::other)
    }

    fn wait(&mut self, timeout: Duration) -> io::Result<Readiness> {
        Ok(self.poll(PollInterest::Output, timeout)?.output_readiness())
    }

    fn wait_writable(&mut self, timeout: Duration) -> io::Result<WriteReadiness> {
        self.poll(PollInterest::OutputAndInput, timeout)?
            .input_readiness()
    }
}

#[derive(Debug)]
struct Owner<T> {
    transport: T,
    sender: Sender<PtyOutputEvent>,
    sequence: SequenceIssuer,
    read_state: ReadState,
}

#[derive(Debug, Default)]
struct SequenceIssuer(u64);

impl SequenceIssuer {
    fn next(&mut self) -> Result<ObservationSequence, TransportError> {
        self.0 = self
            .0
            .checked_add(1)
            .ok_or(TransportError::SequenceExhausted)?;
        Ok(ObservationSequence(
            NonZeroU64::new(self.0).ok_or(TransportError::SequenceExhausted)?,
        ))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReadState {
    Open,
    Closed,
}

impl<T: Transport> Owner<T> {
    fn emit(&mut self, kind: PtyOutputKind) -> Result<(), TransportError> {
        let sequence = self.sequence.next()?;
        self.sender
            .send(PtyOutputEvent {
                sequence,
                observed_at: Instant::now(),
                kind,
            })
            .map_err(|_| TransportError::Closed)
    }

    fn drain_available(&mut self, deadline: Instant) -> Result<(), TransportError> {
        match self.read_state {
            ReadState::Closed => return Ok(()),
            ReadState::Open => {}
        }
        let mut buffer = [0_u8; 65_536];
        loop {
            if Instant::now() >= deadline {
                return Err(TransportError::DrainDeadline);
            }
            match self.transport.read(&mut buffer) {
                Ok(TransportByteCount(0)) => {
                    self.read_state = ReadState::Closed;
                    self.emit(PtyOutputKind::OutputClosed)?;
                    return Ok(());
                }
                Ok(length) => self.emit(PtyOutputKind::Data(buffer[..length.as_usize()].into()))?,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(()),
                Err(error) => return Err(TransportError::Read(error)),
            }
        }
    }

    fn input(
        &mut self,
        request: &Request,
        bytes: &[u8],
        stop: &Cancellation,
    ) -> Result<InputByteCount, TransportError> {
        let requested = InputByteCount(bytes.len());
        let failed = |reason| TransportError::Input {
            committed: InputByteCount(0),
            requested,
            reason,
        };
        loop {
            if request.cancellation.state() == CancellationState::Cancelled
                || stop.state() == CancellationState::Cancelled
            {
                return Err(failed(InputFailure::Cancelled));
            }
            if Instant::now() >= request.deadline {
                return Err(failed(InputFailure::Deadline));
            }
            // This owner-observed empty read is the input linearization
            // boundary. No other thread may read, write or record this PTY.
            self.drain_available(request.deadline)?;
            match self.read_state {
                ReadState::Closed => return Err(TransportError::Closed),
                ReadState::Open => {}
            }
            if request.cancellation.state() == CancellationState::Cancelled
                || stop.state() == CancellationState::Cancelled
            {
                return Err(failed(InputFailure::Cancelled));
            }
            if bytes.is_empty() {
                return Ok(InputByteCount(0));
            }
            match self.transport.write(bytes) {
                Ok(TransportByteCount(0)) => return Err(failed(InputFailure::ZeroWrite)),
                Ok(length) => {
                    let committed = InputByteCount::from(length);
                    self.emit(PtyOutputKind::InputCommitted {
                        request: request.id,
                        bytes: bytes[..length.as_usize()].into(),
                    })?;
                    if length.as_usize() != bytes.len() {
                        return Err(TransportError::Input {
                            committed,
                            requested,
                            reason: InputFailure::PartialWrite,
                        });
                    }
                    if request.cancellation.state() == CancellationState::Cancelled
                        || stop.state() == CancellationState::Cancelled
                    {
                        return Err(TransportError::Input {
                            committed,
                            requested,
                            reason: InputFailure::Cancelled,
                        });
                    }
                    return Ok(committed);
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    match self.transport.wait_writable(
                        request
                            .deadline
                            .saturating_duration_since(Instant::now())
                            .min(Duration::from_millis(50)),
                    ) {
                        Ok(WriteReadiness::Ready | WriteReadiness::Retry) => {}
                        Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                        Err(error) => return Err(failed(InputFailure::Io(error))),
                    }
                }
                Err(error) => return Err(failed(InputFailure::Io(error))),
            }
        }
    }

    fn perform(&mut self, request: Request, stop: &Cancellation) -> Result<(), TransportError> {
        let result = if request.cancellation.state() == CancellationState::Cancelled
            || stop.state() == CancellationState::Cancelled
        {
            Err(TransportError::Cancelled)
        } else {
            match &request.action {
                Action::Input(bytes) => self.input(&request, bytes, stop),
                Action::Resize(size) => self.drain_available(request.deadline).and_then(|()| {
                    request.ensure_active(stop)?;
                    self.transport
                        .resize(*size)
                        .map_err(TransportError::Resize)?;
                    self.emit(PtyOutputKind::Resized(*size))?;
                    request.ensure_active(stop)?;
                    Ok(InputByteCount(0))
                }),
                Action::Mark(label) => self.drain_available(request.deadline).and_then(|()| {
                    request.ensure_active(stop)?;
                    self.emit(PtyOutputKind::Marked(label.clone()))?;
                    request.ensure_active(stop)?;
                    Ok(InputByteCount(0))
                }),
                Action::Fence => self.drain_available(request.deadline).and_then(|()| {
                    request.ensure_active(stop)?;
                    Ok(InputByteCount(0))
                }),
            }
        }
        .map(|committed| InputReceipt {
            request: request.id,
            committed,
        });
        let disposition = match &result {
            Ok(_) => WorkerDisposition::Continue,
            Err(_) => WorkerDisposition::Stop,
        };
        self.emit(PtyOutputKind::Completed {
            request: request.id,
            result,
        })?;
        match disposition {
            WorkerDisposition::Continue => Ok(()),
            WorkerDisposition::Stop => Err(TransportError::Closed),
        }
    }
}

#[derive(Debug)]
enum WorkerDisposition {
    Continue,
    Stop,
}

pub(super) fn run(
    transport: PtyTransport,
    requests: Receiver<Request>,
    sender: Sender<PtyOutputEvent>,
    stop: Arc<Cancellation>,
) {
    run_transport(transport, requests, sender, stop);
}

fn run_transport<T: Transport>(
    transport: T,
    requests: Receiver<Request>,
    sender: Sender<PtyOutputEvent>,
    stop: Arc<Cancellation>,
) {
    let mut owner = Owner {
        transport,
        sender,
        sequence: SequenceIssuer::default(),
        read_state: ReadState::Open,
    };
    let result = (|| {
        loop {
            match requests.try_recv() {
                Ok(request) => owner.perform(request, &stop)?,
                Err(TryRecvError::Disconnected) => return Ok(()),
                Err(TryRecvError::Empty) => {}
            }
            if stop.state() == CancellationState::Cancelled {
                return owner.drain_available(Instant::now() + Duration::from_secs(1));
            }
            match owner.read_state {
                ReadState::Closed => {
                    match requests.recv_timeout(Duration::from_millis(50)) {
                        Ok(request) => owner.perform(request, &stop)?,
                        Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => return Ok(()),
                    }
                    continue;
                }
                ReadState::Open => {}
            }
            match owner.transport.wait(Duration::from_millis(50)) {
                Ok(Readiness::Available) => {
                    owner.drain_available(Instant::now() + Duration::from_secs(10))?;
                }
                Ok(Readiness::Idle) => {}
                Ok(Readiness::Closed) => {
                    owner.read_state = ReadState::Closed;
                    owner.emit(PtyOutputKind::OutputClosed)?;
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(TransportError::Read(error)),
            }
        }
    })();
    if let Err(error) = result {
        match error {
            TransportError::Closed => {}
            error @ (TransportError::Cancelled
            | TransportError::DrainDeadline
            | TransportError::RequestDeadline
            | TransportError::Read(_)
            | TransportError::Resize(_)
            | TransportError::Input { .. }
            | TransportError::Observer(_)
            | TransportError::Wake(_)
            | TransportError::WorkerPanicked
            | TransportError::RequestMismatch
            | TransportError::ObservationOrder
            | TransportError::SequenceExhausted) => {
                let _ = owner.emit(PtyOutputKind::Failed(error));
            }
        }
    }
    // Requests already queued at close get explicit failed completions. A
    // later sender receives Closed from the dropped command receiver.
    for request in requests.try_iter() {
        let _ = owner.emit(PtyOutputKind::Completed {
            request: request.id,
            result: Err(TransportError::Closed),
        });
    }
    let _ = owner.emit(PtyOutputKind::Closed);
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
