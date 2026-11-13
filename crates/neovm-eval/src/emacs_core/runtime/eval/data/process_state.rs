//! Process state vocabulary owned by the evaluator layer (stage-4 W2/E6c A3).
//!
//! Peer ruling 16:2xZ: the ProcessManager state unit moves into eval data.
//! This module holds the type definitions (fields widened to plain pub so
//! the system-side behaviour impls can keep operating on them in place); the
//! I/O methods live in system as SystemProcessExt, the Context-bound process
//! service methods as ProcessContextExt.  Sibling state types that are still
//! in the process module are imported until their own move lands; the
//! delivery of A3 re-points them here.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::io;
use std::io::Read;
use std::net::{SocketAddr, TcpListener, TcpStream, UdpSocket};
#[cfg(unix)]
use std::os::fd::{AsRawFd, OwnedFd};
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
#[cfg(unix)]
use std::os::unix::net::{UnixDatagram, UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use neovm_value::heap_types::{LispString, ProcessId};
use neovm_value::value::buffer_vocab::BufferId;
use neovm_value::value::{Value, ValueKind, equal_value, list_to_vec};
use num_enum::IntoPrimitive;
use polling::Poller;
use socket2::Socket;
use strum::{EnumString, IntoStaticStr};

use crate::emacs_core::eval::data::process_spawn::SpawnedChild;
use crate::emacs_core::eval::data::process_tls::TlsStream;
use crate::host_effect::wake::{
    ControlDestination, ControlNudge, ControlNudgeOutcome, NotificationLatch,
};
use neovm_edit::buffer::BufferManager;
use neovm_value::error::{EvalResult, Flow, LispCondition, signal};
use neovm_value::gc_trace::GcTrace;
use neovm_value::intern::resolve_sym;

/// Whether a live process participates in the exit confirmation query.
///
/// GNU stores this as the inverse `kill_without_query` bit.  Naming the two
/// policies directly keeps that inversion out of process creation and, more
/// importantly, makes the accepted-client default explicit: `make_process`
/// creates a querying client even when its listening server is `:noquery t`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitQueryPolicy {
    Query,
    NoQuery,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessReadConfig {
    pub readmax: usize,
    pub adaptive_read_buffering: u8,
}

/// Process family used by compatibility helpers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
pub enum ProcessKind {
    Real,
    Network,
    Pipe,
    Serial,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
#[strum(prefix = ":", serialize_all = "kebab-case")]
pub enum ProcessKeyword {
    Name,
    Type,
    Buffer,
    Command,
    Coding,
    Noquery,
    Stop,
    ConnectionType,
    Filter,
    Sentinel,
    Stderr,
    FileHandler,
    Host,
    Service,
    Family,
    Local,
    Remote,
    Server,
    Nowait,
    Log,
    TlsParameters,
    UseExternalSocket,
    Plist,
    Bindtodevice,
    Broadcast,
    Dontroute,
    Keepalive,
    Linger,
    Oobinline,
    Priority,
    Reuseaddr,
    Nodelay,
    Port,
    Speed,
    Process,
    Bytesize,
    Stopbits,
    Parity,
    Flowcontrol,
    Summary,
}

#[derive(Default, PartialEq, Eq)]
pub enum TlsPolling {
    #[default]
    Enabled,
    Suspended,
}

/// Operating-system resources owned by a live process connection.
///
/// GNU keeps Lisp process identity/status alive after `remove_process`, but
/// `deactivate_process` closes every descriptor immediately.  Keeping all
/// native handles in one Rust owner gives Neomacs the same lifetime split:
/// `Process` is durable Lisp-visible state, while replacing this bundle with
/// `Default::default()` drops every live handle as one operation.
#[derive(Default)]
pub struct LiveProcessIo {
    /// Pollable child-status wakeup source, where the platform exposes one.
    pub child_status_source: Option<ChildStatusSource>,
    /// The OS child this process owns, and GNU's `p->alive` for it.
    ///
    /// One slot for both spawn shapes -- the `std::process::Child` a pipe (or
    /// a `pre_exec` pty) leaves behind and the `portable_pty` handle -- because
    /// they are the same obligation: each owns a `waitpid`, and GNU allows
    /// exactly one owner (src/process.c:1080-1088).  See `process/reap.rs`.
    pub child: ChildOwnership,
    /// OS-level output pipe for non-blocking reads (pipe mode).  With no
    /// explicit `:stderr`, this is one shared pipe carrying both stdout and
    /// stderr in the child's write order, as in GNU Emacs.
    pub child_stdout: Option<ChildOutputReader>,
    /// Writable endpoint owned by a `make-pipe-process` connection.
    pub module_pipe_writer: Option<os_pipe::PipeWriter>,
    /// Real process that received this pipe's writer as its `:stderr`.
    ///
    /// GNU obtains this ordering relation from process-alist insertion order.
    /// Keeping the actual transfer owner here avoids confusing it with later
    /// processes that merely retain the same Lisp `:stderr` reference after
    /// the one writable endpoint has already been consumed.
    pub stderr_pipe_owner: Option<ProcessId>,
    /// PTY master handle for resize and I/O (PTY mode).
    pub pty_master: Option<Box<dyn portable_pty::MasterPty + Send>>,
    /// PTY reader for non-blocking reads from the master side.
    pub pty_reader: Option<PtyOutputReader>,
    /// PTY writer for sending input to the master side.
    pub pty_writer: Option<Box<dyn std::io::Write + Send>>,
    pub network_socket: Option<NetworkSocket>,
    /// Filesystem identity captured when binding a Unix listener. Exit cleanup
    /// must not unlink a different endpoint installed by an exit hook.
    #[cfg(unix)]
    pub unix_listener_path: Option<(PathBuf, u64, u64)>,
    pub pending_network_connect: Option<PendingNetworkConnect>,
    /// TLS-wrapped stream for encrypted network connections.
    pub tls_stream: Option<TlsStream>,
    pub tls_polling: TlsPolling,
    /// The open device behind a serial process.  GNU keeps one descriptor for
    /// both directions (`p->infd = fd; p->outfd = fd`, src/process.c:3216-3217),
    /// so this single slot is the read source AND the write source.
    ///
    /// It is `Option` only because `LiveProcessIo` is one struct for every
    /// process kind and `deactivate_process_io` empties it; a serial process
    /// cannot be BORN without one -- see [`ProcessManager::create_serial_process`].
    pub serial_port: Option<SerialPort>,
}

pub enum ChildOutputReader {
    Stdout(std::fs::File),
    Shared(os_pipe::PipeReader),
    #[cfg(all(any(test, feature = "test-support"), unix))]
    Injected(pipe_exit_test_support::PipeReader),
}

/// Test-only injected reader used by the re-homed system pipe tests.
///
/// The evaluator owns [`ChildOutputReader`], so the injection type must live
/// here: upper test crates construct it, and a lower crate cannot name an
/// upper crate's test module.  Compiled for the evaluator's own tests and for
/// upper test builds under `test-support`.
#[cfg(all(any(test, feature = "test-support"), unix))]
pub mod pipe_exit_test_support {
    use super::ChildOutputReader;
    use std::collections::VecDeque;
    use std::io::{self, Read};
    use std::os::fd::{AsRawFd, RawFd};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    /// Test-only read seam around a real owned pipe. The fixture's sole
    /// mutator injects interruptions; no global hooks, sleeps, signals or
    /// borrowed PIDs.
    pub struct PipeReader {
        reader: Box<ChildOutputReader>,
        steps: VecDeque<ReadStep>,
        calls: Arc<AtomicUsize>,
    }

    impl PipeReader {
        pub fn new(
            reader: Box<ChildOutputReader>,
            steps: VecDeque<ReadStep>,
            calls: Arc<AtomicUsize>,
        ) -> Self {
            Self {
                reader,
                steps,
                calls,
            }
        }
    }

    impl std::fmt::Debug for PipeReader {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("PipeReader")
                .field("fd", &self.reader.as_raw_fd())
                .field("steps", &self.steps)
                .field("calls", &self.calls)
                .finish()
        }
    }

    #[derive(Debug)]
    pub enum ReadStep {
        Prefix(usize),
        Interrupted,
    }

    impl Read for PipeReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            match self.steps.pop_front() {
                Some(ReadStep::Interrupted) => Err(io::ErrorKind::Interrupted.into()),
                Some(ReadStep::Prefix(limit)) => {
                    let len = buffer.len().min(limit);
                    self.reader.read(&mut buffer[..len])
                }
                None => self.reader.read(buffer),
            }
        }
    }

    impl AsRawFd for PipeReader {
        fn as_raw_fd(&self) -> RawFd {
            self.reader.as_raw_fd()
        }
    }
}

/// A tracked process record.
pub struct Process {
    pub id: ProcessId,
    pub name: Value,
    pub command: Value,
    pub executable: Option<LispString>,
    pub kind: ProcessKind,
    pub proc_type: Value,
    pub status: Value,
    /// A child-status transition has been recorded, but GNU-style status
    /// notification (sentinel/default buffer message and optional reaping)
    /// still needs to run.
    pub status_notify_pending: bool,
    /// GNU's `p->tick` / `p->update_tick` (src/process.h:144-147), which is
    /// `status_notify`'s membership test (:7892) and NOT the same bit as
    /// `status_notify_pending` (GNU's `raw_status_new`).
    pub status_ticks: StatusChangeTicks,
    /// Kernel child-status transition delivered by the wait backend but not
    /// yet published to the process sentinel.  This includes stop/continue as
    /// well as exit/signal, mirroring GNU's `raw_status_new`.
    pub pending_status: Value,
    pub buffer: Value,
    pub childp: Value,
    /// Queued input entries `(STRING . (OFFSET . LENGTH))`, matching GNU's `write_queue`.
    pub write_queue: Value,
    /// Maximum bytes read by one `read_process_output` pass.
    pub readmax: usize,
    /// GNU's tri-state adaptive read buffering flag: 0=nil, 1=t, 2=other non-nil.
    pub adaptive_read_buffering: u8,
    /// Current adaptive read delay.
    pub read_output_delay: Duration,
    /// Whether the next non-targeted service pass should skip this process once.
    pub read_output_skip: bool,
    /// Whether this process participates in the exit confirmation query.
    pub exit_query_policy: ExitQueryPolicy,
    /// Process filter callback (or default marker symbol).
    pub filter: Value,
    /// Process sentinel callback (or default marker symbol).
    pub sentinel: Value,
    /// Server process log callback.
    pub log: Value,
    /// Process plist state.
    pub plist: Value,
    /// Pipe process attached to standard error.
    pub stderrproc: Value,
    /// Current decoding coding-system.
    pub coding_decode: Value,
    /// GNU's per-process `struct coding_system`, reduced to the fields that
    /// outlive a single read: the decoder's carryover and its
    /// `CODING_MODE_LAST_BLOCK` latch.  See [`ProcessCodingState`].
    pub coding_state: ProcessCodingState,
    /// Current encoding coding-system.
    pub coding_encode: Value,
    /// The coding and encoder continuation currently installed on the output fd.
    pub encoding_state: ProcessEncodingState,
    /// True once Lisp explicitly changes this process's coding system.
    pub coding_explicitly_set: bool,
    /// True after explicit process coding has deferred one terminal status
    /// notification so Lisp can observe decoded output before the sentinel.
    pub explicit_coding_status_deferred_once: bool,
    /// Inherit-coding-system flag.
    pub inherit_coding_system_flag: bool,
    /// Attached thread object.
    pub thread: Value,
    /// Last process-window-size columns value.
    pub window_cols: Option<i64>,
    /// Last process-window-size rows value.
    pub window_rows: Option<i64>,
    /// Terminal name reported by `process-tty-name`, when this process uses a tty.
    pub tty_name: Value,
    /// Whether stdin is tty-backed for this process.
    pub tty_stdin: bool,
    /// Whether stdout is tty-backed for this process.
    pub tty_stdout: bool,
    /// Whether stderr is tty-backed for this process.
    pub tty_stderr: bool,
    /// The child's real OS process id, captured at spawn time.  GNU's
    /// `Fprocess_id` returns this pid (`XPROCESS (process)->pid`); it is `None`
    /// for network/serial/pipe connections that have no OS child, and stays
    /// independent of the internal `ProcessId` used to key the manager.
    pub os_pid: Option<u32>,
    /// GNU's three write-side states, kept separate from bidirectional native
    /// resource ownership. See [`ProcessInputDisposition`].
    pub input_disposition: ProcessInputDisposition,
    /// True after Lisp explicitly called `process-send-eof' on this process.
    /// GNU can publish the ensuing pipe status in the same wait that reads
    /// output from that explicit EOF, while naturally exiting pipe children can
    /// remain `run' until a later wait.  PTY subprocesses have their own
    /// same-wait status rule.
    pub eof_sent_to_process: bool,
    /// Native resources exist only while this process has active I/O.
    pub live_io: LiveProcessIo,
    /// Current peer address for datagram network processes, as Lisp.
    pub datagram_address: Value,
    /// Current peer address for datagram network processes, as a Rust socket address.
    pub datagram_socket_addr: Option<SocketAddr>,
    /// Current peer address for Unix datagram network processes.
    #[cfg(unix)]
    pub datagram_unix_path: Option<PathBuf>,
    /// GNU-compatible GnuTLS initialization stage for this process.
    pub gnutls_initstage: GnutlsInitStage,
    /// Deferred parameters set by `gnutls-asynchronous-parameters`.
    pub gnutls_boot_parameters: Value,
    /// End-of-output marker, matching GNU's `p->mark`.
    pub mark: Value,
    /// Working directory for the subprocess, derived from
    /// `default-directory` at the time the process was created.
    /// If `None`, the child inherits the Rust process's cwd.
    pub default_directory: Option<PathBuf>,
}

