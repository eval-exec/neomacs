//! The primary Unix reader owns acquisition and its old unbounded backlog.
//! Only the existing4096-event RenderComms channel is a bounded admission point.

use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::{TryRecvError, TrySendError};

use super::{
    GNU_KBD_BUFFER_SIZE, TTY_READER_POLL_INTERVAL, TerminalCellSize, TerminalSizeQuery,
    TtyInputReader, raw_tty_input_event,
};
use crate::thread_comm::{InputEvent, LifecycleCommand, RenderCommand, RenderComms};

#[derive(Debug)]
struct CapacityWake {
    writer: UnixStream,
    armed: AtomicBool,
    closed: AtomicBool,
}

/// A coalesced capacity wake for the primary TTY reader.
/// Ordinary input reception does not write a wake byte unless the reader
/// previously found its existing bounded input channel full. Clones may be
/// shared by the owner and primary bridge: atomics coordinate readiness and
/// the private nonblocking socket carries only remembered wake bytes.
#[derive(Clone, Debug)]
pub struct TtyInputCapacityNotifier {
    wake: Arc<CapacityWake>,
}

impl TtyInputCapacityNotifier {
    #[must_use = "a failed capacity wake can delay queued TTY input"]
    pub fn notify_capacity(&self) -> io::Result<()> {
        // This SC swap follows the bounded channel's SC dequeue reservation.
        // Together with the worker's SC arm and the Full retry's SC fence,
        // it rules out both the retry and the consumer missing each other.
        if self.wake.armed.swap(false, Ordering::SeqCst) {
            self.notify()
        } else {
            Ok(())
        }
    }

    #[cfg(test)]
    pub(super) fn fill_wake_for_test(&self) -> io::Result<usize> {
        let mut writer = &self.wake.writer;
        // Match notify's exact one-byte write until that same write would block.
        let bytes = [1u8];
        let mut written = 0;
        loop {
            match writer.write(&bytes) {
                Ok(0) => return Err(io::Error::from(io::ErrorKind::WriteZero)),
                Ok(count) => written += count,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(written),
                Err(error) => return Err(error),
            }
        }
    }

    pub(super) fn notify_owner_stop(&self) -> io::Result<()> {
        self.notify()
    }

