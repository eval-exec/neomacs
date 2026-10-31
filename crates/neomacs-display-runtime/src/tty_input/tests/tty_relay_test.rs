//! Deterministic observations of the production Unix reader on owned PTYs.
//! Deadlines below only guard broken synchronization; no latency is asserted.

use super::unix_reader::{ReaderTestCheckpoint as Checkpoint, ReaderTestObservation};
use super::*;
use crate::thread_comm::{LifecycleCommand, RenderCommand, ThreadComms};
use crossbeam_channel::{Receiver, Sender, TrySendError};
use std::fs::File;
use std::io::Write;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Mutex;
use std::time::Instant;

const DEADLOCK_CAP: Duration = Duration::from_secs(30);
const CAPACITY: usize = 4096;
const FILL: u8 = b'_';

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
struct Observed {
    observation: ReaderTestObservation,
    gated: bool,
}

#[derive(Debug)]
struct HookState {
    gate: Mutex<Option<Gate>>,
    releases: Mutex<Vec<Sender<()>>>,
    history: Mutex<Vec<ReaderTestObservation>>,
    canceled: AtomicBool,
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
                gate: Mutex::new(None),
                releases: Mutex::new(Vec::new()),
                history: Mutex::new(Vec::new()),
                canceled: AtomicBool::new(false),
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
                if !state.canceled.load(Ordering::Acquire)
                    && armed
                        .as_ref()
                        .is_some_and(|gate| gate.checkpoint == observation.checkpoint)
                {
                    armed.take()
                } else {
                    None
                }
            };
            let _ = state.observed.send(Observed {
                observation,
                gated: gate.is_some(),
            });
            if let Some(gate) = gate {
                let _ = gate.release.recv();
            }
        })
    }

    fn arm(&self, checkpoint: Checkpoint) -> GateHold {
        let (tx, rx) = crossbeam_channel::unbounded();
        let mut gate = self.state.gate.lock().unwrap();
        assert!(
            gate.is_none(),
            "only one native checkpoint is armed at a time"
        );
        *gate = Some(Gate {
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
            if event.gated {
                assert_eq!(event.observation.checkpoint, checkpoint);
                return event.observation;
            }
            assert_ne!(
                event.observation.checkpoint,
                Checkpoint::Finished,
                "reader finished before checkpoint"
            );
        }
    }

    fn saw(&self, checkpoint: Checkpoint) -> bool {
        self.state
            .history
            .lock()
            .unwrap()
            .iter()
            .any(|event| event.checkpoint == checkpoint)
    }

    fn release_all(&self) {
        self.state.canceled.store(true, Ordering::Release);
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

    fn resize(&self, columns: u16, rows: u16) {
        let size = libc::winsize {
            ws_row: rows,
            ws_col: columns,
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

#[derive(Debug)]
enum InitialQueue {
    Empty,
    Full,
}

#[derive(Debug)]
struct ReaderFixture {
    reader: Option<TtyInputReader>,
    input: Option<Receiver<InputEvent>>,
    capacity: Option<TtyInputCapacityNotifier>,
    commands: crate::thread_comm::RenderCommandSender,
    pty: NativePty,
    hooks: Hooks,
}

impl ReaderFixture {
    fn start(initial: InitialQueue, hooks: Hooks) -> Self {
        let pty = NativePty::new();
        let (emacs, render) = ThreadComms::new().split();
        assert_eq!(render.input_tx.capacity(), Some(CAPACITY));
        if matches!(initial, InitialQueue::Full) {
            for _ in 0..CAPACITY {
                render
                    .input_tx
                    .try_send(raw_tty_input_event(vec![FILL]))
                    .unwrap();
            }
            assert!(matches!(
                render.input_tx.try_send(raw_tty_input_event(vec![b'!'])),
                Err(TrySendError::Full(_))
            ));
        }
        let reader = TtyInputReader::spawn_with_test_source(
            render,
            pty.slave.try_clone().expect("reader-owned slave lease"),
            pty.size_query(),
            Some(hooks.callback()),
        )
        .expect("spawn production Unix reader");
        let capacity = reader.capacity_notifier();
        Self {
            reader: Some(reader),
            input: Some(emacs.input_rx),
            capacity,
            commands: emacs.cmd_tx,
            pty,
            hooks,
        }
    }

    fn recv(&self) -> InputEvent {
        let event = self
            .input
            .as_ref()
            .unwrap()
            .recv_timeout(DEADLOCK_CAP)
            .expect("actual bounded input");
        self.capacity
            .as_ref()
            .expect("Unix capacity notifier")
            .notify_capacity()
            .expect("publish freed capacity");
        event
    }

    fn request_stop(&self) {
        self.reader
            .as_ref()
            .unwrap()
            .request_stop_for_test()
            .expect("wake acquisition stop");
    }

    fn finish(mut self) -> Vec<InputEvent> {
        self.request_stop();
        self.hooks.release_all();
        let input = self.input.take();
        let mut events = Vec::new();
        if let Some(input) = input {
            let deadline = Instant::now() + DEADLOCK_CAP;
            loop {
                let remaining = deadline.saturating_duration_since(Instant::now());
                assert!(!remaining.is_zero(), "reader did not drain and close");
                match input.recv_timeout(remaining) {
                    Ok(event) => {
                        events.push(event);
                        if let Some(capacity) = &self.capacity {
                            // A closed worker can leave an accepted prefix to drain.
                            let _ = capacity.notify_capacity();
                        }
                    }
                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                        panic!("reader drain stalled")
                    }
                }
            }
        }
        self.reader.take().unwrap().join();
        assert!(self.hooks.saw(Checkpoint::Finished));
        events
    }
}

impl Drop for ReaderFixture {
    fn drop(&mut self) {
        // Emergency cleanup never waits in Drop. The normal path explicitly
        // finishes/joins; a failed fixture unblocks gates and disconnects its
        // receiver before releasing the reader handle.
        self.hooks.release_all();
        if let Some(reader) = &self.reader {
            let _ = reader.request_stop_for_test();
        }
        self.input.take();
    }
}

fn bytes(event: InputEvent) -> Vec<u8> {
    match event {
        InputEvent::RawTtyBytes {
            bytes,
            emacs_frame_id: 0,
        } => bytes,
        other => panic!("expected unchanged primary raw bytes, got {other:?}"),
    }
}

#[test]
fn real_reader_keeps_acquiring_behind_the_full_4096_event_queue_and_stop_drains() {
    let hooks = Hooks::new();
    let first = hooks.arm(Checkpoint::AfterRawRead { len: 1 });
    let mut fixture = ReaderFixture::start(InitialQueue::Full, hooks);
    fixture.pty.send(b"A");
    fixture.hooks.wait_gate(Checkpoint::AfterRawRead { len: 1 });
    let second = fixture.hooks.arm(Checkpoint::AfterRawRead { len: 1 });
    drop(first);
    // Primary raw acquisition preserves C-g as a byte; only the surviving
    // bridge raises QuitRequest. This complements the real busy-Lisp TUI test.
    fixture.pty.send(b"\x07");
    let observed = fixture.hooks.wait_gate(Checkpoint::AfterRawRead { len: 1 });
    assert!(
        observed.has_head || observed.pending_len > 0,
        "A remains pending while later C-g is acquired"
    );
    fixture.request_stop();
    drop(second);
    let events = fixture.finish();
    let batches: Vec<_> = events.into_iter().map(bytes).collect();
    assert_eq!(batches.len(), CAPACITY + 2);
    assert!(batches[..CAPACITY].iter().all(|batch| batch == &[FILL]));
    assert_eq!(&batches[CAPACITY..], &[b"A".to_vec(), b"\x07".to_vec()]);
}

#[derive(Clone, Copy, Debug)]
enum CapacityRace {
    BeforeArm,
    BetweenArmAndRetry,
    AfterRetryFull,
}

impl CapacityRace {
    fn checkpoint(self) -> Checkpoint {
        match self {
            Self::BeforeArm => Checkpoint::BeforeCapacityArm,
            Self::BetweenArmAndRetry => Checkpoint::AfterCapacityArm,
            Self::AfterRetryFull => Checkpoint::AfterCapacityRetryFull,
        }
    }
}

fn capacity_race(race: CapacityRace) {
    let checkpoint = race.checkpoint();
    let hooks = Hooks::new();
    let held = hooks.arm(checkpoint);
    let mut fixture = ReaderFixture::start(InitialQueue::Full, hooks);
    fixture.pty.send(b"A");
    let observed = fixture.hooks.wait_gate(checkpoint);
    assert!(observed.has_head);
    let credit_gate = match race {
        CapacityRace::BeforeArm | CapacityRace::BetweenArmAndRetry => held,
        CapacityRace::AfterRetryFull => {
            // Keep the bound full until the actual loop is about to enter
            // poll. Otherwise a subsequent inline retry could mask a lost wake.
            let poll = fixture.hooks.arm(Checkpoint::BeforePoll);
            drop(held);
            fixture.hooks.wait_gate(Checkpoint::BeforePoll);
            poll
        }
    };
    let published = fixture.hooks.arm(Checkpoint::AfterPublish);
    let before_credit = fixture.hooks.state.history.lock().unwrap().len();
    assert_eq!(bytes(fixture.recv()), vec![FILL]);
    drop(credit_gate);
    fixture.hooks.wait_gate(Checkpoint::AfterPublish);
    let progress: Vec<_> = fixture.hooks.state.history.lock().unwrap()[before_credit..]
        .iter()
        .map(|event| event.checkpoint)
        .collect();
    match race {
        CapacityRace::BeforeArm | CapacityRace::BetweenArmAndRetry => {
            assert!(
                !progress.contains(&Checkpoint::BeforePoll),
                "the inline retry must close this race without waiting"
            );
        }
        CapacityRace::AfterRetryFull => {
            assert!(
                progress.iter().any(|checkpoint| matches!(checkpoint,
                    Checkpoint::AfterCapacityWakeRead { len } if *len > 0
                )),
                "an actual wake-fd read must precede publication; timeout recovery alone is insufficient"
            );
        }
    }
    // Publication must complete without another PTY byte. The native wake-read
    // observation distinguishes a remembered wake from timeout recovery.
    let queued: Vec<_> = (0..CAPACITY).map(|_| bytes(fixture.recv())).collect();
    assert!(queued[..CAPACITY - 1].iter().all(|batch| batch == &[FILL]));
    assert_eq!(queued.last().unwrap(), b"A");
    drop(published);
    assert!(fixture.finish().is_empty());
}

#[test]
fn real_reader_capacity_freed_before_arm_is_not_lost() {
    capacity_race(CapacityRace::BeforeArm);
}

#[test]
fn real_reader_capacity_freed_between_arm_and_retry_is_not_lost() {
    capacity_race(CapacityRace::BetweenArmAndRetry);
}

#[test]
fn real_reader_capacity_freed_after_retry_full_is_not_lost() {
    capacity_race(CapacityRace::AfterRetryFull);
}

#[test]
fn real_reader_full_private_wake_keeps_pending_capacity_observable() {
    let hooks = Hooks::new();
    let full = hooks.arm(Checkpoint::AfterCapacityRetryFull);
    let mut fixture = ReaderFixture::start(InitialQueue::Full, hooks);
    fixture.pty.send(b"A");
    fixture.hooks.wait_gate(Checkpoint::AfterCapacityRetryFull);
    let poll = fixture.hooks.arm(Checkpoint::BeforePoll);
    drop(full);
    fixture.hooks.wait_gate(Checkpoint::BeforePoll);
    let written = fixture
        .capacity
        .as_ref()
        .unwrap()
        .fill_wake_for_test()
        .expect("fill actual private wake to WouldBlock");
    assert!(
        written > 0,
        "the private wake was filled with retained bytes"
    );
    let published = fixture.hooks.arm(Checkpoint::AfterPublish);
    let before_credit = fixture.hooks.state.history.lock().unwrap().len();
    // notify_capacity's real nonblocking write now encounters WouldBlock.
    // Existing readable bytes must retain the wake; an error is not waived.
    assert_eq!(bytes(fixture.recv()), vec![FILL]);
    drop(poll);
    fixture.hooks.wait_gate(Checkpoint::AfterPublish);
    let history = fixture.hooks.state.history.lock().unwrap();
    assert!(
        history[before_credit..].iter().any(|event| matches!(
            event.checkpoint, Checkpoint::AfterCapacityWakeRead { len } if len > 0
        )),
        "the real worker must consume its full wake stream before publishing A"
    );
    drop(history);
    let queued: Vec<_> = (0..CAPACITY).map(|_| bytes(fixture.recv())).collect();
    assert!(queued[..CAPACITY - 1].iter().all(|batch| batch == &[FILL]));
    assert_eq!(queued.last().unwrap(), b"A");
    drop(published);
    assert!(fixture.finish().is_empty());
}

#[test]
fn real_reader_preserves_bytes_split_across_actual_host_reads() {
    let raw = neovm_core::emacs_core::emacs_char::EmacsChar::from_byte8(0xa4);
    let mut internal = [0; neovm_core::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH];
    let count = raw.char_string(&mut internal);
    let cases = [
        "中".as_bytes(),
        "😀".as_bytes(),
        &[0xa4, 0x40],
        &internal[..count],
    ];
    for encoded in cases {
        let mut fixture = ReaderFixture::start(InitialQueue::Empty, Hooks::new());
        fixture.pty.send(&encoded[..1]);
        assert_eq!(bytes(fixture.recv()), encoded[..1]);
        // Receiving the first real publication proves the read boundary; two
        // unsynchronized writes would permit the kernel to coalesce the input.
        fixture.pty.send(&encoded[1..]);
        let mut suffix = Vec::new();
        while suffix.len() < encoded.len() - 1 {
            suffix.extend(bytes(fixture.recv()));
        }
        assert_eq!(suffix, encoded[1..]);
        assert!(fixture.finish().is_empty());
    }
}

#[test]
fn real_reader_native_size_query_orders_pending_a_resize_b() {
    let hooks = Hooks::new();
    let a = hooks.arm(Checkpoint::AfterPublish);
    let mut fixture = ReaderFixture::start(InitialQueue::Empty, hooks);
    fixture.pty.send(b"A");
    fixture.hooks.wait_gate(Checkpoint::AfterPublish);
    let query = fixture.hooks.arm(Checkpoint::BeforeSizeQuery);
    drop(a);
    fixture.hooks.wait_gate(Checkpoint::BeforeSizeQuery);
    fixture.pty.resize(100, 31);
    fixture.pty.send(b"B");
    drop(query);
    assert_eq!(bytes(fixture.recv()), b"A");
    assert!(matches!(fixture.recv(), InputEvent::WindowResize {
        width: 100, height: 31, scale_factor, emacs_frame_id: 0,
    } if scale_factor == 1.0));
    assert_eq!(bytes(fixture.recv()), b"B");
    assert!(fixture.finish().is_empty());
}

#[test]
fn real_reader_shutdown_waits_for_held_head_then_drops_later_backlog() {
    let hooks = Hooks::new();
    let first = hooks.arm(Checkpoint::AfterRawRead { len: 1 });
    let mut fixture = ReaderFixture::start(InitialQueue::Full, hooks);
    fixture.pty.send(b"A");
    fixture.hooks.wait_gate(Checkpoint::AfterRawRead { len: 1 });
    let later = fixture.hooks.arm(Checkpoint::AfterRawRead { len: 1 });
    drop(first);
    fixture.pty.send(b"B");
    fixture.hooks.wait_gate(Checkpoint::AfterRawRead { len: 1 });
    fixture
        .commands
        .send(RenderCommand::Lifecycle(LifecycleCommand::Shutdown))
        .unwrap();
    let published = fixture.hooks.arm(Checkpoint::AfterPublish);
    assert_eq!(bytes(fixture.recv()), vec![FILL]);
    drop(later);
    fixture.hooks.wait_gate(Checkpoint::AfterPublish);
    assert!(
        !fixture.hooks.saw(Checkpoint::ShutdownObserved),
        "held A must publish before Shutdown is observed"
    );
    let shutdown = fixture.hooks.arm(Checkpoint::ShutdownObserved);
    drop(published);
    fixture.hooks.wait_gate(Checkpoint::ShutdownObserved);
    drop(shutdown);
    let queued: Vec<_> = fixture.finish().into_iter().map(bytes).collect();
    assert_eq!(queued.len(), CAPACITY);
    assert!(queued[..CAPACITY - 1].iter().all(|batch| batch == &[FILL]));
    assert_eq!(queued.last().unwrap(), b"A");
}

#[derive(Debug)]
struct OwnedWriter {
    child: Child,
    control: ChildStdin,
}

impl OwnedWriter {
    fn spawn(pty: &NativePty) -> Self {
        let mut child = Command::new("/bin/sh")
            .args([
                "-c",
                "printf A; IFS= read -r phase; printf B; IFS= read -r phase; printf C",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::from(
                pty.master.try_clone().expect("writer master lease"),
            ))
            .stderr(Stdio::null())
            .spawn()
            .expect("owned PTY writer");
        let control = child.stdin.take().unwrap();
        Self { child, control }
    }

    fn release(&mut self) {
        self.control
            .write_all(b"next\n")
            .expect("release own writer phase");
    }

    fn finish(&mut self) {
        assert!(self.child.wait().expect("reap own writer").success());
    }

    fn cancel_and_finish(&mut self) {
        // Explicit exceptional cleanup, outside Drop. Never signal a pid
        // discovered outside this Child ownership; reap before resuming unwind.
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            self.child
                .wait()
                .expect("reap own writer during fixture failure");
        }
    }
}

#[test]
fn real_reader_disconnect_drops_delivery_but_continues_owned_child_acquisition() {
    let mut fixture = ReaderFixture::start(InitialQueue::Empty, Hooks::new());
    let mut writer = OwnedWriter::spawn(&fixture.pty);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        assert_eq!(bytes(fixture.recv()), b"A");
        let disconnected = fixture.hooks.arm(Checkpoint::InputDisconnected);
        fixture.input.take();
        writer.release();
        fixture.hooks.wait_gate(Checkpoint::InputDisconnected);
        let next_read = fixture.hooks.arm(Checkpoint::AfterRawRead { len: 1 });
        drop(disconnected);
        writer.release();
        fixture.hooks.wait_gate(Checkpoint::AfterRawRead { len: 1 });
        fixture.request_stop();
        drop(next_read);
        assert!(fixture.finish().is_empty());
    }));
    match result {
        Ok(()) => writer.finish(),
        Err(payload) => {
            writer.cancel_and_finish();
            std::panic::resume_unwind(payload);
        }
    }
}