/// Manages the set of live processes.
///
/// Uses `polling::Poller` for efficient I/O multiplexing (epoll on Linux,
/// kqueue on macOS, wepoll on Windows) instead of sleep-based polling.
pub struct ProcessManager {
    pub processes: HashMap<ProcessId, Process>,
    pub deleted_processes: HashMap<ProcessId, Process>,
    // WorldSlot reserves ProcessManager's original size in Context. Keep the
    // new read cursor behind the word formerly occupied by next_id so adding
    // scheduling state does not shift Context's pinned offsets/JIT ABI.
    pub ids: Box<ProcessIdState>,
    /// GNU's file-scope `process_tick` (src/process.c:232-233), *"Number of
    /// events of change of status of a process"*.  GNU's `update_tick`
    /// counterpart (:234-235) has no field here on purpose: it is only ever
    /// read at two lines (:5524, :5845), both of them a performance
    /// short-circuit deciding whether to bother calling `status_notify`, and
    /// this port's walk is unconditional.  The invariant lives in the
    /// per-process [`StatusChangeTicks`].
    pub process_tick: u64,
    pub default_read_config: ProcessReadConfig,
    /// Environment variable overrides (for `setenv`/`getenv`).
    pub env_overrides: HashMap<LispString, Option<LispString>>,
    pub wait_backend: ProcessWaitBackend,
}

/// Process allocation and round-robin discovery share one manager lifetime.
pub struct ProcessIdState {
    pub next_id: ProcessId,
    /// Resume live reads after this source, like GNU's last_read_channel.
    pub last_read_process: ProcessId,
}

/// Opaque, thread-safe handle a cross-thread producer uses to wake the blocked
/// evaluator after publishing work, via cross-platform `Poller::notify()`.
///
/// This is the platform-agnostic replacement for the Unix-only wakeup pipe: it
/// works identically on Linux/macOS/Windows (the `polling` crate maps `notify`
/// onto eventfd/pipe/IOCP as appropriate) and is one-shot + remembered if no
/// waiter is currently blocked, so `send`-then-`notify` never loses a wakeup.
#[derive(Clone)]
pub struct WaitNotifier {
    pub poller: Arc<polling::Poller>,
    pub notification_pending: Arc<NotificationLatch>,
}

/// Test-only native-connect observation types (moved down from the system
/// crate: eval cannot name system, and the wait backend owns the slot).
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug)]
pub struct NativeConnectWritable {
    pub process: ProcessId,
    pub owner: std::thread::ThreadId,
}

#[cfg(any(test, feature = "test-support"))]
impl NativeConnectWritable {
    pub fn process(&self) -> ProcessId {
        self.process
    }
    pub fn owner(&self) -> std::thread::ThreadId {
        self.owner
    }
}

#[cfg(any(test, feature = "test-support"))]
pub struct NativeConnectObserver {
    pub process: ProcessId,
    pub owner: std::thread::ThreadId,
    pub callback: Box<dyn FnOnce(NativeConnectWritable) + Send>,
}

#[cfg(any(test, feature = "test-support"))]
impl std::fmt::Debug for NativeConnectObserver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeConnectObserver")
            .field("process", &self.process)
            .field("owner", &self.owner)
            .finish_non_exhaustive()
    }
}

#[cfg(any(test, feature = "test-support"))]
static_assertions::assert_impl_all!(NativeConnectObserver: Send, std::fmt::Debug);
#[cfg(any(test, feature = "test-support"))]
static_assertions::assert_not_impl_any!(NativeConnectObserver: Clone, Copy);
#[cfg(any(test, feature = "test-support"))]
static_assertions::assert_impl_all!(NativeConnectWritable: Send, std::fmt::Debug);
#[cfg(any(test, feature = "test-support"))]
static_assertions::assert_not_impl_any!(NativeConnectWritable: Clone, Copy);

/// Test-only storage for a consumed native-connect observation.
#[cfg(any(test, feature = "test-support"))]
#[derive(Default, Debug)]
#[repr(transparent)]
pub struct NativeConnectObservationSlot(pub std::sync::Mutex<Option<NativeConnectObserver>>);

#[cfg(any(test, feature = "test-support"))]
static_assertions::assert_impl_all!(NativeConnectObservationSlot: Send, Sync, std::fmt::Debug);
#[cfg(any(test, feature = "test-support"))]
static_assertions::assert_not_impl_any!(NativeConnectObservationSlot: Clone, Copy);
#[cfg(all(
    any(test, feature = "test-support"),
    target_arch = "x86_64",
    target_os = "linux"
))]
static_assertions::const_assert_eq!(std::mem::size_of::<NativeConnectObservationSlot>(), 40);

/// Refusal to observe this fixture's actual pending TCP owner state.
///
/// Moved down from the system process vocabulary (peer 02:5xZ pattern: the
/// pending-connect observation types live with the evaluator state they
/// describe; the system copies had no users).
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, thiserror::Error)]
pub(crate) enum PendingConnectObservationError {
    #[error("pending connect process missing")]
    MissingProcess,
    #[error("Main reached Select after connect was already decoded")]
    AlreadyDecoded,
    #[error("expected pending TCP connect")]
    NotPendingTcp,
    #[error("pending connect socket missing")]
    MissingSocket,
    #[error("expected TCP stream")]
    NotTcpStream,
    #[error("pending connect poller missing")]
    MissingBackend,
}
#[cfg(any(test, feature = "test-support"))]
static_assertions::assert_impl_all!(PendingConnectObservationError: std::error::Error, Send, Sync);

/// Native-only proof for the one pending TCP connect in a Green test.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug)]
pub(crate) struct MainConnectPending {
    process: ProcessId,
}
#[cfg(any(test, feature = "test-support"))]
impl MainConnectPending {
    pub(crate) fn process(&self) -> ProcessId {
        self.process
    }
}
#[cfg(any(test, feature = "test-support"))]
static_assertions::assert_impl_all!(MainConnectPending: Send, std::fmt::Debug);
#[cfg(any(test, feature = "test-support"))]
static_assertions::assert_not_impl_any!(MainConnectPending: Clone, Copy);

pub struct ProcessWaitBackend {
    /// A per-backend, one-shot scheduling gate for input-wakeup regressions.
    /// Installation and consumption are synchronized; callbacks own no Lisp state.
    #[cfg(any(test, feature = "test-support"))]
    pub before_next_wait: std::sync::Mutex<Option<Box<dyn FnOnce(BackendWaitEntry) + Send>>>,
    #[cfg(any(test, feature = "test-support"))]
    pub native_connect_observer: NativeConnectObservationSlot,
    #[cfg(unix)]
    pub signal_fd: Option<std::os::fd::RawFd>,
    /// I/O multiplexer for process descriptors and cross-thread notifications.
    ///
    /// Shared (`Arc`) so any cross-thread producer can wake a blocked
    /// `poller.wait()` via the cross-platform `Poller::notify()` — the basis for
    /// the unified single-poll wait loop (no per-OS wakeup pipe needed).
    pub poller: Option<Arc<polling::Poller>>,
    /// Semantic cause paired with the poller's eventless notify wakeup.
    ///
    /// A bool is sufficient because notifications are coalescing readiness,
    /// not a count: one wake tells the evaluator to service published work.
    pub notification_pending: Arc<NotificationLatch>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessWaitBackendInterest {
    ProcessesOnly,
    NotificationsOnly,
    NotificationsAndProcesses,
}

/// A native pre-Poller observation, after pending signal/notification checks.
/// This reports entry to the wait boundary, not proof of a kernel-blocked task.
#[cfg(any(test, feature = "test-support"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BackendWaitEntry {
    pub requested_timeout: Duration,
    pub interest: ProcessWaitBackendInterest,
}

/// GNU `coding->mode & CODING_MODE_LAST_BLOCK` for a process's decoder.
///
/// A latch and not a boolean argument, because `read_process_output` READS and
/// RAISES it in the same three lines and behaves differently on each side of
/// the transition (src/process.c:6315-6321):
///
/// ```c
///   if (nbytes <= 0)
///     {
///       if (nbytes < 0 || coding->mode & CODING_MODE_LAST_BLOCK)
///         { SAFE_FREE_UNBIND_TO (count, Qnil); return nbytes; }
///       coding->mode |= CODING_MODE_LAST_BLOCK;
///     }
/// ```
///
/// It is never lowered, and it lives on the process rather than on the read
/// because GNU's `coding` here IS the process's own `struct coding_system`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LastBlock {
    /// No read has returned zero bytes yet.
    #[default]
    NotReached,
    /// A read returned zero bytes and raised the flag.  Every later zero-byte
    /// read returns immediately.
    Reached,
}

/// GNU keeps ONE `struct coding_system` per process for the process's whole
/// life -- `proc_decode_coding_system[channel]`, set up by
/// `setup_process_coding_systems` (src/process.c:8395-8407) and read back by
/// every `read_process_output` (:6238) -- and some of its fields are facts
/// about the PROCESS rather than about a single read:
///
/// * `coding->carryover` / `carryover_bytes`, the trailing bytes the decoder
///   could not consume.  Written AFTER the decode (:6448-6457) and prepended
///   to the next read (:6252-6254).
/// * `coding->mode & CODING_MODE_LAST_BLOCK`, raised exactly once by the first
///   read that returns nothing (:6313-6321) and never lowered.
///
/// This port had the first as a bare `Vec<u8>` on the process and the second
/// nowhere at all: every call site worked out a `flush` boolean for itself, and
/// "flush" happened to mean "there is carryover left" -- which is why the EOF
/// read of a process with no carryover decoded nothing where GNU decodes zero
/// bytes and runs the coding system's `:post-read-conversion` for it.  One
/// struct is what GNU has, and it is what makes the two impossible to update
/// independently.
#[derive(Clone, Debug, Default)]
pub struct ProcessCodingState {
    pub carryover: Vec<u8>,
    pub last_block: LastBlock,
    pub decoder: neovm_text::encoding::CodingDecoderState,
}

/// Whether GNU's installed output descriptor invokes an encoding engine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessEncodingEligibility {
    Required,
    Bypass,
}

/// A descriptor groups its coding identity, setup flags, and continuation.
/// Fields stay private so changing identity cannot retain another codec's state.
#[derive(Clone, Debug)]
pub struct ProcessEncodingDescriptor {
    pub coding: neovm_text::encoding::RuntimeCodingSystem,
    pub raw_coding: neovm_text::encoding::RuntimeCodingSystem,
    pub eligibility: ProcessEncodingEligibility,
    pub encoder: neovm_text::encoding::CodingEncoderState,
}

#[derive(Clone, Debug, Default)]
pub enum ProcessEncodingState {
    /// Manager-only records have no Context with which to perform GNU setup.
    #[default]
    Uninitialized,
    Active(ProcessEncodingDescriptor),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkSocketOption {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    Bindtodevice,
    Broadcast,
    Dontroute,
    Keepalive,
    Linger,
    Oobinline,
    #[cfg(any(target_os = "linux", target_os = "android"))]
    Priority,
    Reuseaddr,
    Nodelay,
}

#[derive(Clone, Copy, Debug)]
pub struct NetworkSocketOptionSpec {
    pub keyword: ProcessKeyword,
    pub option: NetworkSocketOption,
    pub value: Value,
}

#[derive(Debug)]
pub enum PendingNetworkConnect {
    Tcp {
        remaining_addrs: Vec<SocketAddr>,
        socket_options: Vec<NetworkSocketOptionSpec>,
    },
    Dns(PendingDnsRequest),
    #[cfg(unix)]
    Local,
}

#[derive(Debug)]
pub struct PendingDnsRequest {
    pub host: String,
    pub receiver: mpsc::Receiver<Result<Vec<SocketAddr>, String>>,
    pub ready: Arc<AtomicBool>,
    pub socket_options: Vec<NetworkSocketOptionSpec>,
}

// Sibling process-state definitions (A3 part 3b).

/// GNU-compatible GnuTLS process initialization stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, IntoPrimitive)]
#[repr(i64)]
pub enum GnutlsInitStage {
    Empty = 0,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    CredAlloc = 1,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    Files = 2,
    Callbacks = 3,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    Init = 4,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    Priority = 5,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    CredSet = 6,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    TransportPointersSet = 7,
    HandshakeTried = 8,
    Ready = 9,
}

/// What a Lisp send should do with bytes after the process's write side has
/// changed state.
///
/// GNU represents this with `p->outfd`: a live descriptor accepts bytes,
/// `process-send-eof` replaces it with `/dev/null`, and EPIPE sets it to -1.
/// Neomacs keeps bidirectional sockets, TLS streams, and serial ports in one
/// Rust owner, so dropping the owner to express a closed write side would also
/// discard still-readable output. Naming the three states preserves GNU's
/// half-connection semantics independently of resource ownership.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProcessInputDisposition {
    #[default]
    Connected,
    Discard,
    Disconnected,
}

/// OS socket owned by a network process.
///
/// GNU Emacs keeps the concrete socket type in the process record
/// (`socktype`, `is_server`, and fd slots).  Keep the Rust side explicit as
/// well, so listener-only operations and stream I/O cannot be confused.
#[derive(Debug)]
pub enum NetworkSocket {
    TcpStream(TcpStream),
    TcpListener(TcpListener),
    UdpSocket(UdpSocket),
    #[cfg(unix)]
    SeqpacketStream(Socket),
    #[cfg(unix)]
    SeqpacketListener(Socket),
    #[cfg(unix)]
    UnixStream(UnixStream),
    #[cfg(unix)]
    UnixListener(UnixListener),
    #[cfg(unix)]
    UnixDatagram(UnixDatagram),
}

impl NetworkSocket {
    /// GNU's process `:type` name for this socket.
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::TcpStream(_) => "tcp-stream",
            Self::TcpListener(_) => "tcp-listener",
            Self::UdpSocket(_) => "udp-socket",
            #[cfg(unix)]
            Self::SeqpacketStream(_) => "seqpacket-stream",
            #[cfg(unix)]
            Self::SeqpacketListener(_) => "seqpacket-listener",
            #[cfg(unix)]
            Self::UnixStream(_) => "unix-stream",
            #[cfg(unix)]
            Self::UnixListener(_) => "unix-listener",
            #[cfg(unix)]
            Self::UnixDatagram(_) => "unix-datagram",
        }
    }
}