    fn notify(&self) -> io::Result<()> {
        if self.wake.closed.load(Ordering::Acquire) {
            return Ok(());
        }
        let mut writer = &self.wake.writer;
        loop {
            match writer.write(&[1]) {
                Ok(1) => return Ok(()),
                Ok(_) => return Err(io::Error::from(io::ErrorKind::WriteZero)),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                // A full private stream already retains a readable wake.
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(()),
                Err(_) if self.wake.closed.load(Ordering::Acquire) => return Ok(()),
                Err(error) => return Err(error),
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum TtyEventKind {
    Bytes,
    Resize,
}

impl TtyEventKind {
    fn name(self) -> &'static str {
        match self {
            Self::Bytes => "raw-tty-bytes",
            Self::Resize => "window-resize",
        }
    }
}

// Private constructors close the direct-send domain to Unix byte/resize facts.
// Neither uses RenderComms' mapped scroll receipts or lossy GUI event policy.
#[derive(Debug)]
struct PendingTtyEvent {
    kind: TtyEventKind,
    event: InputEvent,
}

impl PendingTtyEvent {
    fn bytes(bytes: Vec<u8>) -> Self {
        Self {
            kind: TtyEventKind::Bytes,
            event: raw_tty_input_event(bytes),
        }
    }

    fn resize(size: TerminalCellSize) -> Self {
        Self {
            kind: TtyEventKind::Resize,
            event: InputEvent::WindowResize {
                width: u32::from(size.columns()),
                height: u32::from(size.rows()),
                scale_factor: 1.0,
                emacs_frame_id: 0,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Lifecycle {
    Running,
    StopAndDrain,
    ShutdownDiscard,
}

#[derive(Clone, Copy, Debug)]
enum WaitMode {
    Poll,
    Wait,
}

#[derive(Clone, Copy, Debug)]
enum ReadState {
    QuerySize,
    PollInput { deadline: Instant },
    ReadReady,
    Delay { deadline: Instant },
    Stopped,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ReaderTestCheckpoint {
    BeforeSizeQuery,
    AfterRawRead {
        len: usize,
    },
    BeforeCapacityArm,
    AfterCapacityArm,
    BeforeCapacityRetry,
    AfterCapacityRetryFull,
    AfterPublish,
    InputDisconnected,
    OwnerStopObserved,
    ShutdownObserved,
    AcquisitionStopped,
    BeforePoll,
    AfterPoll {
        wake_readable: bool,
        input_readable: bool,
        timed_out: bool,
    },
    AfterCapacityWakeRead {
        len: usize,
    },
    Finished,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ReaderTestObservation {
    pub(super) checkpoint: ReaderTestCheckpoint,
    pub(super) pending_len: usize,
    pub(super) has_head: bool,
}

#[cfg(test)]
type TestHook = Option<Arc<dyn Fn(ReaderTestObservation) + Send + Sync>>;

// Moved into exactly one native worker. Only that worker mutates the FIFO,
// in-flight head, acquisition phases and RenderComms; it runs no Lisp. The
// query owner is Send; test observers are per-instance and absent in product.
struct Reader {
    input: OwnedFd,
    wake_reader: UnixStream,
    notifier: TtyInputCapacityNotifier,
    stop: Arc<AtomicBool>,
    comms: RenderComms,
    pending: VecDeque<PendingTtyEvent>,
    head: Option<PendingTtyEvent>,
    lifecycle: Lifecycle,
    read_state: ReadState,
    size_query: TerminalSizeQuery,
    last_size: Option<TerminalCellSize>,
    #[cfg(test)]
    hook: TestHook,
}

impl std::fmt::Debug for Reader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reader")
            .field("input", &self.input)
            .field("wake_reader", &self.wake_reader)
            .field("notifier", &self.notifier)
            .field("stop", &self.stop)
            .field("pending_len", &self.pending.len())
            .field("head", &self.head)
            .field("lifecycle", &self.lifecycle)
            .field("read_state", &self.read_state)
            .field("last_size", &self.last_size)
            .finish_non_exhaustive()
    }
}

// Construction moves this worker and its query resource to a native thread;
// neither holds evaluator/TLS or Lisp heap references.
static_assertions::assert_impl_all!(Reader: Send, std::fmt::Debug);

impl Reader {
    #[cfg(test)]
    fn checkpoint(&self, checkpoint: ReaderTestCheckpoint) {
        if let Some(hook) = &self.hook {
            hook(ReaderTestObservation {
                checkpoint,
                pending_len: self.pending.len(),
                has_head: self.head.is_some(),
            });
        }
    }

    fn observe_owner_stop(&mut self) {
        if self.lifecycle == Lifecycle::Running && self.stop.load(Ordering::Acquire) {
            self.lifecycle = Lifecycle::StopAndDrain;
            #[cfg(test)]
            self.checkpoint(ReaderTestCheckpoint::OwnerStopObserved);
        }
    }

    fn shutdown(&mut self) {
        self.lifecycle = Lifecycle::ShutdownDiscard;
        self.stop.store(true, Ordering::Release);
        self.pending.clear();
        #[cfg(test)]
        self.checkpoint(ReaderTestCheckpoint::ShutdownObserved);
    }

    fn acquire(&mut self, event: PendingTtyEvent) {
        if self.lifecycle != Lifecycle::ShutdownDiscard {
            self.pending.push_back(event);
        }
    }

    // Commands are observed only between completed head deliveries. A full
    // head remains in flight just as the old blocking forwarder's send did.
    fn progress_delivery(&mut self) -> bool {
        let mut progressed = false;
        if self.head.is_none() && self.lifecycle != Lifecycle::ShutdownDiscard {
            match self.comms.cmd_rx.try_recv() {
                Ok(RenderCommand::Lifecycle(LifecycleCommand::Shutdown))
                | Err(TryRecvError::Disconnected) => {
                    self.shutdown();
                    return true;
                }
                Ok(_) => progressed = true,
                Err(TryRecvError::Empty) => {}
            }
            self.head = self.pending.pop_front();
        }
        if self.head.is_none() {
            return progressed;
        }
        if self.try_head() {
            return true;
        }
        #[cfg(test)]
        self.checkpoint(ReaderTestCheckpoint::BeforeCapacityArm);
        // Crossbeam's bounded-array Full path fences before checking its SC
        // head reservation. Keep the arm and the bridge's post-pop swap SC:
        // a pop missed by that retry must leave a remembered capacity wake.
        self.notifier.wake.armed.store(true, Ordering::SeqCst);
        #[cfg(test)]
        self.checkpoint(ReaderTestCheckpoint::AfterCapacityArm);
        #[cfg(test)]
        self.checkpoint(ReaderTestCheckpoint::BeforeCapacityRetry);
        // A pop before arming may not have notified. This retry closes that
        // gap; a pop after this retry leaves a remembered byte for poll.
        if self.try_head() {
            return true;
        }
        #[cfg(test)]
        self.checkpoint(ReaderTestCheckpoint::AfterCapacityRetryFull);
        progressed
    }

    // Returns false only on Full, retaining exactly the same owned envelope.
    fn try_head(&mut self) -> bool {
        let Some(PendingTtyEvent { kind, event }) = self.head.take() else {
            return true;
        };
        match self.comms.input_tx.try_send(event) {
            Ok(()) => {
                self.notifier.wake.armed.store(false, Ordering::Release);
                if matches!(kind, TtyEventKind::Resize) {
                    tracing::debug!("send_input: queued {}", kind.name());
                }
                #[cfg(test)]
                self.checkpoint(ReaderTestCheckpoint::AfterPublish);
                true
            }
            Err(TrySendError::Full(event)) => {
                self.head = Some(PendingTtyEvent { kind, event });
                false
            }
            Err(TrySendError::Disconnected(_)) => {
                self.notifier.wake.armed.store(false, Ordering::Release);
                tracing::warn!(
                    "send_input: dropped {} because the input queue is disconnected",
                    kind.name()
                );
                #[cfg(test)]
                self.checkpoint(ReaderTestCheckpoint::InputDisconnected);
                true
            }
        }
    }

    // Acquisition is the original raw loop expressed in phases. Transport
    // wakes never restart size sampling, read deadlines or error/EOF delays.
    fn progress_acquisition(&mut self) -> bool {
        match self.read_state {
            ReadState::QuerySize => {
                if self.lifecycle != Lifecycle::Running {
                    self.read_state = ReadState::Stopped;
                    #[cfg(test)]
                    self.checkpoint(ReaderTestCheckpoint::AcquisitionStopped);
                    return true;
                }
                #[cfg(test)]
                self.checkpoint(ReaderTestCheckpoint::BeforeSizeQuery);
                if let Ok(size) = self.size_query.query()
                    && self.last_size != Some(size)
                {
                    self.last_size = Some(size);
                    self.acquire(PendingTtyEvent::resize(size));
                }
                self.read_state = ReadState::PollInput {
                    deadline: Instant::now() + TTY_READER_POLL_INTERVAL,
                };
                true
            }
            ReadState::ReadReady => {
                let mut bytes = [0u8; GNU_KBD_BUFFER_SIZE - 1];
                // SAFETY: the worker owns the valid input descriptor and is
                // its sole reader; bytes is writable for exactly bytes.len().
                let count = unsafe {
                    libc::read(
                        self.input.as_raw_fd(),
                        bytes.as_mut_ptr().cast(),
                        bytes.len(),
                    )
                };
                if count < 0 {
                    let error = io::Error::last_os_error();
                    if matches!(
                        error.kind(),
                        io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                    ) {
                        self.read_state = ReadState::QuerySize;
                    } else {
                        tracing::warn!("tty_input: read error: {}", error);
                        self.read_state = ReadState::Delay {
                            deadline: Instant::now() + Duration::from_millis(100),
                        };
                    }
                } else if count == 0 {
                    self.read_state = ReadState::Delay {
                        deadline: Instant::now() + Duration::from_millis(10),
                    };
                } else {
                    #[cfg(test)]
                    self.checkpoint(ReaderTestCheckpoint::AfterRawRead {
                        len: count as usize,
                    });
                    let event = PendingTtyEvent::bytes(bytes[..count as usize].to_vec());
                    tracing::debug!("tty_input: raw byte batch {:?}", event.event);
                    self.acquire(event);
                    self.read_state = ReadState::QuerySize;
                }
                true
            }
            ReadState::Delay { deadline } if Instant::now() >= deadline => {
                self.read_state = ReadState::QuerySize;
                true
            }
            ReadState::PollInput { .. } | ReadState::Delay { .. } | ReadState::Stopped => false,
        }
    }

    fn wait(&mut self, mode: WaitMode) {
        let now = Instant::now();
        let acquisition_deadline = match self.read_state {
            ReadState::PollInput { deadline } | ReadState::Delay { deadline } => deadline,
            ReadState::QuerySize | ReadState::ReadReady => return,
            ReadState::Stopped => now + TTY_READER_POLL_INTERVAL,
        };
        // Command observation is bounded even while acquisition has a100ms
        // error delay. A held head still defers command service as before.
        let timeout = match mode {
            WaitMode::Poll => Duration::ZERO,
            WaitMode::Wait => acquisition_deadline
                .min(now + TTY_READER_POLL_INTERVAL)
                .saturating_duration_since(now),
        };
        let fractional_millisecond = if timeout.subsec_nanos() % 1_000_000 == 0 {
            0
        } else {
            1
        };
        let timeout_ms = timeout
            .as_millis()
            .saturating_add(fractional_millisecond)
            .min(libc::c_int::MAX as u128) as libc::c_int;
        let mut fds = [
            libc::pollfd {
                fd: if matches!(self.read_state, ReadState::PollInput { .. }) {
                    self.input.as_raw_fd()
                } else {
                    -1
                },
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: self.wake_reader.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        #[cfg(test)]
        self.checkpoint(ReaderTestCheckpoint::BeforePoll);
        // SAFETY: fds is an initialized writable array. Its descriptors are
        // worker-owned and live, or -1 (the documented ignored poll entry).
        let result = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, timeout_ms) };
        #[cfg(test)]
        self.checkpoint(ReaderTestCheckpoint::AfterPoll {
            wake_readable: fds[1].revents & libc::POLLIN != 0,
            input_readable: fds[0].revents & libc::POLLIN != 0,
            timed_out: result == 0,
        });
        if result < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                if matches!(self.read_state, ReadState::PollInput { .. }) {
                    self.read_state = ReadState::QuerySize;
                }
            } else {
                tracing::warn!("tty_input: poll error: {}", error);
                self.read_state = ReadState::Delay {
                    deadline: Instant::now() + Duration::from_millis(100),
                };
            }
            return;
        }
        if fds[1].revents & libc::POLLIN != 0 {
            let mut bytes = [0u8; 128];
            loop {
                match self.wake_reader.read(&mut bytes) {
                    Ok(0) => break,
                    Ok(count) => {
                        #[cfg(test)]
                        self.checkpoint(ReaderTestCheckpoint::AfterCapacityWakeRead { len: count });
                        #[cfg(not(test))]
                        let _ = count;
                    }
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                    Err(error) => {
                        tracing::warn!("tty_input: capacity wake read error: {}", error);
                        break;
                    }
                }
            }
        }
        if let ReadState::PollInput { deadline } = self.read_state {
            if fds[0].revents & libc::POLLIN != 0 {
                self.read_state = ReadState::ReadReady;
            } else if fds[0].revents != 0 || Instant::now() >= deadline {
                // HUP without readable data and poll timeouts start the next
                // original acquisition iteration; neither invents EOF/close.
                self.read_state = ReadState::QuerySize;
            }
        }
    }

    fn run(mut self) {
        self.last_size = self.size_query.query().ok();
        loop {
            self.observe_owner_stop();
            let delivery_progress = self.progress_delivery();
            let acquisition_progress = self.progress_acquisition();
            if matches!(self.read_state, ReadState::Stopped)
                && self.head.is_none()
                && self.pending.is_empty()
            {
                self.notifier.wake.closed.store(true, Ordering::Release);
                self.notifier.wake.armed.store(false, Ordering::Release);
                // Closing the sole producer lets the bridge drain its accepted
                // bounded prefix. The reader handle owns no extra sender.
                drop(self.comms);
                // Comms is already closed; the remaining native fields are
                // released by this return, before the owner join completes.
                #[cfg(test)]
                if let Some(hook) = &self.hook {
                    hook(ReaderTestObservation {
                        checkpoint: ReaderTestCheckpoint::Finished,
                        pending_len: self.pending.len(),
                        has_head: self.head.is_some(),
                    });
                }
                return;
            }
            if !acquisition_progress {
                if !delivery_progress {
                    self.wait(WaitMode::Wait);
                } else if matches!(self.read_state, ReadState::PollInput { .. }) {
                    // Draining an acquired backlog must not starve fresh raw
                    // readiness. This is nonblocking while delivery progresses.
                    self.wait(WaitMode::Poll);
                }
            }
        }
    }
}

pub(super) fn spawn_default(comms: RenderComms) -> io::Result<TtyInputReader> {
    // dup owns a CLOEXEC reference to the slave without changing the shared
    // open-file description's flags or closing the process's stdin.
    // SAFETY: fcntl borrows stdin and creates a new CLOEXEC descriptor; it
    // does not take ownership of stdin or alter shared open-file flags.
    let fd = unsafe { libc::fcntl(libc::STDIN_FILENO, libc::F_DUPFD_CLOEXEC, 0) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fd is the successful new fcntl result, transferred exactly once.
    let input = unsafe { OwnedFd::from_raw_fd(fd) };
    let query = TerminalSizeQuery::new();
    #[cfg(test)]
    {
        spawn_with_source(comms, input, query, None)
    }
    #[cfg(not(test))]
    {
        spawn_with_source(comms, input, query)
    }
}

pub(super) fn spawn_with_source(
    comms: RenderComms,
    input: OwnedFd,
    size_query: TerminalSizeQuery,
    #[cfg(test)] hook: TestHook,
) -> io::Result<TtyInputReader> {
    let (wake_reader, writer) = UnixStream::pair()?;
    wake_reader.set_nonblocking(true)?;
    writer.set_nonblocking(true)?;
    let notifier = TtyInputCapacityNotifier {
        wake: Arc::new(CapacityWake {
            writer,
            armed: AtomicBool::new(false),
            closed: AtomicBool::new(false),
        }),
    };
    let stop = Arc::new(AtomicBool::new(false));
    let worker = Reader {
        input,
        wake_reader,
        notifier: notifier.clone(),
        stop: Arc::clone(&stop),
        comms,
        pending: VecDeque::new(),
        head: None,
        lifecycle: Lifecycle::Running,
        read_state: ReadState::QuerySize,
        size_query,
        last_size: None,
        #[cfg(test)]
        hook,
    };
    let handle = thread::Builder::new()
        .name("tty-input-raw".to_string())
        .spawn(move || worker.run())?;
    Ok(TtyInputReader {
        handle: Some(handle),
        stop,
        capacity_notifier: notifier,
    })
}
