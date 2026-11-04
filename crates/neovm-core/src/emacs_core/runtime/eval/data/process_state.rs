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
use std::io::Read;
use std::net::{SocketAddr, TcpListener, TcpStream, UdpSocket};
#[cfg(unix)]
use std::os::fd::OwnedFd;
#[cfg(unix)]
use std::os::unix::net::{UnixDatagram, UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::time::Duration;

use neovm_value::heap_types::{LispString, ProcessId};
use neovm_value::value::Value;
use num_enum::IntoPrimitive;
use polling::Poller;
use socket2::Socket;
use strum::{EnumString, IntoStaticStr};

use crate::emacs_core::eval::data::process_spawn::SpawnedChild;
use crate::emacs_core::eval::data::process_tls::TlsStream;
#[cfg(test)]
use crate::emacs_core::process::types::NativeConnectObservationSlot;
use crate::host_effect::wake::{
    ControlDestination, ControlNudge, ControlNudgeOutcome, NotificationLatch,
};

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
    #[cfg(all(test, unix))]
    Injected(crate::emacs_core::process::pipe_exit_tests::PipeReader),
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

pub struct ProcessWaitBackend {
    /// A per-backend, one-shot scheduling gate for input-wakeup regressions.
    /// Installation and consumption are synchronized; callbacks own no Lisp state.
    #[cfg(test)]
    pub before_next_wait: std::sync::Mutex<Option<Box<dyn FnOnce(BackendWaitEntry) + Send>>>,
    #[cfg(test)]
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
#[cfg(test)]
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
    pub decoder: crate::encoding::CodingDecoderState,
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
    pub coding: crate::encoding::RuntimeCodingSystem,
    pub raw_coding: crate::encoding::RuntimeCodingSystem,
    pub eligibility: ProcessEncodingEligibility,
    pub encoder: crate::encoding::CodingEncoderState,
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

pub struct ChildStatusSource(pub Source);