/// GNU's `p->alive`, with the pid attached to the arm that may use it.
///
/// [`Default`] is [`Self::NotAChild`], which is what ledger 165's finding
/// needs: a pipe, network or serial connection has no pid at all, so
/// `get_child_status`'s `eassert (child > 0)` (src/sysdep.c:462) is a state
/// this enum cannot enter rather than a check inside a loop.
#[derive(Default)]
pub enum ChildOwnership {
    /// No child was ever spawned: `make-pipe-process`,
    /// `make-network-process`, `make-serial-process`, and the harness-only
    /// records with no OS backing.
    #[default]
    NotAChild,
    /// GNU's `p->alive == 1`.
    Unreaped(ReapableChild),
    /// GNU's `p->alive == 0`: the pid has gone back to the OS.
    ///
    /// It carries nothing, and that is the point -- there is no pid here for
    /// `kill(2)` or `waitpid(2)` to be given.  The terminal STATUS is not kept
    /// here either, because `Process::pending_status`/`Process::status` is
    /// where this port records it, exactly as GNU records it in
    /// `p->raw_status` rather than in `p->alive`.
    Reaped,
    /// The sole unreaped handle belongs to the deferred registry. This process
    /// cannot signal or wait for it again; native completion performs the reap.
    Deferred,
}

impl ChildOwnership {
    /// Whether a child was ever spawned -- live or already reaped.
    ///
    /// This is *not* `p->alive`: a reaped child is still a child, and the
    /// `Real`-process predicates that ask "does this record have OS backing?"
    /// mean this question rather than the liveness one.
    pub fn has_child(&self) -> bool {
        !matches!(self, Self::NotAChild)
    }
}

/// Which kind of child handle a spawn left behind.
///
/// Both own a reap of their own -- `std::process::Child::try_wait` and
/// `portable_pty::Child::try_wait` are each a `waitpid` -- which is exactly
/// why neither may be reachable from anywhere else.
pub enum ChildHandle {
    /// `std::process::Child`: the pipe spawn, and the pty spawn whose
    /// `:stderr` split goes through `pre_exec`.
    Pipe(Box<SpawnedChild>),
    /// `portable_pty`'s child handle.
    Pty(Box<dyn portable_pty::Child + Send + Sync>),
}

/// A child this port spawned and has **not** reaped: GNU's `p->alive == 1`,
/// carrying the pid that authorises `waitpid` and `kill` on it.
///
/// The fields are private and the only constructors are
/// [`ChildOwnership::of_pipe_child`] and [`ChildOwnership::of_pty_child`], so
/// the handles whose `try_wait` is a reap are not reachable from outside this
/// module.
pub struct ReapableChild {
    /// GNU's `p->pid` as a CAPABILITY.
    ///
    /// `Process::os_pid` keeps the same number for `Fprocess_id`, which GNU
    /// also keeps after the reap; this copy is the one that authorises a
    /// syscall, and it disappears with the reap.
    pub pid: u32,
    pub handle: ChildHandle,
}

#[derive(Clone, Copy, Debug)]
pub enum PtyReadInterest {
    Monitored,
    DrainOnly,
}

/// Owned by one process manager, with the same Send contract as its reader.
/// No shared or thread-local state is involved: the owning mutator alone
/// changes interest, and both states retain the descriptor for status_notify.
pub struct PtyOutputReader {
    pub reader: Box<dyn Read + Send>,
    pub interest: PtyReadInterest,
}

/// GNU's `p->tick` / `p->update_tick` pair (src/process.h:144-147):
///
/// ```c
///     /* Event-count of last event in which this process changed status.  */
///     EMACS_INT tick;
///     /* Event-count of last such event reported.  */
///     EMACS_INT update_tick;
/// ```
///
/// It is one value rather than two fields because the only assignment GNU ever
/// makes to `update_tick` is `p->update_tick = p->tick;` -- at :7894 and again
/// at :7935 -- so [`Self::notified`] takes no argument and *"notified a tick
/// this process never reached"* is not a state that can be written.
///
/// This is deliberately NOT the same bit as `status_notify_pending`, which is
/// GNU's `raw_status_new`: GNU keeps them apart and sets them independently
/// (`Fdelete_process` at :1123-1128 and `process_send_signal` at :7176-7178
/// both CLEAR `raw_status_new` and MOVE the tick in the same breath).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StatusChangeTicks {
    /// GNU `p->tick`.
    pub changed: u64,
    /// GNU `p->update_tick`.
    pub notified: u64,
}

// Platform process-state leaves (A3 part 3c): the per-backend Source/Device
// definitions are cfg-congruent with the backend module selection in
// sys/mod.rs (linux vs fallback) and sys/serial.rs (unix vs unsupported).

#[cfg(target_os = "linux")]
pub struct Source {
    pub pidfd: OwnedFd,
}

#[cfg(not(target_os = "linux"))]
pub enum Source {}

#[cfg(unix)]
pub struct Device {
    pub fd: OwnedFd,
}

#[cfg(not(unix))]
pub enum Device {}

pub struct SerialPort(pub Device);

// The std-trait impls stay beside the owned types (stage-4 W5/E13): `Device`
// and `SerialPort` live in the evaluator, so their `AsRawFd`/`io::Read`/
// `io::Write` implementations cannot stay in the system layer. The bodies are
// GNU's `emacs_read`/`emacs_write` shape: retry on `EINTR`, one syscall.
#[cfg(unix)]
impl AsRawFd for Device {
    fn as_raw_fd(&self) -> std::os::fd::RawFd {
        self.fd.as_raw_fd()
    }
}

#[cfg(unix)]
impl std::io::Read for SerialPort {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        loop {
            // SAFETY: `read` writes at most `buffer.len()` bytes through the
            // pointer, and the descriptor is owned by `self`.
            let count = unsafe {
                libc::read(
                    self.0.fd.as_raw_fd(),
                    buffer.as_mut_ptr().cast::<libc::c_void>(),
                    buffer.len(),
                )
            };
            match usize::try_from(count) {
                Ok(count) => return Ok(count),
                Err(_) => {
                    let err = std::io::Error::last_os_error();
                    if err.kind() != std::io::ErrorKind::Interrupted {
                        return Err(err);
                    }
                }
            }
        }
    }
}

#[cfg(unix)]
impl std::io::Write for SerialPort {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        loop {
            // SAFETY: `write` reads at most `buffer.len()` bytes through the
            // pointer, and the descriptor is owned by `self`.
            let count = unsafe {
                libc::write(
                    self.0.fd.as_raw_fd(),
                    buffer.as_ptr().cast::<libc::c_void>(),
                    buffer.len(),
                )
            };
            match usize::try_from(count) {
                Ok(count) => return Ok(count),
                Err(_) => {
                    let err = std::io::Error::last_os_error();
                    if err.kind() != std::io::ErrorKind::Interrupted {
                        return Err(err);
                    }
                }
            }
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

// Off Unix, `Device` is uninhabited (`unsupported_backend::open` never returns
// `Ok`). The call sites still name `Read`/`Write`; these bodies are the same
// `match *device {}` the backend uses, so Windows type-checks without a fd.
#[cfg(not(unix))]
impl std::io::Read for SerialPort {
    fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
        let device = &self.0;
        match *device {}
    }
}

#[cfg(not(unix))]
impl std::io::Write for SerialPort {
    fn write(&mut self, _buffer: &[u8]) -> std::io::Result<usize> {
        let device = &self.0;
        match *device {}
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let device = &self.0;
        match *device {}
    }
}

pub struct ChildStatusSource(pub Source);

// Self-contained state impls (A3b-1).

/// Whether this compilation of the evaluator carries the test-support fields
/// that change the layout of the wait backend and any host record embedding it.
///
/// Upper crates pin process layouts and cannot evaluate this crate's
/// `cfg(feature = "test-support")` directly.  Exposing the predicate as a
/// `const` keeps their pins exact even when Cargo unifies dev-dependency
/// features across `--all-targets` targets (where the upper crate's own
/// `cfg(test)` is off while the lower crate's test fields are on).
#[doc(hidden)]
pub const TEST_SUPPORT_LAYOUT: bool = cfg!(any(test, feature = "test-support"));

pub const DEFAULT_READ_PROCESS_OUTPUT_MAX: usize = 65_536;

pub const READ_PROCESS_OUTPUT_MAX_CEILING: usize = i32::MAX as usize;

pub const READ_OUTPUT_DELAY_INCREMENT_MS: u64 = 10;

pub const READ_OUTPUT_DELAY_MAX_MS: u64 = READ_OUTPUT_DELAY_INCREMENT_MS * 5;

pub const READ_OUTPUT_DELAY_MAX_MAX_MS: u64 = READ_OUTPUT_DELAY_INCREMENT_MS * 7;

impl ExitQueryPolicy {
    pub const GNU_MAKE_PROCESS: Self = Self::Query;

    pub fn from_lisp_query_flag(flag: Value) -> Self {
        if flag.is_truthy() {
            Self::Query
        } else {
            Self::NoQuery
        }
    }

    pub fn queries_on_exit(self) -> bool {
        self == Self::Query
    }
}

impl Default for ProcessReadConfig {
    fn default() -> Self {
        Self {
            readmax: DEFAULT_READ_PROCESS_OUTPUT_MAX,
            adaptive_read_buffering: 0,
        }
    }
}

impl ProcessKind {
    pub fn name(self) -> &'static str {
        self.into()
    }
}

impl ProcessKeyword {
    pub fn keyword(self) -> &'static str {
        self.into()
    }

    pub fn value(self) -> Value {
        Value::keyword(self.keyword())
    }

    pub fn from_keyword(name: &str) -> Option<Self> {
        name.strip_prefix(':')?.parse().ok()
    }

    pub fn from_value(value: &Value) -> Option<Self> {
        Self::from_keyword(keyword_name(value)?)
    }
}

impl std::io::Read for ChildOutputReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        // GNU emacs_read never exposes EINTR (sysdep.c:2758-2789). Finish
        // that interrupted operation before the caller can classify a failed
        // read and retire the pipe, or status_notify can publish exit while
        // unread bytes still belong to this descriptor.
        loop {
            let result = match self {
                Self::Stdout(stdout) => stdout.read(buffer),
                Self::Shared(pipe) => pipe.read(buffer),
                #[cfg(all(any(test, feature = "test-support"), unix))]
                Self::Injected(pipe) => pipe.read(buffer),
            };
            match result {
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                result => return result,
            }
        }
    }
}

#[cfg(unix)]
impl std::os::fd::AsRawFd for ChildOutputReader {
    fn as_raw_fd(&self) -> std::os::fd::RawFd {
        match self {
            Self::Stdout(stdout) => stdout.as_raw_fd(),
            Self::Shared(pipe) => pipe.as_raw_fd(),
            #[cfg(any(test, feature = "test-support"))]
            Self::Injected(pipe) => pipe.as_raw_fd(),
        }
    }
}

#[cfg(windows)]
impl std::os::windows::io::AsRawHandle for ChildOutputReader {
    fn as_raw_handle(&self) -> std::os::windows::io::RawHandle {
        match self {
            Self::Stdout(stdout) => stdout.as_raw_handle(),
            Self::Shared(pipe) => pipe.as_raw_handle(),
        }
    }
}

impl LiveProcessIo {
    /// GNU's `record_kill_process` (src/callproc.c:196-211), reached from
    /// `Fdelete_process` as `if (p->alive) record_kill_process (p, Qnil);`
    /// (src/process.c:1135).
    ///
    /// The `p->alive` gate lives in [`ChildOwnership`] now.  It used to be
    /// spelled here as `!matches!(child.try_wait(), Ok(Some(_)))`, which is a
    /// second `waitpid` on a child `sys::poll_child_status` had already reaped
    /// and reads its `ECHILD` -- *nobody has that child* -- as "still
    /// running", so `kill(-pid, SIGKILL)` went to a pid the kernel had handed
    /// back.  See `process/reap.rs`.
    pub fn terminate_and_defer_children(&mut self) {
        // GNU process.c:1135: `if (p->alive) record_kill_process (p, Qnil);`.
        // The `Unreaped` arm is this port's `p->alive`; the hook's own
        // implementation starts with the same state gate, so taking it here is
        // behaviour-identical and avoids an evaluator hook dispatch when this
        // port no longer owns a live child (a lower-crate unit test can reach
        // the drop path without any installed evaluator hook set).
        if !matches!(self.child, ChildOwnership::Unreaped(_)) {
            return;
        }
        crate::emacs_core::eval::process_hooks::terminate_and_defer_reaping(
            &mut self.child,
            signal_kill_number(),
        );
    }
}

impl Drop for LiveProcessIo {
    fn drop(&mut self) {
        self.terminate_and_defer_children();
    }
}

impl WaitNotifier {
    pub fn new(poller: Arc<polling::Poller>, notification_pending: Arc<NotificationLatch>) -> Self {
        Self {
            poller,
            notification_pending,
        }
    }

    /// Metadata-only notification. The caller publishes its durable request
    /// before this hint; failed wakeup never returns an accepted obligation.
    pub fn nudge_control(
        &self,
        destination: ControlDestination,
        reason: ControlNudge,
    ) -> ControlNudgeOutcome {
        self.notification_pending.nudge(destination, reason)
    }

    /// Wake the current (or next) `poller.wait()` so the evaluator services
    /// work already published by the caller (input, diagnostics, async DNS).
    #[must_use = "a failed notification can leave the evaluator blocked"]
    pub fn notify(&self) -> std::io::Result<()> {
        // `Poller::wait` represents both notify and timeout as an empty event
        // batch.  Preserve the semantic cause separately so the wait adapter
        // never invents a notification on a timeout.  Release pairs with the backend's
        // AcqRel swap; Poller supplies the actual cross-thread wakeup.
        self.notification_pending.store(true, Ordering::Release);
        self.poller.notify()
    }
}

#[cfg(any(test, feature = "test-support"))]
impl BackendWaitEntry {
    pub fn requested_timeout(self) -> Duration {
        self.requested_timeout
    }

    pub fn interest(self) -> ProcessWaitBackendInterest {
        self.interest
    }
}

impl ProcessWaitBackendInterest {
    pub fn wants_notifications(self) -> bool {
        matches!(
            self,
            Self::NotificationsOnly | Self::NotificationsAndProcesses
        )
    }

    pub fn wants_processes(self) -> bool {
        matches!(self, Self::ProcessesOnly | Self::NotificationsAndProcesses)
    }
}

impl Drop for ProcessWaitBackend {
    fn drop(&mut self) {
        if let Some(wake) = self.notification_pending.control() {
            wake.retire();
        }
    }
}

