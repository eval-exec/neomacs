//! Direct primary publication through the production Unix acquisition machine.
//! Owned PTYs and checkpoint gates determine order; deadlines only catch deadlocks.
//! The legacy queue stays full or is explicitly dropped; neither gates direct input.

use super::unix_reader::{ReaderTestCheckpoint as Checkpoint, ReaderTestObservation};
use super::*;
use crate::thread_comm::{LifecycleCommand, RenderCommand, ThreadComms};
use crossbeam_channel::{Receiver, Sender};
use std::fs::File;
use std::io::Write;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::Mutex;
use std::time::Instant;

const DEADLOCK_CAP: Duration = Duration::from_secs(30);
const LEGACY_CAPACITY: usize = 4096;

#[derive(Debug)]
struct Gate {
    checkpoint: Checkpoint,
    release: Receiver<()>,
}

#[derive(Debug)]
struct GateHold(Sender<()>);

impl Drop for GateHold {
    fn drop(&mut self) {
        // Unbounded test-control send: releasing a checkpoint never blocks.
        let _ = self.0.send(());
    }
}

#[derive(Debug)]
enum Observed {
    Passed(ReaderTestObservation),
    Held(ReaderTestObservation),
}

/// One mutex owns the complete per-test gate lifecycle. Canceled cannot be armed.
#[derive(Debug)]
enum HookGate {
    Open,
    Armed(Gate),
    Canceled,
}

#[derive(Debug)]
struct HookState {
    gate: Mutex<HookGate>,
    releases: Mutex<Vec<Sender<()>>>,
    history: Mutex<Vec<ReaderTestObservation>>,
    observed: Sender<Observed>,
}

#[derive(Debug)]
struct Hooks {
    state: Arc<HookState>,
    observed: Receiver<Observed>,
}

// The actual reader hook shares this state across the native worker and test.
const _: fn() = || {
    fn assert_shareable_debug<T: Send + Sync + std::fmt::Debug>() {}
    assert_shareable_debug::<HookState>();
    assert_shareable_debug::<Hooks>();
};

impl Hooks {
    fn new() -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        Self {
            state: Arc::new(HookState {
                gate: Mutex::new(HookGate::Open),
                releases: Mutex::new(Vec::new()),
                history: Mutex::new(Vec::new()),
                observed: tx,
            }),
            observed: rx,
        }
    }

    fn callback(&self) -> Arc<dyn Fn(ReaderTestObservation) + Send + Sync> {
        let state = Arc::clone(&self.state);
        Arc::new(move |observation| {
            state.history.lock().unwrap().push(observation);
            let gate = {
                let mut armed = state.gate.lock().unwrap();
                match std::mem::replace(&mut *armed, HookGate::Open) {
                    HookGate::Armed(gate) => {
                        if gate.checkpoint == observation.checkpoint {
                            Some(gate)
                        } else {
                            *armed = HookGate::Armed(gate);
                            None
                        }
                    }
                    HookGate::Open => None,
                    HookGate::Canceled => {
                        *armed = HookGate::Canceled;
                        None
                    }
                }
            };
            let event = match &gate {
                Some(_) => Observed::Held(observation),
                None => Observed::Passed(observation),
            };
            let _ = state.observed.send(event);
            if let Some(gate) = gate {
                let _ = gate.release.recv();
            }
        })
    }

    fn arm(&self, checkpoint: Checkpoint) -> GateHold {
        let (tx, rx) = crossbeam_channel::unbounded();
        let mut gate = self.state.gate.lock().unwrap();
        match &*gate {
            HookGate::Open => {}
            HookGate::Armed(_) => panic!("only one native checkpoint is armed at a time"),
            HookGate::Canceled => panic!("a canceled fixture cannot arm another checkpoint"),
        }
        *gate = HookGate::Armed(Gate {
            checkpoint,
            release: rx,
        });
        self.state.releases.lock().unwrap().push(tx.clone());
        GateHold(tx)
    }

    fn wait_gate(&self, checkpoint: Checkpoint) -> ReaderTestObservation {
        let deadline = Instant::now() + DEADLOCK_CAP;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(
                !remaining.is_zero(),
                "native checkpoint {checkpoint:?} not reached"
            );
            let event = self
                .observed
                .recv_timeout(remaining)
                .expect("native reader observation");
            match event {
                Observed::Held(observation) => {
                    assert_eq!(observation.checkpoint, checkpoint);
                    return observation;
                }
                Observed::Passed(observation) => assert_ne!(
                    observation.checkpoint,
                    Checkpoint::Finished,
                    "reader finished before checkpoint"
                ),
            }
        }
    }

    fn history(&self) -> Vec<ReaderTestObservation> {
        self.state.history.lock().unwrap().clone()
    }

    fn release_all(&self) {
        *self
            .state
            .gate
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = HookGate::Canceled;
        let releases = self
            .state
            .releases
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for release in releases.iter() {
            let _ = release.send(());
        }
    }
}