impl ProcessWaitBackend {
    pub fn new() -> Self {
        // Miri implements no `timerfd_create`, which `polling::Poller::new`
        // uses on Linux. The backend already tolerates a missing poller (its
        // wait loop falls back to sleep-polling), and interpreted tests do not
        // spawn children, so interpreted runs start without one.
        #[cfg(not(miri))]
        let poller = polling::Poller::new().ok().map(Arc::new);
        #[cfg(miri)]
        let poller: Option<Arc<polling::Poller>> = None;
        #[cfg(unix)]
        let signal_fd = poller.as_ref().and_then(|poller| {
            let fd = super::super::input_hooks::signal_wake_pipe_read_fd()?;
            // SAFETY: the install report owns the descriptor for the process
            // lifetime, longer than every registration. The reserved event key
            // cannot collide with a real process identifier.
            unsafe { poller.add(fd, polling::Event::readable(usize::MAX - 1)) }
                .expect("cannot register evaluator signal wake pipe");
            Some(fd)
        });
        Self {
            poller,
            #[cfg(any(test, feature = "test-support"))]
            before_next_wait: std::sync::Mutex::new(None),
            #[cfg(any(test, feature = "test-support"))]
            native_connect_observer: NativeConnectObservationSlot::default(),
            #[cfg(unix)]
            signal_fd,
            notification_pending: Arc::new(NotificationLatch::new()),
        }
    }

    pub fn poller(&self) -> Option<&polling::Poller> {
        self.poller.as_deref()
    }

    /// A shared handle producers use to wake a blocked wait (cross-platform).
    pub fn notify_handle(&self) -> Option<WaitNotifier> {
        self.poller
            .clone()
            .map(|poller| WaitNotifier::new(poller, Arc::clone(&self.notification_pending)))
    }

    pub fn has_notifications(&self) -> bool {
        // Cross-platform: any live poller can be woken via `Poller::notify()`,
        // so the unified notification+process wait path is available on every OS.
        self.poller.is_some()
    }

    pub fn wait_for_events(
        &self,
        processes: &HashMap<ProcessId, Process>,
        timeout: std::time::Duration,
        interest: ProcessWaitBackendInterest,
    ) -> Option<ProcessWaitEvents> {
        if let Some(ref poller) = self.poller {
            if super::super::input_hooks::os_signal_pending() {
                return Some(ProcessWaitEvents::notification_wakeup());
            }
            #[cfg(unix)]
            if let Some(fd) = self.signal_fd {
                // Rearm one-shot interest. A signal between the pending check
                // and wait leaves a readable byte, so no wake is lost.
                // SAFETY: install owns fd for the process lifetime.
                let source = unsafe { std::os::fd::BorrowedFd::borrow_raw(fd) };
                poller
                    .modify(source, polling::Event::readable(usize::MAX - 1))
                    .ok()?;
            }
            if interest.wants_notifications()
                && self.notification_pending.swap(false, Ordering::AcqRel)
            {
                return Some(ProcessWaitEvents::notification_wakeup());
            }

            #[cfg(any(test, feature = "test-support"))]
            if !timeout.is_zero() {
                // Nonblocking service polls are not reader wait-entry witnesses.
                let before_wait = self.before_next_wait.lock().expect("wait gate lock").take();
                if let Some(before_wait) = before_wait {
                    before_wait(BackendWaitEntry {
                        requested_timeout: timeout,
                        interest,
                    });
                }
            }

            let deadline = ProcessWaitDeadline::from(timeout);
            loop {
                let now = Instant::now();
                let wait_time = deadline.remaining(now);
                let mut events = polling::Events::new();
                match poller.wait(&mut events, wait_time) {
                    Ok(_) => {
                        let mut notification_wakeup = interest.wants_notifications()
                            && self.notification_pending.swap(false, Ordering::AcqRel);
                        let mut ready_processes = Vec::new();
                        let mut writable_processes = Vec::new();
                        for event in events.iter() {
                            #[cfg(unix)]
                            if event.key == usize::MAX - 1 {
                                if let Some(fd) = self.signal_fd {
                                    super::super::input_hooks::drain_signal_wake_pipe(fd);
                                }
                                notification_wakeup = true;
                                continue;
                            }
                            if interest.wants_processes() {
                                let id = event.key as ProcessId;
                                let Some(process) = processes.get(&id) else {
                                    continue;
                                };
                                if event.readable
                                    && (process_has_readable_process_io(process)
                                        || process_has_pending_tls_handshake_io(process)
                                        || process_has_observable_child_status(process))
                                {
                                    ready_processes.push(id);
                                }
                                if event.writable
                                    && (process.live_io.pending_network_connect.is_some()
                                        || process_has_pending_tls_handshake_io(process)
                                        || !process.write_queue.is_nil())
                                {
                                    writable_processes.push(id);
                                }
                            }
                        }
                        // A cross-platform `Poller::notify()` wake carries no
                        // poll event.  The shared pending bit above distinguishes
                        // that semantic wake from an equally eventless timeout.
                        if interest.wants_processes() {
                            ready_processes.extend(processes.iter().filter_map(|(id, process)| {
                                process_has_ready_async_dns(process).then_some(*id)
                            }));
                        }
                        let backend = ProcessWaitEvents::from_sources_with_writable(
                            notification_wakeup,
                            ready_processes,
                            writable_processes,
                        );
                        if backend.has_notification_wakeup()
                            || backend.has_ready_processes()
                            || backend.has_writable_processes()
                            || deadline.is_expired(Instant::now())
                        {
                            return Some(backend);
                        }
                        std::thread::yield_now();
                    }
                    // GNU's own wait spells the answer in one line -- `if
                    // (xerrno == EINTR) no_avail = 1;`
                    // (src/process.c:5891-5892) -- i.e. treat an interrupted
                    // poll as an empty batch and let the loop re-check its
                    // deadline, NOT as a broken poller.
                    //
                    // **This arm is not reached today, and ledger 200 measured
                    // that rather than reasoning about it.**  A previous
                    // version of this comment claimed it "became reachable the
                    // moment ledger 184 installed the first `sigaction`",
                    // because `epoll_wait` is one of the calls signal(7) lists
                    // as never restarted regardless of `SA_RESTART`.  That is
                    // true of the syscall and false of this call site:
                    // `polling::Poller::wait` catches `ErrorKind::Interrupted`
                    // from the sys poller and re-enters the wait itself
                    // (polling-3.11.0/src/lib.rs:751-764), so a delivery
                    // cannot surface here.  Measured: a confirmed SIGCHLD
                    // delivered during a 3s block left it running the full
                    // 3.000038747s.  The arm stays because `io::ErrorKind` is
                    // non-exhaustive and a future backend may surface it; what
                    // it must NOT be is a mechanism something else relies on.
                    Err(err) if err.kind() == std::io::ErrorKind::Interrupted => {
                        if deadline.is_expired(Instant::now()) {
                            return Some(ProcessWaitEvents::from_sources_with_writable(
                                false,
                                Vec::new(),
                                Vec::new(),
                            ));
                        }
                    }
                    Err(_) => {
                        return None;
                    }
                }
            }
        }

        None
    }
}

impl PendingDnsRequest {
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }
}

impl NetworkSocketOption {
    pub fn from_keyword(keyword: ProcessKeyword) -> Option<Self> {
        match keyword {
            #[cfg(any(target_os = "linux", target_os = "android"))]
            ProcessKeyword::Bindtodevice => Some(Self::Bindtodevice),
            ProcessKeyword::Broadcast => Some(Self::Broadcast),
            ProcessKeyword::Dontroute => Some(Self::Dontroute),
            ProcessKeyword::Keepalive => Some(Self::Keepalive),
            ProcessKeyword::Linger => Some(Self::Linger),
            ProcessKeyword::Oobinline => Some(Self::Oobinline),
            #[cfg(any(target_os = "linux", target_os = "android"))]
            ProcessKeyword::Priority => Some(Self::Priority),
            ProcessKeyword::Reuseaddr => Some(Self::Reuseaddr),
            ProcessKeyword::Nodelay => Some(Self::Nodelay),
            _ => None,
        }
    }
}

// State helper fns and wait types (A3b-2).

pub fn process_has_ready_async_dns(proc: &Process) -> bool {
    proc.live_io
        .pending_network_connect
        .as_ref()
        .is_some_and(pending_network_connect_has_ready_async_dns)
}

pub fn process_has_observable_child_status(proc: &Process) -> bool {
    matches!(
        ProcessStatusSymbol::from_status_value(proc.status),
        Some(ProcessStatusSymbol::Run | ProcessStatusSymbol::Stop)
    ) && (proc.os_pid.is_some() || proc.live_io.child.has_child())
}

pub fn process_has_readable_process_io(proc: &Process) -> bool {
    !process_stopped_for_io(proc)
        && proc
            .live_io
            .pty_reader
            .as_ref()
            .is_none_or(PtyOutputReader::is_monitored)
        && process_filter_accepts_output(proc)
        && process_status_has_readable_process_io(&proc.status)
}

pub fn process_has_pending_tls_handshake_io(process: &Process) -> bool {
    process.gnutls_initstage == GnutlsInitStage::HandshakeTried
        && process.live_io.tls_stream.is_some()
        && process.live_io.tls_polling == TlsPolling::Enabled
}

impl ProcessWaitDeadline {
    #[inline]
    pub fn remaining(self, now: Instant) -> Option<Duration> {
        match self {
            Self::Poll => Some(Duration::ZERO),
            Self::Until(deadline) => Some(deadline.saturating_duration_since(now)),
            Self::Forever => None,
        }
    }

    #[inline]
    pub fn is_expired(self, now: Instant) -> bool {
        match self {
            Self::Poll => true,
            Self::Until(deadline) => now >= deadline,
            Self::Forever => false,
        }
    }
}

/// A validated monotonic deadline. An overflowing duration denotes an
/// indefinite wait, which still wakes for process or evaluator notifications.
/// This immutable clock value contains no mutator-owned state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessWaitDeadline {
    Poll,
    Until(Instant),
    Forever,
}

impl ProcessWaitEvents {
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub fn from_sources(notification_wakeup: bool, ready_processes: Vec<ProcessId>) -> Self {
        Self::from_sources_with_writable(notification_wakeup, ready_processes, Vec::new())
    }

    pub fn from_sources_with_writable(
        notification_wakeup: bool,
        ready_processes: Vec<ProcessId>,
        writable_processes: Vec<ProcessId>,
    ) -> Self {
        Self {
            notification_wakeup,
            ready_processes,
            writable_processes,
        }
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub fn notification_wakeup() -> Self {
        Self::from_sources(true, Vec::new())
    }

    pub fn ready_processes(processes: Vec<ProcessId>) -> Self {
        Self {
            notification_wakeup: false,
            ready_processes: processes,
            writable_processes: Vec::new(),
        }
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub fn writable_processes(processes: Vec<ProcessId>) -> Self {
        Self::from_sources_with_writable(false, Vec::new(), processes)
    }

    pub fn has_notification_wakeup(&self) -> bool {
        self.notification_wakeup
    }

    pub fn has_ready_processes(&self) -> bool {
        !self.ready_processes.is_empty()
    }

    pub fn has_writable_processes(&self) -> bool {
        !self.writable_processes.is_empty()
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub fn has_ready_process(&self, process: ProcessId) -> bool {
        self.ready_processes.contains(&process)
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub fn is_empty(&self) -> bool {
        !self.notification_wakeup
            && self.ready_processes.is_empty()
            && self.writable_processes.is_empty()
    }

    pub fn ready_processes_ref(&self) -> &[ProcessId] {
        &self.ready_processes
    }

    pub fn writable_processes_ref(&self) -> &[ProcessId] {
        &self.writable_processes
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProcessWaitEvents {
    pub notification_wakeup: bool,
    pub ready_processes: Vec<ProcessId>,
    pub writable_processes: Vec<ProcessId>,
}

pub fn signal_kill_number() -> i32 {
    cfg_select! {
        unix => { libc::SIGKILL }
        _ => { 9 }
    }
}

pub fn keyword_name(value: &Value) -> Option<&str> {
    match value.kind() {
        ValueKind::Symbol(k) => Some(resolve_sym(k)),
        _ => None,
    }
}

// Status helpers (A3b-3).

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
pub enum ProcessStatusSymbol {
    Run,
    Stop,
    Exit,
    Signal,
    Open,
    Listen,
    Closed,
    Connect,
    Failed,
}

pub fn process_status_has_readable_process_io(status: &Value) -> bool {
    matches!(
        ProcessStatusSymbol::from_status_value(*status),
        Some(
            ProcessStatusSymbol::Run
                | ProcessStatusSymbol::Open
                | ProcessStatusSymbol::Listen
                | ProcessStatusSymbol::Connect
        )
    )
}

pub fn process_stopped_for_io(proc: &Process) -> bool {
    proc.command == Value::T
}

/// GNU `update_status` view of a process: a pending-but-unnotified child
/// status (`raw_status_new` in GNU, `status_notify_pending` +
/// `pending_status` here) is DECODED at observation points --
/// `Fprocess_status` and `Fprocess_exit_status` both run `update_status`
/// before reading -- while the sentinel notification stays pending for the
/// wait loop's `status_notify` pass.
pub fn process_effective_status(process: &Process) -> Value {
    if process.status_notify_pending && !process.pending_status.is_nil() {
        process.pending_status
    } else {
        process.status
    }
}

/// GNU `Fprocess_status`'s connection remapping (src/process.c:1193-1201):
///
/// ```c
///   if (NETCONN1_P (p) || SERIALCONN1_P (p) || PIPECONN1_P (p))
///     {
///       if (EQ (status, Qexit))          status = Qclosed;   /* :1195-1196 */
///       else if (EQ (p->command, Qt))    status = Qstop;     /* :1197-1198 */
///       else if (EQ (status, Qrun))      status = Qopen;     /* :1199-1200 */
///     }
/// ```
///
/// The chain is an `else if`, so `exit -> closed` WINS over the
/// `command == t` stop: a connection that has finished reports `closed`
/// however many times `stop-process` was called on it.  This port answered
/// `command == t` first, which reported `stop` for a `:stderr` pipe that had
/// already closed -- the last divergent row of ledger 169's three-kind
/// neighbour sweep, and reachable only once `stop-process` started setting
/// `p->command' on a retired connection the way GNU does.
pub fn process_public_status_symbol(process: &Process) -> Value {
    if process_stopped_for_io(process)
        && !matches!(
            ProcessStatusSymbol::from_status_value(process_effective_status(process)),
            Some(
                ProcessStatusSymbol::Exit
                    | ProcessStatusSymbol::Signal
                    | ProcessStatusSymbol::Closed
            )
        )
    {
        return ProcessStatusSymbol::Stop.value();
    }
    match ProcessStatusSymbol::from_status_value(process_effective_status(process)) {
        Some(ProcessStatusSymbol::Run) => match process.kind {
            ProcessKind::Network => {
                if process_contact_server_p(process) {
                    Value::symbol("listen")
                } else {
                    Value::symbol("open")
                }
            }
            ProcessKind::Pipe => Value::symbol("open"),
            _ => Value::symbol("run"),
        },
        Some(ProcessStatusSymbol::Stop) => ProcessStatusSymbol::Stop.value(),
        Some(ProcessStatusSymbol::Exit) => match process.kind {
            ProcessKind::Real => ProcessStatusSymbol::Exit.value(),
            _ => ProcessStatusSymbol::Closed.value(),
        },
        Some(ProcessStatusSymbol::Signal) => match process.kind {
            ProcessKind::Real => Value::symbol("signal"),
            _ => ProcessStatusSymbol::Closed.value(),
        },
        Some(ProcessStatusSymbol::Open) => ProcessStatusSymbol::Open.value(),
        Some(ProcessStatusSymbol::Listen) => ProcessStatusSymbol::Listen.value(),
        Some(ProcessStatusSymbol::Closed) => ProcessStatusSymbol::Closed.value(),
        Some(ProcessStatusSymbol::Connect) => ProcessStatusSymbol::Connect.value(),
        Some(ProcessStatusSymbol::Failed) => ProcessStatusSymbol::Failed.value(),
        _ => Value::NIL,
    }
}

/// Whether a network process's contact plist names a server, which
/// `Fprocess_status` projects to `listen` rather than `open`.
pub fn process_contact_server_p(proc: &Process) -> bool {
    neovm_value::symbol::plist_core::plist_get(proc.childp, &ProcessKeyword::Server.value())
        .unwrap_or(Value::NIL)
        .is_truthy()
}

pub fn process_filter_accepts_output(proc: &Process) -> bool {
    ProcessFilterDispatch::from_lisp(proc.filter).accepts_output()
}

pub fn pending_network_connect_has_ready_async_dns(pending: &PendingNetworkConnect) -> bool {
    matches!(pending, PendingNetworkConnect::Dns(request) if request.is_ready())
}

// Filter dispatch (A3b-4).

/// The three behaviors GNU assigns to a process filter Lisp value.
///
/// Keep the original [`Value`] on [`Process`] so `process-filter` can return
/// it exactly.  Classify it at the I/O boundary so Lisp `t` changes read
/// interest instead of ever being treated as a callable value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessFilterDispatch {
    Default,
    Suspended,
    Callback(Value),
}

// Filter dispatch impl (A3b-5).

impl ProcessFilterDispatch {
    pub fn from_lisp(filter: Value) -> Self {
        if filter.is_nil() || filter.is_bare_symbol_named(DEFAULT_PROCESS_FILTER_SYMBOL) {
            Self::Default
        } else if filter.is_t() {
            Self::Suspended
        } else {
            Self::Callback(filter)
        }
    }

    pub fn accepts_output(self) -> bool {
        !matches!(self, Self::Suspended)
    }
}

// Default filter symbol (A3b-6).

pub const DEFAULT_PROCESS_FILTER_SYMBOL: &str = "internal-default-process-filter";

// ProcessManager state impl (A3b-2).

/// Host-session and green-readiness accessors (stage-4 A3b/E13).
///
/// Their entire state is this module's `wait_backend`, and every caller is
/// evaluator code, so they live here now rather than on core's
/// system-side `SystemProcessExt` trait.
impl ProcessManager {
    #[doc(hidden)]
    pub fn attach_host_control_session(&self) -> crate::host_effect::wake::HostSessionControl {
        self.wait_backend
            .notification_pending
            .attach_native(self.wait_backend.poller.as_ref().map(Arc::clone))
            .control()
            .attach_host_session()
    }

    #[doc(hidden)]
    pub fn revoke_host_control_session(&self) {
        if let Some(wake) = self.wait_backend.notification_pending.control() {
            wake.revoke_host_session();
        }
    }

    #[doc(hidden)]
    pub fn observe_green_admission_refusal(
        &self,
        observe: impl FnOnce(crate::host_effect::wake::NativeReadinessCause),
    ) {
        self.wait_backend
            .notification_pending
            .attach_native(self.wait_backend.poller.as_ref().map(Arc::clone))
            .observe_admission_refusal(observe);
    }

    #[doc(hidden)]
    pub fn green_native_readiness(
        &self,
    ) -> Result<(), crate::host_effect::wake::NativeReadinessCause> {
        self.wait_backend
            .notification_pending
            .attach_native(self.wait_backend.poller.as_ref().map(Arc::clone))
            .require_native()
            .map(|_| ())
    }

    #[cfg(test)]
    #[doc(hidden)]
    pub fn refuse_green_native_readiness_for_test(
        &mut self,
        cause: crate::host_effect::wake::NativeReadinessCause,
    ) {
        if matches!(
            cause,
            crate::host_effect::wake::NativeReadinessCause::AbsentPoller
        ) {
            self.wait_backend.poller = None;
            #[cfg(unix)]
            {
                self.wait_backend.signal_fd = None;
            }
        }
        self.wait_backend
            .notification_pending
            .refuse_native_for_test(cause);
    }
}

impl ProcessManager {
    pub fn new() -> Self {
        Self {
            processes: HashMap::new(),
            deleted_processes: HashMap::new(),
            ids: Box::new(ProcessIdState {
                next_id: 1,
                last_read_process: 0,
            }),
            process_tick: 0,
            default_read_config: ProcessReadConfig::default(),
            env_overrides: HashMap::new(),
            wait_backend: ProcessWaitBackend::new(),
        }
    }

    pub fn set_default_read_config(&mut self, config: ProcessReadConfig) {
        self.default_read_config = config;
    }

    pub fn adaptive_read_timeout(&self) -> Option<Duration> {
        self.processes
            .values()
            .filter(|process| process.adaptive_read_buffering != 0)
            .filter_map(|process| {
                (!process.read_output_delay.is_zero()).then_some(process.read_output_delay)
            })
            .min()
            .map(|delay| delay.min(Duration::from_millis(READ_OUTPUT_DELAY_MAX_MS)))
    }

    pub fn clear_adaptive_read_skip_if_needed(&mut self, id: ProcessId) -> bool {
        let Some(proc) = self.processes.get_mut(&id) else {
            return false;
        };
        if proc.adaptive_read_buffering == 0
            || proc.read_output_delay.is_zero()
            || !proc.read_output_skip
        {
            return false;
        }
        proc.read_output_skip = false;
        true
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_process_record(
        &mut self,
        name: LispString,
        buffer: Value,
        command: LispString,
        args: Vec<LispString>,
        kind: ProcessKind,
        executable: Option<LispString>,
        coding: ProcessCodingSystems,
    ) -> ProcessId {
        // GNU `make_process` owns process-name allocation for every process
        // kind.  Probe the live process registry and append the smallest free
        // `<N>` suffix before the new record becomes visible.
        let name = self.allocate_process_name(name);
        let id = self.ids.next_id;
        self.ids.next_id += 1;
        let (tty_name, tty_stdin, tty_stdout, tty_stderr) = match kind {
            ProcessKind::Real => {
                let tty_name = Value::string(default_process_tty_name());
                (tty_name, true, true, true)
            }
            ProcessKind::Network | ProcessKind::Pipe | ProcessKind::Serial => {
                (Value::NIL, false, false, false)
            }
        };
        let proc_type = process_type_value(&kind);
        let childp = if kind == ProcessKind::Real {
            Value::T
        } else {
            Value::NIL
        };
        let read_config = self.default_read_config;
        let proc = Process {
            id,
            name: process_name_lisp_value(&name),
            command: make_process_command_lisp_value(&kind, &command, &args),
            executable,
            kind,
            proc_type,
            status: process_status_run_value(),
            status_notify_pending: false,
            status_ticks: StatusChangeTicks::default(),
            pending_status: Value::NIL,
            buffer,
            childp,
            write_queue: Value::NIL,
            readmax: read_config.readmax,
            adaptive_read_buffering: read_config.adaptive_read_buffering,
            read_output_delay: Duration::ZERO,
            read_output_skip: false,
            exit_query_policy: ExitQueryPolicy::GNU_MAKE_PROCESS,
            filter: Value::symbol(DEFAULT_PROCESS_FILTER_SYMBOL),
            sentinel: Value::symbol(DEFAULT_PROCESS_SENTINEL_SYMBOL),
            log: Value::NIL,
            plist: Value::NIL,
            stderrproc: Value::NIL,
            // There is no initialiser to write here any more.  GNU's
            // `make_process` leaves both slots nil and lets the creating
            // primitive's own resolver fill them in; each of the five
            // primitives now supplies its resolver's answer as an argument, so
            // the pair arrives already attributed.  The `utf-8-unix` literal
            // that used to sit here was not a neutral placeholder -- it was the
            // answer `default-process-coding-system` happens to hold under a
            // UTF-8 locale, which is why nothing noticed that
            // `make-pipe-process` and `make-serial-process` never resolved
            // anything.  See DIVERGENCES.md entries 131 and 137.
            coding_decode: coding.decode,
            coding_state: ProcessCodingState::default(),
            coding_encode: coding.encode,
            encoding_state: ProcessEncodingState::default(),
            coding_explicitly_set: false,
            explicit_coding_status_deferred_once: false,
            inherit_coding_system_flag: false,
            thread: Value::NIL,
            window_cols: None,
            window_rows: None,
            tty_name,
            tty_stdin,
            tty_stdout,
            tty_stderr,
            os_pid: None,
            input_disposition: ProcessInputDisposition::Connected,
            eof_sent_to_process: false,
            live_io: LiveProcessIo::default(),
            datagram_address: Value::NIL,
            datagram_socket_addr: None,
            #[cfg(unix)]
            datagram_unix_path: None,
            gnutls_initstage: GnutlsInitStage::Empty,
            gnutls_boot_parameters: Value::NIL,
            mark: neovm_edit::emacs_core::marker::make_marker_value(None, None, false),
            default_directory: None,
        };
        register_process_print_name(id, &process_name_runtime(proc.name));
        self.processes.insert(id, proc);
        id
    }

    pub fn allocate_process_name(&self, requested: LispString) -> LispString {
        if !self.process_name_is_in_use(&requested) {
            return requested;
        }

        for suffix in 1_u64.. {
            let suffix = LispString::from_unibyte(format!("<{suffix}>").into_bytes());
            let candidate = requested.concat(&suffix);
            if !self.process_name_is_in_use(&candidate) {
                return candidate;
            }
        }

        unreachable!("the process-name suffix space cannot be exhausted")
    }

    pub fn process_name_is_in_use(&self, candidate: &LispString) -> bool {
        self.processes.values().any(|process| {
            process.name.as_lisp_string().is_some_and(|existing| {
                // `Fget_process` searches `Vprocess_alist` with `assoc`, whose
                // string equality compares character count, byte count, and
                // contents while ignoring text properties and the
                // unibyte/multibyte flag itself.
                existing.schars() == candidate.schars()
                    && existing.sbytes() == candidate.sbytes()
                    && existing.as_bytes() == candidate.as_bytes()
            })
        })
    }

    pub fn sync_process_mark(&mut self, buffers: &mut BufferManager, id: ProcessId) -> EvalResult {
        let proc = self
            .get_mut(id)
            .ok_or_else(|| signal("error", vec![Value::string("Process not found")]))?;
        update_process_mark(buffers, proc)
    }

    pub fn set_child_status_pending(&mut self, id: ProcessId, status: Value) {
        if let Some(proc) = self.processes.get_mut(&id) {
            proc.pending_status = status;
            proc.status_notify_pending = true;
        } else {
            return;
        }
        // GNU stamps the tick and `raw_status_new` together in
        // `handle_child_signal` (src/process.c:7746-7747).  Recording the tick
        // here rather than at the callers is what keeps the invariant this
        // module rests on: every `status_notify_pending` this port sets has an
        // unnotified tick behind it, so `status_notify`'s visit set can be read
        // off the tick pair alone.
        self.record_status_change(StatusChangeSite::HandleChildSignal, id);
    }

    pub fn stderr_pipe_owner(&self, stderr_id: ProcessId) -> Option<ProcessId> {
        self.processes
            .get(&stderr_id)
            .and_then(|stderr| stderr.live_io.stderr_pipe_owner)
    }

    pub fn clear_status_notify_pending(&mut self, id: ProcessId) {
        if let Some(proc) = self.processes.get_mut(&id) {
            proc.status_notify_pending = false;
            proc.pending_status = Value::NIL;
            #[cfg(windows)]
            {}
        }
    }

    pub fn process_ids_for_buffer(&self, buffer_id: BufferId) -> Vec<ProcessId> {
        let buffer = Value::make_buffer(buffer_id);
        self.processes
            .iter()
            .filter_map(|(id, proc)| (proc.buffer == buffer).then_some(*id))
            .collect()
    }

    /// Get process status.
    pub fn process_status(&self, id: ProcessId) -> Option<&Value> {
        self.processes.get(&id).map(|p| &p.status)
    }

    /// Get process status for both live and stale process handles.
    pub fn process_status_any(&self, id: ProcessId) -> Option<&Value> {
        self.processes
            .get(&id)
            .map(|p| &p.status)
            .or_else(|| self.deleted_processes.get(&id).map(|p| &p.status))
    }

    /// Project only the live owner's actual pending TCP state. This neither
    /// changes nor claims to inspect the native poller's registration. The
    /// fixture separately requires the real canonical writable backend event.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn main_connect_pending_for_test(
        &self,
        id: ProcessId,
    ) -> Result<MainConnectPending, PendingConnectObservationError> {
        let process = self
            .get(id)
            .ok_or(PendingConnectObservationError::MissingProcess)?;
        if ProcessStatusSymbol::from_status_value(process.status)
            != Some(ProcessStatusSymbol::Connect)
        {
            return Err(PendingConnectObservationError::AlreadyDecoded);
        }
        match process.live_io.pending_network_connect.as_ref() {
            Some(PendingNetworkConnect::Tcp { .. }) => {}
            Some(PendingNetworkConnect::Dns(_)) | None => {
                return Err(PendingConnectObservationError::NotPendingTcp);
            }
            #[cfg(unix)]
            Some(PendingNetworkConnect::Local) => {
                return Err(PendingConnectObservationError::NotPendingTcp);
            }
        }
        let socket = process
            .live_io
            .network_socket
            .as_ref()
            .ok_or(PendingConnectObservationError::MissingSocket)?;
        match socket {
            NetworkSocket::TcpStream(_) => {}
            NetworkSocket::TcpListener(_) | NetworkSocket::UdpSocket(_) => {
                return Err(PendingConnectObservationError::NotTcpStream);
            }
            #[cfg(unix)]
            NetworkSocket::SeqpacketStream(_)
            | NetworkSocket::SeqpacketListener(_)
            | NetworkSocket::UnixStream(_)
            | NetworkSocket::UnixListener(_)
            | NetworkSocket::UnixDatagram(_) => {
                return Err(PendingConnectObservationError::NotTcpStream);
            }
        }
        self.wait_backend
            .poller()
            .ok_or(PendingConnectObservationError::MissingBackend)?;
        Ok(MainConnectPending { process: id })
    }

    /// Get a process by id.
    pub fn get(&self, id: ProcessId) -> Option<&Process> {
        self.processes.get(&id)
    }

    /// Get a process by id from either live or stale process tables.
    pub fn get_any(&self, id: ProcessId) -> Option<&Process> {
        self.processes
            .get(&id)
            .or_else(|| self.deleted_processes.get(&id))
    }

    /// Get a mutable process by id.
    pub fn get_mut(&mut self, id: ProcessId) -> Option<&mut Process> {
        self.processes.get_mut(&id)
    }

    /// Read the Lisp-visible status at one of the enumerated holes, without
    /// GNU's recording having been made here, and without exposing the
    /// process reference.
    ///
    /// The `site` argument is not used at run time; it exists so the call
    /// cannot be written without naming which hole it is.
    #[doc(hidden)]
    pub fn read_public_status_without_recording(
        &self,
        site: UnrecordedStatusRead,
        id: ProcessId,
    ) -> Option<Value> {
        let _ = site;
        self.get_any(id).map(process_public_status_symbol)
    }

    /// Get a mutable process by id from either live or stale process tables.
    pub fn get_any_mut(&mut self, id: ProcessId) -> Option<&mut Process> {
        if self.processes.contains_key(&id) {
            self.processes.get_mut(&id)
        } else {
            self.deleted_processes.get_mut(&id)
        }
    }

    /// List all process ids.
    pub fn list_processes(&self) -> Vec<ProcessId> {
        // GNU `process-list` is `(mapcar #'cdr Vprocess_alist)`, and a new
        // process is consed to the FRONT of `Vprocess_alist` (process.c:953), so
        // the list is newest-first. `ProcessId` is a monotonic counter, so
        // sorting by descending id reproduces GNU's order exactly (a deleted
        // process is removed from both the alist and the map).
        let mut ids: Vec<ProcessId> = self.processes.keys().copied().collect();
        ids.sort_unstable_by(|a, b| b.cmp(a));
        ids
    }

    /// Return IDs of processes with output or lifecycle work the wait loop can service.
    ///
    /// Child status observation is deliberately independent of readable I/O:
    /// a stopped child does not admit output, but GNU continues including it
    /// in `waitpid(WUNTRACED | WCONTINUED)` scans so a later `SIGCONT` can
    /// publish the `run` transition.
    ///
    /// The order is GNU's, and it is Lisp-visible.  This list drives the
    /// service pass that both drains output and publishes status, which is
    /// `status_notify`'s `FOR_EACH_PROCESS` walk (src/process.c:7885) -- over
    /// `Vprocess_alist`, onto whose FRONT `make_process` conses each new
    /// process (:953).  So the walk is NEWEST-FIRST, and two children that
    /// exited together run their sentinels in reverse creation order.  A
    /// `HashMap` iteration order made that a coin flip instead; sorting on
    /// descending `ProcessId` is the same identity `list_processes` uses to
    /// reproduce `process-list`.
    pub fn live_process_ids(&self) -> Vec<ProcessId> {
        let mut ids: Vec<ProcessId> = self
            .processes
            .iter()
            .filter(|(_, p)| {
                if p.status_notify_pending {
                    return true;
                }
                if p.live_io.pending_network_connect.is_some() {
                    return true;
                }
                if process_has_observable_child_status(p) {
                    return true;
                }
                if !process_has_readable_process_io(p) {
                    return false;
                }
                if p.live_io.network_socket.is_some() || p.live_io.tls_stream.is_some() {
                    return true;
                }
                // Standalone pipe processes (including a make-process
                // `:stderr` pipe) have no child of their own.  Their readable
                // endpoint is `child_stdout`, and they must be serviced so
                // output is drained and EOF retires them.
                if is_standalone_pipe_process(p) {
                    return true;
                }
                false
            })
            .map(|(id, _)| *id)
            .collect();
        ids.sort_unstable_by(|a, b| b.cmp(a));
        ids
    }

    /// Returns true if this id has been allocated at least once.
    pub fn was_issued_id(&self, id: ProcessId) -> bool {
        id > 0 && id < self.ids.next_id
    }

    /// Find a process by name.
    pub fn find_by_name(&self, name: &str) -> Option<ProcessId> {
        let wanted = process_name_value(name);
        self.processes
            .values()
            .find(|p| equal_value(&p.name, &wanted, 0))
            .map(|p| p.id)
    }

    /// Find a process associated with BUFFER-ID.
    pub fn find_by_buffer_id(
        &self,
        buffer_id: neovm_value::value::buffer_vocab::BufferId,
    ) -> Option<ProcessId> {
        self.processes
            .values()
            .find(|p| p.buffer.as_buffer_id() == Some(buffer_id))
            .map(|p| p.id)
    }

    pub fn queue_input(&mut self, id: ProcessId, input: &LispString) -> Result<bool, Flow> {
        let Some(proc) = self.processes.get_mut(&id) else {
            return Ok(false);
        };
        proc.write_queue =
            write_queue_push(proc.write_queue, Value::heap_string(input.clone()), false);
        Ok(true)
    }

    pub fn write_queue_is_empty(&self, id: ProcessId) -> bool {
        self.processes
            .get(&id)
            .is_none_or(|proc| proc.write_queue.is_nil())
    }

    pub fn pop_process_write_queue(&mut self, id: ProcessId) -> Option<ProcessWriteQueueEntry> {
        let proc = self.processes.get_mut(&id)?;
        let (queue, entry) = write_queue_pop(proc.write_queue);
        proc.write_queue = queue;
        entry
    }

    pub fn push_process_write_queue_entry(
        &mut self,
        id: ProcessId,
        entry: ProcessWriteQueueEntry,
        front: bool,
    ) {
        if let Some(proc) = self.processes.get_mut(&id) {
            proc.write_queue = write_queue_push_entry(proc.write_queue, entry, front);
        }
    }

    /// Get an environment variable (checking overrides first, then OS).
    pub fn getenv(&self, name: &str) -> Option<LispString> {
        let key = LispString::from_utf8(name);
        if let Some(override_val) = self.env_overrides.get(&key) {
            return override_val.clone();
        }
        std::env::var_os(name)
            .as_ref()
            .map(|value| os_str_to_lisp_string(value.as_os_str()))
    }

    /// Set an environment variable override.  If value is None, unset it.
    pub fn setenv(&mut self, name: LispString, value: Option<LispString>) {
        self.env_overrides.insert(name, value);
    }

    pub fn record_status_change(&mut self, site: StatusChangeSite, id: ProcessId) {
        // GNU's `++process_tick` is a plain `EMACS_INT` increment.  `u64`
        // saturates rather than wraps so that a wrap could never make a
        // recorded change compare equal to a notified one; at one tick per
        // status change the bound is not reachable, and saying so is cheaper
        // than reasoning about it later.
        self.process_tick = self.process_tick.saturating_add(1);
        let tick = self.process_tick;
        if let Some(proc) = self.processes.get_mut(&id) {
            proc.status_ticks.record(tick);
        }
        tracing::trace!(
            gnu = site.gnu(),
            what = site.what(),
            tick,
            "p->tick = ++process_tick"
        );
    }
}

// Wait-queue/coding/status vocabulary (A3b-7).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessWriteQueueEntry {
    pub object: Value,
    pub offset: usize,
    pub len: usize,
}

impl ProcessWriteQueueEntry {
    pub fn bytes(self) -> Option<Vec<u8>> {
        let string = self.object.as_lisp_string()?;
        let bytes = string.as_bytes();
        let start = self.offset.min(bytes.len());
        let end = start.saturating_add(self.len).min(bytes.len());
        Some(bytes[start..end].to_vec())
    }

    pub fn advance(self, written: usize) -> Self {
        let written = written.min(self.len);
        Self {
            object: self.object,
            offset: self.offset.saturating_add(written),
            len: self.len.saturating_sub(written),
        }
    }
}

pub fn write_queue_push_entry(queue: Value, entry: ProcessWriteQueueEntry, front: bool) -> Value {
    let entry = Value::cons(
        entry.object,
        Value::cons(
            Value::fixnum(entry.offset as i64),
            Value::fixnum(entry.len as i64),
        ),
    );
    let mut entries = list_to_vec(&queue).unwrap_or_default();
    if front {
        entries.insert(0, entry);
    } else {
        entries.push(entry);
    }
    Value::list(entries)
}

pub fn write_queue_push(queue: Value, input_obj: Value, front: bool) -> Value {
    let len = input_obj
        .as_lisp_string()
        .map(|string| string.sbytes())
        .unwrap_or(0);
    write_queue_push_entry(
        queue,
        ProcessWriteQueueEntry {
            object: input_obj,
            offset: 0,
            len,
        },
        front,
    )
}

/// The decode/encode coding systems an asynchronous subprocess is created with.
///
/// There is deliberately no `Default` and no field-wise constructor: GNU has
/// FIVE creation-time resolvers for this pair -- `Fcall_process`
/// (src/callproc.c:729-763), `Fmake_process` (src/process.c:1950-2008),
/// `Fmake_pipe_process` (:2517-2570), `Fmake_serial_process` (:3247-3275) and
/// `set_network_socket_coding_system` (:3291-3367) -- and they disagree about
/// the explicit override, about which operation symbol (if any) reaches
/// `process-coding-system-alist`, about whether a unibyte buffer short-circuits
/// the lookup, about WHICH buffer answers that question for each half, and
/// about whether `default-process-coding-system` applies at all.  A pair that
/// does not say which resolver produced it is a pair nobody can check, which is
/// exactly how `make-process` came to ship a coding system invented in Rust --
/// and how `make-pipe-process` and `make-serial-process` went on shipping one
/// after entry 131 fixed `make-process`.
///
/// It is a REQUIRED parameter of every process constructor, so there is no
/// signature left in which a process can come into existence without a caller
/// naming where its pair came from.
#[derive(Clone, Copy, Debug)]
pub struct ProcessCodingSystems {
    pub decode: Value,
    pub encode: Value,
}

pub const DEFAULT_PROCESS_SENTINEL_SYMBOL: &str = "internal-default-process-sentinel";

/// GNU's `p->tick = ++process_tick;` -- the assignment that puts a process
/// into `status_notify`'s visit set -- with **one variant per line in
/// `src/process.c` that spells it**.
///
/// `grep -n 'tick = ++process_tick' src/process.c` returns exactly eight lines
/// on GNU master since e381cf1fc97 (2025-08-15, "Allow child processes to
/// continue after EPIPE"), and they are the eight below.  The emacs-31.1
/// release (a360712c9d, 2026-08-24) predates that change on its branch and
/// still has a NINTH: `send_process`'s EPIPE arm at :6927, which synthesizes
/// `(exit . 256)` -- behavior 4d7e6e51dd4 introduced in 2012 and master no
/// longer has.  This port follows master's arm (see `write_process_input_once`),
/// so the ninth line is deliberately not a variant; the line numbers cited
/// on the eight variants are emacs-31.0.90's, identical in emacs-31.1
/// (master's are 1169, 1189, 6075, 6092, 6101, 6158, 7193, 7752, the same
/// eight sites renumbered).  **Seven of them have nothing to do with
/// SIGCHLD**, which is the fact this type exists to keep in front of the next
/// reader: the record that decides whom `status_notify` visits is not the
/// child-signal record.  GNU declares the counter and the reason for it at
/// :232-235:
///
/// ```c
/// /* Number of events of change of status of a process.  */
/// static EMACS_INT process_tick;
/// /* Number of events for which the user or sentinel has been notified.  */
/// static EMACS_INT update_tick;
/// ```
///
/// A ninth site cannot be added without a GNU citation, because
/// [`Self::gnu`], [`Self::what`] and [`Self::notifier`] are exhaustive matches
/// and [`Self::COUNT`] (derived from the last discriminant) is asserted in
/// `process_test.rs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum StatusChangeSite {
    /// `Fdelete_process`'s network/serial/pipe arm (:1128).
    DeleteProcessConnection = 0,
    /// `Fdelete_process`'s real-subprocess arm, `infd >= 0` (:1148).
    DeleteProcessChild,
    /// `read_process_output`'s PTY `EIO` arm for a `pid == -2` process
    /// (:6058), which becomes `Qfailed`.
    ///
    /// **Never constructed outside the table, and the compiler is right about
    /// that.**  It is GNU's line all the same, and the table is the eight lines
    /// rather than the seven this port reaches; `Self::recorder`'s `NoAnalogue`
    /// arm says why there is nothing to record here, and deleting the `allow`
    /// is how a future entry that grows the window announces itself.
    #[allow(dead_code)]
    PtyEioBeforeFork,
    /// `wait_reading_process_output`'s pipe-connection EOF arm (:6075).
    PipeConnectionReadEof,
    /// `wait_reading_process_output`'s non-EOF read failure arm (:6084),
    /// which becomes `(exit . 256)`.
    SubprocessReadFailure,
    /// `connect_network_socket`'s failed non-blocking connect (:6141), which
    /// becomes `(failed . ERRNO)`.
    NonBlockingConnectFailed,
    /// `send_process`'s EPIPE arm on the release lineage (:6927), which
    /// becomes `(exit . 256)` and deactivates the connection.  GNU master
    /// (e381cf1fc97) removed this bump; this port applies it only to network
    /// connections, which have no child whose real status must survive the
    /// write failure.  See `ProcessManager::write_process_input_once`.
    SendProcessEpipe,
    /// `process_send_signal` sent `SIGCONT` (:7178), which becomes `Qrun`.
    ProcessSendSignalSigcont,
    /// `handle_child_signal`'s `child_status_changed` (:7746) -- the only one
    /// of the eight that is the SIGCHLD record.
    HandleChildSignal,
}

/// Whether GNU consumes the bump on the spot or leaves it for the wait.
#[cfg(any(test, feature = "test-support"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusChangeNotifier {
    SynchronouslyAtTheSite {
        gnu: &'static str,
        here: &'static str,
    },
    TheWaitsStatusNotify,
}

/// Where this port makes the record for a [`StatusChangeSite`], or why not.
#[cfg(any(test, feature = "test-support"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusChangeRecorder {
    Here(&'static str),
    NoAnalogue { why: &'static str },
}

impl StatusChangeSite {
    /// Derived from the last discriminant, so a variant missing from
    /// [`Self::ALL`] is a compile error rather than a silent omission.
    #[cfg(any(test, feature = "test-support"))]
    pub const COUNT: usize = Self::HandleChildSignal as usize + 1;

    #[cfg(any(test, feature = "test-support"))]
    pub const ALL: [Self; Self::COUNT] = [
        Self::DeleteProcessConnection,
        Self::DeleteProcessChild,
        Self::PtyEioBeforeFork,
        Self::PipeConnectionReadEof,
        Self::SubprocessReadFailure,
        Self::NonBlockingConnectFailed,
        Self::SendProcessEpipe,
        Self::ProcessSendSignalSigcont,
        Self::HandleChildSignal,
    ];

    /// `file:line` of the `p->tick = ++process_tick;` in the GNU tree.
    pub fn gnu(self) -> &'static str {
        match self {
            Self::DeleteProcessConnection => "src/process.c:1128",
            Self::DeleteProcessChild => "src/process.c:1148",
            Self::PtyEioBeforeFork => "src/process.c:6058",
            Self::PipeConnectionReadEof => "src/process.c:6075",
            Self::SubprocessReadFailure => "src/process.c:6084",
            Self::NonBlockingConnectFailed => "src/process.c:6141",
            Self::SendProcessEpipe => "src/process.c:6927",
            Self::ProcessSendSignalSigcont => "src/process.c:7178",
            Self::HandleChildSignal => "src/process.c:7746",
        }
    }

    /// The status GNU publishes at the site, so the table reads as a table.
    pub fn what(self) -> &'static str {
        match self {
            Self::DeleteProcessConnection => "(exit . 0) on a deleted connection",
            Self::DeleteProcessChild => "(signal . SIGKILL) on a deleted subprocess",
            Self::PtyEioBeforeFork => "failed, on EIO before the fork completed",
            Self::PipeConnectionReadEof => "(exit . 0) on a pipe connection's EOF",
            Self::SubprocessReadFailure => "(exit . 256) on a subprocess read failure",
            Self::NonBlockingConnectFailed => "(failed . ERRNO) on a failed connect",
            Self::SendProcessEpipe => "(exit . 256) on a network connection's EPIPE",
            Self::ProcessSendSignalSigcont => "run, on SIGCONT",
            Self::HandleChildSignal => "the raw wait status of a changed child",
        }
    }

    /// Where this port records the change, or why it has nothing to record.
    #[cfg(any(test, feature = "test-support"))]
    pub fn recorder(self) -> StatusChangeRecorder {
        match self {
            Self::DeleteProcessConnection | Self::DeleteProcessChild => {
                StatusChangeRecorder::Here("ProcessManager::stamp_process_for_delete")
            }
            Self::PipeConnectionReadEof => {
                StatusChangeRecorder::Here("ProcessManager::retire_pipe_process_at_read_eof")
            }
            Self::SubprocessReadFailure => {
                StatusChangeRecorder::Here("ProcessManager::retire_process_at_read_failure")
            }
            Self::NonBlockingConnectFailed => {
                StatusChangeRecorder::Here("ProcessManager::complete_pending_network_connect")
            }
            Self::SendProcessEpipe => {
                StatusChangeRecorder::Here("ProcessManager::write_process_input_once")
            }
            Self::ProcessSendSignalSigcont => {
                StatusChangeRecorder::Here("builtin_continue_process_impl")
            }
            Self::HandleChildSignal => {
                StatusChangeRecorder::Here("ProcessManager::set_child_status_pending")
            }
            // GNU's `p->pid == -2` is the window between `allocate_pty` and a
            // successful `fork` (src/process.c:6053-6060): the PTY master is
            // open and no child owns the slave yet, so an `EIO` on it means
            // the fork will never be reported by SIGCHLD.  This port has no
            // such window -- `spawn_child_with_environment` either returns a
            // child handle or reports the failure as a status in the same
            // call, so a process is never listed with a PTY and no child --
            // and `Fprocess_id` therefore has no `-2` to answer.
            Self::PtyEioBeforeFork => StatusChangeRecorder::NoAnalogue {
                why: "no pid == -2 window: the spawn publishes its own failure status",
            },
        }
    }

    /// Whether GNU consumes the bump on the spot or leaves it for the wait.
    #[cfg(any(test, feature = "test-support"))]
    pub fn notifier(self) -> StatusChangeNotifier {
        match self {
            Self::DeleteProcessConnection => StatusChangeNotifier::SynchronouslyAtTheSite {
                gnu: "src/process.c:1129, status_notify (p, NULL)",
                here: "Context::delete_process_running_its_sentinel",
            },
            Self::DeleteProcessChild => StatusChangeNotifier::SynchronouslyAtTheSite {
                gnu: "src/process.c:1149, status_notify (p, NULL)",
                here: "Context::delete_process_running_its_sentinel",
            },
            Self::NonBlockingConnectFailed => StatusChangeNotifier::SynchronouslyAtTheSite {
                // GNU has no `status_notify` here; its :6141 bump is picked up
                // by the wait it is already inside.  This port completes a
                // pending connect and runs the `failed` sentinel in the same
                // service pass, which is the same call, so the tick must be
                // consumed there or the sentinel runs twice.
                gnu: "src/process.c:6141, inside wait_reading_process_output's own pass",
                here: "Context::service_process_output_for_ids, \
                       PendingNetworkConnectCompletion::Failed",
            },
            Self::ProcessSendSignalSigcont => StatusChangeNotifier::SynchronouslyAtTheSite {
                gnu: "src/process.c:7181, status_notify (NULL, NULL)",
                here: "Context::builtin_continue_process -> notify_process_status_sentinel",
            },
            Self::PtyEioBeforeFork
            | Self::PipeConnectionReadEof
            | Self::SubprocessReadFailure
            | Self::SendProcessEpipe
            | Self::HandleChildSignal => StatusChangeNotifier::TheWaitsStatusNotify,
        }
    }
}

// Misc process-name/status helpers (A3b-8).

/// Record a process id -> name mapping for the printer.
pub fn register_process_print_name(id: ProcessId, name: &str) {
    PROCESS_NAME_REGISTRY.with(|slot| {
        slot.borrow_mut().insert(id, name.to_string());
    });
}

pub fn process_name_value(name: &str) -> Value {
    Value::heap_string(crate::emacs_core::builtins::plain_str_to_lisp_string(
        name, true,
    ))
}

pub fn process_name_lisp_value(name: &LispString) -> Value {
    Value::heap_string(name.clone())
}

pub fn process_name_runtime(name: Value) -> String {
    name.as_lisp_string()
        .map(|ls| neovm_value::emacs_char::to_utf8_lossy(ls.as_bytes()))
        .unwrap_or_else(|| "<invalid-process-name>".to_string())
}

pub fn process_type_value(kind: &ProcessKind) -> Value {
    Value::symbol(kind.name())
}

pub fn make_process_command_lisp_value(
    kind: &ProcessKind,
    program: &LispString,
    args: &[LispString],
) -> Value {
    if *kind != ProcessKind::Real || program.is_empty() {
        return Value::NIL;
    }
    let mut items = Vec::with_capacity(args.len() + 1);
    items.push(Value::heap_string(program.clone()));
    items.extend(args.iter().cloned().map(Value::heap_string));
    Value::list(items)
}

pub fn os_str_to_lisp_string(value: &OsStr) -> LispString {
    #[cfg(unix)]
    {
        LispString::from_unibyte(value.as_bytes().to_vec())
    }

    #[cfg(not(unix))]
    {
        LispString::from_utf8(value.to_string_lossy().as_ref())
    }
}

pub fn update_process_mark(buffers: &mut BufferManager, proc: &mut Process) -> EvalResult {
    let Some(buffer_id) = proc.buffer.as_buffer_id() else {
        return neovm_edit::emacs_core::marker::builtin_set_marker_in_buffers(
            buffers,
            &[proc.mark, Value::NIL],
        );
    };
    let Some(buffer) = buffers.get(buffer_id) else {
        return neovm_edit::emacs_core::marker::builtin_set_marker_in_buffers(
            buffers,
            &[proc.mark, Value::NIL],
        );
    };
    let position = Value::fixnum(buffer.z_lisp_char_pos().as_i64());
    neovm_edit::emacs_core::marker::builtin_set_marker_in_buffers(
        buffers,
        &[proc.mark, position, proc.buffer],
    )
}

pub fn process_status_run_value() -> Value {
    Value::symbol("run")
}

pub fn write_queue_pop(queue: Value) -> (Value, Option<ProcessWriteQueueEntry>) {
    if queue.is_nil() {
        return (Value::NIL, None);
    }
    let entries = list_to_vec(&queue).unwrap_or_default();
    let Some((entry, rest)) = entries.split_first() else {
        return (Value::NIL, None);
    };
    let object = entry.cons_car();
    let offset_len = entry.cons_cdr();
    let offset = offset_len.cons_car().as_fixnum().unwrap_or(0).max(0) as usize;
    let len = offset_len.cons_cdr().as_fixnum().unwrap_or(0).max(0) as usize;
    let rest = Value::list(rest.to_vec());
    (
        rest,
        Some(ProcessWriteQueueEntry {
            object,
            offset,
            len,
        }),
    )
}

pub fn is_standalone_pipe_process(proc: &Process) -> bool {
    proc.kind == ProcessKind::Pipe
        && proc.live_io.child_stdout.is_some()
        && !proc.live_io.child.has_child()
}

// Name registry and printer helper (A3b-9).

thread_local! {
    /// Name registry keyed by process id, used by the printer to render
    /// `#<process NAME>` without threading a `ProcessManager` into the
    /// stateless print path (mirrors the terminal handle registry).  A process
    /// name never changes after creation and survives `delete-process`, so
    /// entries are inserted once and never removed.
    pub static PROCESS_NAME_REGISTRY: std::cell::RefCell<rustc_hash::FxHashMap<ProcessId, String>> =
        std::cell::RefCell::new(rustc_hash::FxHashMap::default());
}

pub fn print_process_handle(value: &Value) -> Option<String> {
    let id = value.as_process_id()?;
    let name = PROCESS_NAME_REGISTRY.with(|slot| slot.borrow().get(&id).cloned());
    Some(match name {
        Some(name) => format!("#<process {name}>"),
        None => "#<process>".to_string(),
    })
}

pub fn default_process_tty_name() -> String {
    // Fallback TTY name when the actual PTY slave path is not available.
    "/dev/pts/0".to_string()
}

// Pure state impls of moved types (A3b-4).

impl std::fmt::Debug for Process {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Process")
            .field("id", &self.id)
            .field("name", &process_name_runtime(self.name))
            .field("command", &self.command)
            .field("kind", &self.kind)
            .field("proc_type", &self.proc_type)
            .field("status", &self.status)
            .field("pending_status", &self.pending_status)
            .field("buffer", &self.buffer)
            .field("childp", &self.childp)
            .field(
                "pty_master",
                &self.live_io.pty_master.as_ref().map(|_| ".."),
            )
            .field("child", &self.live_io.child)
            .field(
                "pty_reader",
                &self.live_io.pty_reader.as_ref().map(|_| ".."),
            )
            .field(
                "pty_writer",
                &self.live_io.pty_writer.as_ref().map(|_| ".."),
            )
            .field(
                "network_socket",
                &self
                    .live_io
                    .network_socket
                    .as_ref()
                    .map(NetworkSocket::kind_name),
            )
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for ProcessManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessManager")
            .field("processes", &self.processes)
            .field("next_id", &self.ids.next_id)
            .finish()
    }
}