/// Fixture-owned native descriptors; no stdin replacement or process-global hook.
#[derive(Debug)]
struct NativePty {
    master: File,
    slave: OwnedFd,
}

impl NativePty {
    fn new() -> Self {
        let mut master = -1;
        let mut slave = -1;
        let mut size = libc::winsize {
            ws_row: 24,
            ws_col: 80,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: initialized output slots and winsize; no terminal name is needed.
        assert_eq!(
            unsafe {
                libc::openpty(
                    &mut master,
                    &mut slave,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    &mut size,
                )
            },
            0,
            "open test PTY: {}",
            io::Error::last_os_error()
        );
        // SAFETY: successful openpty transfers two distinct descriptors to this fixture.
        let master = unsafe { OwnedFd::from_raw_fd(master) };
        // SAFETY: this is the other descriptor just returned by openpty.
        let slave = unsafe { OwnedFd::from_raw_fd(slave) };
        for fd in [&master, &slave] {
            // SAFETY: each fd is live and owned across the fcntl calls.
            let flags = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETFD) };
            assert!(flags >= 0);
            // SAFETY: preserve descriptor flags and forbid inheritance into the writer exec.
            assert_eq!(
                unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFD, flags | libc::FD_CLOEXEC) },
                0
            );
        }
        // SAFETY: termios contains C scalar/array fields with valid zero representations.
        let mut modes = unsafe { std::mem::zeroed::<libc::termios>() };
        // SAFETY: the owned slave is live and modes is writable local storage.
        assert_eq!(unsafe { libc::tcgetattr(slave.as_raw_fd(), &mut modes) }, 0);
        // SAFETY: cfmakeraw only mutates the initialized local termios.
        unsafe { libc::cfmakeraw(&mut modes) };
        // SAFETY: the terminal belongs exclusively to this fixture.
        assert_eq!(
            unsafe { libc::tcsetattr(slave.as_raw_fd(), libc::TCSANOW, &modes) },
            0
        );
        Self {
            master: File::from(master),
            slave,
        }
    }

    fn size_query(&self) -> TerminalSizeQuery {
        let slave = self.slave.try_clone().expect("size-query lease");
        TerminalSizeQuery::ControllingTerminal(File::from(slave))
    }

    fn resize(&self, dimensions: TerminalCellSize) {
        let size = libc::winsize {
            ws_row: u16::try_from(u32::from(dimensions.rows())).unwrap(),
            ws_col: u16::try_from(u32::from(dimensions.columns())).unwrap(),
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: borrowed live owned slave and initialized winsize.
        assert_eq!(
            unsafe { libc::ioctl(self.slave.as_raw_fd(), libc::TIOCSWINSZ, &size) },
            0
        );
    }

    fn send(&mut self, bytes: &[u8]) {
        self.master.write_all(bytes).expect("write real PTY input");
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Fact {
    Bytes(Vec<u8>),
    Resize(TerminalCellSize),
}

impl From<PrimaryTtyInput> for Fact {
    fn from(input: PrimaryTtyInput) -> Self {
        match input {
            PrimaryTtyInput::Bytes(bytes) => Self::Bytes(bytes),
            PrimaryTtyInput::Resize(size) => Self::Resize(size),
        }
    }
}

/// Unbounded, per-instance test publication. The audit records every call,
/// including a failed send, independently of the receiver's lifetime.
#[derive(Debug)]
struct TestPublisher {
    output: Sender<Fact>,
    attempts: Arc<Mutex<Vec<Fact>>>,
}

impl PrimaryTtyPublisher for TestPublisher {
    fn publish(&mut self, input: PrimaryTtyInput) -> Publication {
        let fact = Fact::from(input);
        self.attempts.lock().unwrap().push(fact.clone());
        match self.output.send(fact) {
            Ok(()) => Publication::Delivered,
            Err(_) => Publication::ReceiverClosed,
        }
    }
}

static_assertions::assert_impl_all!(TestPublisher: Send, std::fmt::Debug);
static_assertions::assert_not_impl_any!(TestPublisher: Clone, Copy);

#[derive(Clone, Copy, Debug)]
enum LegacyReceiverPolicy {
    RetainFull,
    Drop,
}

#[derive(Debug)]
enum LegacyInputReceiver {
    Full(Receiver<InputEvent>),
    Disconnected,
}

#[derive(Debug)]
struct DirectFixture {
    reader: Option<TtyInputReader>,
    output: Option<Receiver<Fact>>,
    attempts: Arc<Mutex<Vec<Fact>>>,
    commands: Option<Sender<RenderCommand>>,
    legacy_input: LegacyInputReceiver,
    pty: NativePty,
    hooks: Hooks,
}

impl DirectFixture {
    fn start(hooks: Hooks) -> Self {
        Self::start_with_legacy(hooks, LegacyReceiverPolicy::RetainFull)
    }

    fn start_with_legacy(hooks: Hooks, policy: LegacyReceiverPolicy) -> Self {
        let pty = NativePty::new();
        let (emacs, render) = ThreadComms::new().split();
        assert_eq!(render.input_tx.capacity(), Some(LEGACY_CAPACITY));
        for _ in 0..LEGACY_CAPACITY {
            render
                .input_tx
                .try_send(raw_tty_input_event(vec![b'_']))
                .unwrap();
        }
        let legacy_input = match policy {
            LegacyReceiverPolicy::RetainFull => LegacyInputReceiver::Full(emacs.input_rx),
            LegacyReceiverPolicy::Drop => {
                drop(emacs.input_rx);
                LegacyInputReceiver::Disconnected
            }
        };
        let (tx, rx) = crossbeam_channel::unbounded();
        let attempts = Arc::new(Mutex::new(Vec::new()));
        let publisher = TestPublisher {
            output: tx,
            attempts: Arc::clone(&attempts),
        };
        let reader = TtyInputReader::spawn_primary_with_test_source(
            render,
            pty.slave.try_clone().expect("direct reader slave lease"),
            pty.size_query(),
            publisher,
            Some(hooks.callback()),
        )
        .expect("spawn production direct reader");
        Self {
            reader: Some(reader),
            output: Some(rx),
            attempts,
            commands: Some(emacs.cmd_tx),
            legacy_input,
            pty,
            hooks,
        }
    }

    fn recv(&self) -> Fact {
        self.output
            .as_ref()
            .unwrap()
            .recv_timeout(DEADLOCK_CAP)
            .expect("direct publication without legacy capacity credit")
    }

    fn request_stop(&self) {
        self.reader
            .as_ref()
            .unwrap()
            .request_stop_for_test()
            .expect("wake direct reader owner stop");
    }

    fn finish(mut self) -> Vec<Fact> {
        self.request_stop();
        self.hooks.release_all();
        let mut remaining = Vec::new();
        if let Some(output) = self.output.take() {
            let deadline = Instant::now() + DEADLOCK_CAP;
            loop {
                let time_left = deadline.saturating_duration_since(Instant::now());
                assert!(!time_left.is_zero(), "direct publisher failed to close");
                match output.recv_timeout(time_left) {
                    Ok(fact) => remaining.push(fact),
                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                        panic!("direct publisher drain stalled")
                    }
                }
            }
        }
        self.reader.take().unwrap().join();
        let history = self.hooks.history();
        assert!(
            history
                .iter()
                .any(|event| event.checkpoint == Checkpoint::Finished)
        );
        assert!(
            history.iter().all(|event| !matches!(
                event.checkpoint,
                Checkpoint::BeforeCapacityArm
                    | Checkpoint::AfterCapacityArm
                    | Checkpoint::BeforeCapacityRetry
                    | Checkpoint::AfterCapacityRetryFull
            )),
            "direct publication must never use bounded-capacity retry"
        );
        match &self.legacy_input {
            LegacyInputReceiver::Full(receiver) => assert_eq!(
                receiver.len(),
                LEGACY_CAPACITY,
                "direct publication must leave the full legacy queue untouched"
            ),
            LegacyInputReceiver::Disconnected => {}
        }
        remaining
    }
}

impl Drop for DirectFixture {
    fn drop(&mut self) {
        // Failed assertions release all native gates and stop the owned reader.
        // Like the legacy fixture, Drop never waits for a worker.
        self.hooks.release_all();
        if let Some(reader) = &self.reader {
            let _ = reader.request_stop_for_test();
        }
        self.output.take();
    }
}

#[test]
fn direct_reader_preserves_utf8_escape_csi_and_control_bytes_across_real_reads() {
    let cases: &[&[u8]] = &[
        "中".as_bytes(),
        "😀".as_bytes(),
        b"\x1b",
        b"\x1b[A",
        b"\x1b[1;5D",
        b"\0\x03\xff",
        &[0xa4, 0x40],
    ];
    for encoded in cases {
        let mut fixture = DirectFixture::start(Hooks::new());
        for byte in *encoded {
            fixture.pty.send(&[*byte]);
            // Receiving before the next write proves distinct host reads. In
            // particular standalone ESC is published without a decoder timer.
            assert_eq!(fixture.recv(), Fact::Bytes(vec![*byte]));
        }
        assert!(fixture.finish().is_empty());
    }
}

#[test]
fn direct_reader_orders_bytes_resize_bytes_without_a_capacity_bridge() {
    let hooks = Hooks::new();
    let a = hooks.arm(Checkpoint::AfterPublish);
    let mut fixture = DirectFixture::start(hooks);
    fixture.pty.send(b"A");
    fixture.hooks.wait_gate(Checkpoint::AfterPublish);
    let query = fixture.hooks.arm(Checkpoint::BeforeSizeQuery);
    drop(a);
    fixture.hooks.wait_gate(Checkpoint::BeforeSizeQuery);
    let size = TerminalCellSize::new(100.into(), 31.into());
    fixture.pty.resize(size);
    fixture.pty.send(b"B");
    drop(query);
    assert_eq!(fixture.recv(), Fact::Bytes(b"A".to_vec()));
    assert_eq!(fixture.recv(), Fact::Resize(size));
    assert_eq!(fixture.recv(), Fact::Bytes(b"B".to_vec()));
    assert!(fixture.finish().is_empty());
}

#[test]
fn direct_reader_owner_stop_drains_bytes_already_read_before_publication() {
    let hooks = Hooks::new();
    let read = hooks.arm(Checkpoint::AfterRawRead { len: 1 });
    let mut fixture = DirectFixture::start(hooks);
    fixture.pty.send(b"\x03");
    fixture.hooks.wait_gate(Checkpoint::AfterRawRead { len: 1 });
    // This byte is physically read, but the hook runs before acquire/publish.
    fixture.request_stop();
    drop(read);
    assert_eq!(fixture.finish(), vec![Fact::Bytes(vec![3])]);
}

#[test]
fn direct_reader_shutdown_keeps_accepted_prefix_and_discards_later_read() {
    let mut fixture = DirectFixture::start(Hooks::new());
    fixture.pty.send(b"A");
    assert_eq!(fixture.recv(), Fact::Bytes(b"A".to_vec()));
    let read = fixture.hooks.arm(Checkpoint::AfterRawRead { len: 1 });
    fixture.pty.send(b"B");
    fixture.hooks.wait_gate(Checkpoint::AfterRawRead { len: 1 });
    fixture
        .commands
        .as_ref()
        .unwrap()
        .send(RenderCommand::Lifecycle(LifecycleCommand::Shutdown))
        .unwrap();
    let shutdown = fixture.hooks.arm(Checkpoint::ShutdownObserved);
    drop(read);
    fixture.hooks.wait_gate(Checkpoint::ShutdownObserved);
    assert_eq!(
        *fixture.attempts.lock().unwrap(),
        vec![Fact::Bytes(b"A".to_vec())]
    );
    drop(shutdown);
    assert!(fixture.finish().is_empty());
}

#[test]
fn direct_reader_closed_command_owner_discards_read_but_not_accepted_prefix() {
    let mut fixture = DirectFixture::start(Hooks::new());
    fixture.pty.send(b"A");
    assert_eq!(fixture.recv(), Fact::Bytes(b"A".to_vec()));
    let read = fixture.hooks.arm(Checkpoint::AfterRawRead { len: 1 });
    fixture.pty.send(b"B");
    fixture.hooks.wait_gate(Checkpoint::AfterRawRead { len: 1 });
    fixture.commands.take();
    let shutdown = fixture.hooks.arm(Checkpoint::ShutdownObserved);
    drop(read);
    fixture.hooks.wait_gate(Checkpoint::ShutdownObserved);
    assert_eq!(
        *fixture.attempts.lock().unwrap(),
        vec![Fact::Bytes(b"A".to_vec())]
    );
    drop(shutdown);
    assert!(fixture.finish().is_empty());
}

#[test]
fn direct_reader_calls_closed_publisher_once_then_continues_raw_acquisition() {
    let mut fixture = DirectFixture::start(Hooks::new());
    fixture.pty.send(b"A");
    assert_eq!(fixture.recv(), Fact::Bytes(b"A".to_vec()));
    let disconnected = fixture.hooks.arm(Checkpoint::InputDisconnected);
    fixture.output.take();
    fixture.pty.send(b"B");
    fixture.hooks.wait_gate(Checkpoint::InputDisconnected);
    let next_read = fixture.hooks.arm(Checkpoint::AfterRawRead { len: 1 });
    drop(disconnected);
    fixture.pty.send(b"C");
    fixture.hooks.wait_gate(Checkpoint::AfterRawRead { len: 1 });
    let discarded = fixture.hooks.arm(Checkpoint::InputDisconnected);
    drop(next_read);
    fixture.hooks.wait_gate(Checkpoint::InputDisconnected);
    // C reached the real raw reader and completed its closed-delivery path,
    // but the publisher was dropped at B's first ReceiverClosed outcome.
    assert_eq!(
        *fixture.attempts.lock().unwrap(),
        vec![Fact::Bytes(b"A".to_vec()), Fact::Bytes(b"B".to_vec()),]
    );
    fixture.request_stop();
    drop(discarded);
    assert!(fixture.finish().is_empty());
}

#[test]
fn direct_reader_owner_stop_wakes_poll_without_capacity_credit_or_input() {
    let hooks = Hooks::new();
    let poll = hooks.arm(Checkpoint::BeforePoll);
    let fixture = DirectFixture::start(hooks);
    fixture.hooks.wait_gate(Checkpoint::BeforePoll);
    fixture.request_stop();
    let wake = fixture.hooks.arm(Checkpoint::AfterPoll {
        wake_readable: true,
        input_readable: false,
        timed_out: false,
    });
    drop(poll);
    fixture.hooks.wait_gate(Checkpoint::AfterPoll {
        wake_readable: true,
        input_readable: false,
        timed_out: false,
    });
    // A positive private-wake readiness observation proves wake ownership;
    // a 50ms timeout or elapsed-time assertion cannot satisfy this checkpoint.
    drop(wake);
    let history_owner = Arc::clone(&fixture.hooks.state);
    assert!(fixture.finish().is_empty());
    let history = history_owner.history.lock().unwrap();
    assert!(history.iter().any(|event| matches!(event.checkpoint,
        Checkpoint::AfterCapacityWakeRead { len } if len > 0
    )));
    assert!(
        history
            .iter()
            .any(|event| event.checkpoint == Checkpoint::OwnerStopObserved)
    );
}

#[test]
fn direct_reader_delivers_with_unused_legacy_receiver_dropped_then_stops() {
    let mut fixture = DirectFixture::start_with_legacy(Hooks::new(), LegacyReceiverPolicy::Drop);
    fixture.pty.send(b"A");
    assert_eq!(fixture.recv(), Fact::Bytes(b"A".to_vec()));
    let history_owner = Arc::clone(&fixture.hooks.state);
    assert!(fixture.finish().is_empty());
    let history = history_owner.history.lock().unwrap();
    assert!(
        history
            .iter()
            .all(|event| event.checkpoint != Checkpoint::InputDisconnected),
        "a disconnected unused display queue must not close the direct publisher"
    );
}