impl Default for ProcessManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessCodingState {
    /// GNU's `p->decoding_carryover` as the next read sees it.
    pub fn carryover_len(&self) -> usize {
        self.carryover.len()
    }

    /// GNU's `p->decoding_carryover = 0` (src/process.c:6312), which happens
    /// before the read decides anything: the tail is MOVED into the run that
    /// is about to be decoded, so a `:post-read-conversion` that re-enters this
    /// process through `accept-process-output` finds it at zero.
    pub fn take_carryover(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.carryover)
    }

    /// `read_process_output_set_last_coding_system`'s half of the write-back
    /// (src/process.c:6448-6457), which runs AFTER the decode.
    pub fn store_carryover(&mut self, carryover: Vec<u8>) {
        self.carryover = carryover;
    }

    /// GNU `coding->spec` as the next read must find it.
    ///
    /// GNU has no write-back here at all, because the decode ran through this
    /// very struct: an ISO-2022 designation is in `coding->spec.iso_2022` the
    /// instant the decoder records it.  This port hands a COPY to the decode
    /// and takes the copy back afterwards, which differs from GNU in exactly
    /// one case -- a `:post-read-conversion` that calls
    /// `accept-process-output` on the process it is decoding for would see the
    /// designations as of before its own run rather than after it.  The
    /// carryover has the same shape and GNU makes the same choice for it, by
    /// clearing `p->decoding_carryover` before the decode (:6312) and writing
    /// the new one after (:6448).
    pub fn store_decoder(&mut self, decoder: neovm_text::encoding::CodingDecoderState) {
        self.decoder = decoder;
    }

    /// The decoder state a run starts from.
    pub fn decoder(&self) -> neovm_text::encoding::CodingDecoderState {
        self.decoder.clone()
    }

    /// The three lines at src/process.c:6315-6321, as one answer.
    pub fn reach_last_block(&mut self) -> LastBlockArrival {
        match self.last_block {
            LastBlock::Reached => LastBlockArrival::AlreadyRaised,
            LastBlock::NotReached => {
                self.last_block = LastBlock::Reached;
                LastBlockArrival::JustRaised
            }
        }
    }

    /// Restart the descriptor as `setup_process_coding_systems` does, keeping
    /// `p->decoding_carryover`: those pending source bytes belong to the
    /// process, outside the coding system that setup replaces (process.c:8380).
    pub fn reset(&mut self) {
        self.last_block = LastBlock::default();
        self.decoder = neovm_text::encoding::CodingDecoderState::default();
    }
}

impl ProcessEncodingState {
    pub fn initialized(
        coding: neovm_text::encoding::RuntimeCodingSystem,
        systems: &neovm_text::emacs_core::coding::CodingSystemManager,
        eol_conversion: neovm_text::emacs_core::coding::EolConversion,
    ) -> Result<Self, Flow> {
        use neovm_text::emacs_core::coding::{EolConversion, EolType};
        let metadata = coding.metadata(systems).ok_or_else(|| {
            signal(
                LispCondition::CodingSystemError,
                vec![Value::symbol(coding.symbol())],
            )
        })?;
        let info = metadata.info();
        // setup_coding_system derives flags from effective EOL, hooks, and
        // runtime codec type (GNU coding.c:5681-5696, 5719-5870).
        let character_conversion = !matches!(
            neovm_value::intern::resolve_sym(info.coding_type),
            "raw-text" | "undecided"
        );
        let eol_conversion_required = eol_conversion == EolConversion::Enabled
            && matches!(metadata.eol_type(), EolType::Dos | EolType::Mac);
        let eligibility = if character_conversion
            || info.pre_write_conversion.is_some()
            || eol_conversion_required
        {
            ProcessEncodingEligibility::Required
        } else {
            ProcessEncodingEligibility::Bypass
        };
        let raw = match metadata.eol_type() {
            EolType::Dos => "raw-text-dos",
            EolType::Mac => "raw-text-mac",
            EolType::Unix => "raw-text-unix",
            EolType::Undecided => "raw-text",
        };
        Ok(Self::Active(ProcessEncodingDescriptor {
            coding,
            raw_coding: neovm_text::encoding::RuntimeCodingSystem::from_symbol(
                neovm_value::intern::intern(raw),
            ),
            eligibility,
            encoder: neovm_text::encoding::CodingEncoderState::default(),
        }))
    }

    pub fn coding(&self) -> Option<neovm_text::encoding::RuntimeCodingSystem> {
        match self {
            Self::Uninitialized => None,
            Self::Active(descriptor) => Some(descriptor.coding),
        }
    }

    pub fn requires_encoding(&self) -> bool {
        matches!(
            self,
            Self::Active(ProcessEncodingDescriptor {
                eligibility: ProcessEncodingEligibility::Required,
                ..
            })
        )
    }

    /// Only send_process's input-mode transition selects another descriptor.
    /// Codec continuation access after callbacks must never reselect it.
    pub fn select_input(
        &mut self,
        systems: &neovm_text::emacs_core::coding::CodingSystemManager,
        configured: neovm_text::encoding::RuntimeCodingSystem,
        multibyte: bool,
        eol_conversion: neovm_text::emacs_core::coding::EolConversion,
    ) -> Result<neovm_text::encoding::RuntimeCodingSystem, Flow> {
        let Self::Active(descriptor) = self else {
            return Err(signal(
                "error",
                vec![Value::string("Process output coding is not initialized")],
            ));
        };
        let reported = descriptor.coding;
        if multibyte {
            if descriptor.coding != configured {
                *self = Self::initialized(configured, systems, eol_conversion)?;
            }
            Ok(configured)
        } else {
            if descriptor.eligibility == ProcessEncodingEligibility::Required {
                *self = Self::initialized(descriptor.raw_coding, systems, eol_conversion)?;
            }
            Ok(reported)
        }
    }

    pub fn encoder_mut(&mut self) -> Option<&mut neovm_text::encoding::CodingEncoderState> {
        match self {
            Self::Uninitialized => None,
            Self::Active(descriptor) => Some(&mut descriptor.encoder),
        }
    }
}

impl StatusChangeTicks {
    /// GNU `p->tick = ++process_tick;`
    pub fn record(&mut self, tick: u64) {
        self.changed = tick;
    }

    /// GNU `p->update_tick = p->tick;` (:7894, and again at :7935).
    pub fn mark_notified(&mut self) {
        self.notified = self.changed;
    }

    /// GNU `p->tick != p->update_tick` (:7892) -- `status_notify`'s membership
    /// test, and the only question this type answers.
    pub fn is_unnotified(self) -> bool {
        self.changed != self.notified
    }
}

impl PtyOutputReader {
    pub fn new(reader: Box<dyn Read + Send>) -> Self {
        Self {
            reader,
            interest: PtyReadInterest::Monitored,
        }
    }

    pub fn is_monitored(&self) -> bool {
        match self.interest {
            PtyReadInterest::Monitored => true,
            PtyReadInterest::DrainOnly => false,
        }
    }

    pub fn stop_monitoring(&mut self) {
        self.interest = PtyReadInterest::DrainOnly;
    }
}

impl Read for PtyOutputReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        // portable-pty's reader calls libc::read directly. GNU emacs_read
        // (sysdep.c:2758-2789) never exposes EINTR as EOF or a permanent read
        // failure; finish that same interrupted operation before classifying it.
        loop {
            match self.reader.read(buffer) {
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                result => return result,
            }
        }
    }
}

// LastBlockArrival (A3b-5).

/// What a zero-byte read found the latch in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LastBlockArrival {
    /// This read raised the flag, so it falls THROUGH to the decode.
    JustRaised,
    /// The flag was already up: `return nbytes` with nothing decoded.
    AlreadyRaised,
}

// GcTrace moved verbatim with the state type (peer ruling condition 3).

impl GcTrace for ProcessManager {
    fn trace_roots(&self, roots: &mut Vec<Value>) {
        for process in self
            .processes
            .values()
            .chain(self.deleted_processes.values())
        {
            roots.push(process.name);
            roots.push(process.proc_type);
            roots.push(process.buffer);
            roots.push(process.mark);
            roots.push(process.command);
            roots.push(process.childp);
            roots.push(process.status);
            roots.push(process.pending_status);
            roots.push(process.tty_name);
            roots.push(process.write_queue);
            roots.push(process.filter);
            roots.push(process.sentinel);
            roots.push(process.log);
            roots.push(process.plist);
            roots.push(process.stderrproc);
            roots.push(process.datagram_address);
            roots.push(process.coding_decode);
            roots.push(process.coding_encode);
            roots.push(process.thread);
            roots.push(process.gnutls_boot_parameters);
        }
    }
}

// ProcessStatusSymbol impl (A3b-7).

impl ProcessStatusSymbol {
    pub fn from_symbol_value(value: Value) -> Option<Self> {
        value.as_bare_symbol_name()?.parse().ok()
    }

    pub fn from_status_value(status: Value) -> Option<Self> {
        Self::from_symbol_value(process_status_symbol_value(status))
    }

    pub fn value(self) -> Value {
        Value::symbol(self.name())
    }

    pub fn name(self) -> &'static str {
        self.into()
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn gnu_public_domain() -> [Self; 9] {
        [
            Self::Run,
            Self::Stop,
            Self::Exit,
            Self::Signal,
            Self::Open,
            Self::Listen,
            Self::Closed,
            Self::Connect,
            Self::Failed,
        ]
    }
}

// Status symbol helper (A3b-8).

pub fn process_status_symbol_value(status: Value) -> Value {
    list_to_vec(&status)
        .and_then(|items| items.first().copied())
        .unwrap_or(status)
}

// Debug impl moved with the type (A3d).

impl std::fmt::Debug for PtyOutputReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PtyOutputReader")
            .field("interest", &self.interest)
            .finish_non_exhaustive()
    }
}

/// Round 3w (plan-3w.md section 7, C1): this World state lives in a
/// thread-local while its World's Baton holder runs, and in the World while
/// it is parked. The Green runtime takes it at a park and restores it at the
/// next resume, so every Lisp thread sees one value, as in GNU. (N-mutator
/// mode needs shared storage instead: a recorded obligation.)
#[cold]
pub fn world_state_placeholder() -> Box<dyn std::any::Any> {
    Box::new(rustc_hash::FxHashMap::<ProcessId, String>::default())
}

#[cold]
pub fn swap_world_state(slot: &mut dyn std::any::Any) {
    let slot = slot
        .downcast_mut::<rustc_hash::FxHashMap<ProcessId, String>>()
        .expect("carried World state");
    PROCESS_NAME_REGISTRY.with(|names| std::mem::swap(&mut *names.borrow_mut(), slot));
}

pub fn make_network_process_subfeatures() -> Value {
    // Advertise only behavior that this runtime actually implements.  Packages
    // use `featurep' to choose code paths, so keep this list tied to backed
    // behavior, not parser acceptance.
    let mut features = vec![
        Value::keyword("nodelay"),
        Value::keyword("reuseaddr"),
        Value::keyword("oobinline"),
        Value::keyword("linger"),
        Value::keyword("keepalive"),
        Value::keyword("dontroute"),
        Value::keyword("broadcast"),
        // GNU's eight `ADD_SUBFEATURE' calls, in the order the finished list
        // reads.  `src/process.c:9072-9089' conses each onto the front, so the
        // list is the REVERSE of the source order: `:nowait' is added first and
        // ends up last, `:server' is added last and ends up first.  Ledger 197
        // put these in GNU's order; before it they ran the other way, which no
        // check could see because the only comparison against GNU
        // (`crates/neovm-oracle-tests/src/process/feature_advertisement_semantics.rs:28')
        // `sort's the list first, and a sorted comparison is a comparison of
        // sets.
        Value::list(vec![Value::keyword("server"), Value::T]),
        Value::list(vec![Value::keyword("service"), Value::T]),
        Value::list(vec![Value::keyword("family"), Value::symbol("ipv6")]),
        Value::list(vec![Value::keyword("family"), Value::symbol("ipv4")]),
        Value::list(vec![Value::keyword("family"), Value::symbol("local")]),
        // Local SOCK_SEQPACKET connections are fully backed (server accept +
        // client + data delivery verified against GNU); GNU advertises this
        // under HAVE_SEQPACKET (process.c `ADD_SUBFEATURE (QCtype,
        // Qseqpacket)`).
        Value::list(vec![Value::keyword("type"), Value::symbol("seqpacket")]),
        Value::list(vec![Value::keyword("type"), Value::symbol("datagram")]),
        Value::list(vec![Value::keyword("nowait"), Value::T]),
    ];
    cfg_select! {
        any(target_os = "linux", target_os = "android") => {
            features.insert(2, Value::keyword("priority"));
            features.insert(8, Value::keyword("bindtodevice"));
        }
        _ => {}
    }
    Value::list(features)
}

/// A Lisp-visible status read that this port CANNOT put the sweep in front
/// of, with the reason.
///
/// Every such read is a hole in the guarantee above, so the holes are a
/// finite type rather than a habit: adding one is adding a variant, which
/// forces a GNU citation and a written reason through the exhaustive
/// [`UnrecordedStatusRead::why`] match, and `COUNT` is asserted in
/// `process_test.rs` so a second hole cannot appear unremarked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
#[doc(hidden)]
pub enum UnrecordedStatusRead {
    /// The `%s` mode-line construct.  GNU's `decode_mode_spec` spells it
    /// `obj = Fsymbol_name (Fprocess_status (obj));` (src/xdisp.c:29717-
    /// 29725), so in GNU it IS one of `Fprocess_status`'s callers and does
    /// harvest.
    ///
    /// Here it cannot: `expand_mode_line_percent_in_state` (xdisp.rs:2644)
    /// takes `&ProcessManager`, and so does every frame of the recursive
    /// mode-line renderer above it.  Threading `&mut` through redisplay to
    /// reach one `%` spec is a change to redisplay, not to process status,
    /// and it is not measurable from `--batch`: `format-mode-line` answers
    /// `""` for EVERY spec there, `%b` included, in both editors.
    ModeLinePercentS = 0,
}

impl UnrecordedStatusRead {
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub const COUNT: usize = Self::ModeLinePercentS as usize + 1;
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub const ALL: [Self; Self::COUNT] = [Self::ModeLinePercentS];

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn gnu(self) -> &'static str {
        match self {
            Self::ModeLinePercentS => "src/xdisp.c:29723",
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn why(self) -> &'static str {
        match self {
            Self::ModeLinePercentS => {
                "the recursive mode-line renderer holds &ProcessManager, not &mut"
            }
        }
    }
}

// --- Moved down from the upper process layer (stage-4 W5/E13): both impls
// belong to evaluator-owned types; the trait impls are orphan-legal only here.

impl From<Duration> for ProcessWaitDeadline {
    #[inline]
    fn from(timeout: Duration) -> Self {
        if timeout.is_zero() {
            Self::Poll
        } else {
            Instant::now()
                .checked_add(timeout)
                .map_or(Self::Forever, Self::Until)
        }
    }
}

impl std::fmt::Debug for ChildOwnership {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAChild => f.write_str("NotAChild"),
            Self::Unreaped(child) => write!(f, "Unreaped({})", child.pid),
            Self::Reaped => f.write_str("Reaped"),
            Self::Deferred => f.write_str("Deferred"),
        }
    }
}
