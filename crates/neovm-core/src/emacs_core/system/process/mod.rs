//! Process/subprocess management for the Elisp VM.
//!
//! Provides process abstractions: creating, killing, querying, and
//! communicating with subprocesses.  `start-process` creates a tracked
//! record; `call-process` and `shell-command-to-string` run real OS
//! commands via `std::process::Command`.
//!
//! ## Network processes
//!
//! `make-network-process` supports TCP streams, UDP datagrams, and Unix local
//! sockets on platforms that provide them. Network sockets are registered with
//! the process I/O poller so `accept-process-output` and `poll_process_output`
//! wake on incoming data.  Unix child pipes are also poller-backed; Windows
//! child pipes are serviced by synchronous `PeekNamedPipe` polling.
//!
//! **TLS**: `gnutls-boot` upgrades a network process through the Neomacs TLS
//! facade. The `TcpStream` is moved into the backend-neutral
//! live-I/O owner. Read/write/send automatically use the TLS layer when present.
//! The default rustls backend uses Mozilla roots and augments them with any
//! GNU-compatible `:trustfiles` supplied by Lisp.

use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::{expect_args, expect_min_args};
use num_enum::IntoPrimitive;
use socket2::{Domain, Protocol, SockAddr, SockRef, Socket, Type};
use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::io::{Read as IoRead, Write as IoWrite};
use std::net::{
    IpAddr, Ipv4Addr, Ipv6Addr, Shutdown, SocketAddr, TcpListener, TcpStream, UdpSocket,
};
#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(unix)]
use std::os::unix::ffi::{OsStrExt, OsStringExt};
#[cfg(unix)]
use std::os::unix::net::{SocketAddr as UnixSocketAddr, UnixDatagram, UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use strum::{EnumString, IntoStaticStr};

use super::tls::{
    RustlsBackend, TlsBackendError, TlsClientBackend, TlsClientParameters, TlsStream,
    gnutls_close_notify_result_value, gnutls_peer_status_to_value, parse_gnutls_boot_parameters,
};
use super::wait::ProcessOutputWaitOutcome;

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

/// Platform abstraction layer for OS-specific subprocess facilities (currently
/// the child-status wait source). See `process/sys/mod.rs`.
pub(crate) mod sys;
use sys::ChildStatusSource;

mod executable;
pub(super) use executable::{ExecutableLookupMode, ExecutableSearch};

/// GNU `status_notify`'s "retire, then run the sentinel" ordering, as a type.
/// See `process/status_notify.rs`.
mod status_notify;
use status_notify::ProcessStatusNotification;

/// GNU `handle_child_signal`'s ASYNCHRONOUS child-status recording
/// (src/process.c:7691), and the type that keeps a Lisp-visible status from
/// being read before the recording has been made.  See
/// `process/child_status.rs`.
pub(crate) mod child_status;
pub(crate) use child_status::{
    StatusChangeSite, StatusChangeTicks, UnrecordedStatusRead, UpdateStatusSite,
};

/// The single owner of `waitpid`, and GNU's `p->alive` as a type: a child that
/// has been reaped has no pid to hand to `kill(2)` or to a second `waitpid`.
/// See `process/reap.rs`.
pub(crate) mod reap;
pub(crate) use reap::{ChildOwnership, ChildStatusChange};

/// What GNU's `status_notify` does with a process whose status has just become
/// terminal, chosen by `delete-exited-processes` (src/process.c:7926-7929, the
/// variable at :8916-8920 with default 1).
///
/// A bool at these call sites reads as "should we delete?", which is the
/// question, not the answer; the two answers are different GNU functions with
/// different observable effects, so they are spelled out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExitedProcessDisposition {
    /// GNU `remove_process` (src/process.c:957-966): drop the process from
    /// `Vprocess_alist`, so `get-process`, `get-buffer-process` and
    /// `process-list` stop returning it.  The object itself is untouched.
    Remove,
    /// GNU `deactivate_process` (src/process.c:4812): close the descriptors and
    /// leave the process listed.
    Deactivate,
}

impl ExitedProcessDisposition {
    fn from_delete_exited_processes(delete_exited_processes: bool) -> Self {
        if delete_exited_processes {
            Self::Remove
        } else {
            Self::Deactivate
        }
    }
}

/// Which of GNU's single `deactivate_process` teardown this port must apply.
///
/// GNU needs no such distinction: every kind's descriptors live in
/// `p->open_fd[]` and `deactivate_process` (src/process.c:4812) closes them all.
/// This port keeps a real child's pty and pipes, a network connection's socket
/// and a TLS stream in different fields of `Process::live_io`, so the teardown
/// is selected by what the process actually had open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProcessIoTeardown {
    /// A real child (or a pipe) that has terminated: drop all live I/O.
    Terminal,
    /// A network connection whose peer closed: drop the socket and any TLS
    /// stream, keeping the rest of `live_io` as this port's network accessors
    /// expect.
    Network,
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
enum ProcessInputDisposition {
    #[default]
    Connected,
    Discard,
    Disconnected,
}

/// The concrete endpoint selected by GNU's `send_process` precedence.
///
/// Every variant must define both its write operation and its EPIPE cleanup.
/// This makes adding another process transport a compile-time obligation
/// instead of letting the shared error arm silently close an unrelated pipe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProcessInputEndpoint {
    Pty,
    ChildPipe,
    Tls,
    Network,
    Serial,
}

impl ProcessInputEndpoint {
    fn select(proc: &Process) -> Option<Self> {
        if proc.live_io.pty_writer.is_some() {
            Some(Self::Pty)
        } else if proc.live_io.child.has_pipe_child() && proc.live_io.child.stdin().is_some() {
            Some(Self::ChildPipe)
        } else if proc.live_io.tls_stream.is_some() {
            Some(Self::Tls)
        } else if proc.live_io.network_socket.is_some() {
            Some(Self::Network)
        } else if proc.live_io.serial_port.is_some() {
            Some(Self::Serial)
        } else {
            None
        }
    }

    fn write_once(self, proc: &mut Process, bytes: &[u8]) -> Option<std::io::Result<usize>> {
        match self {
            Self::Pty => Some(proc.live_io.pty_writer.as_mut()?.write(bytes)),
            Self::ChildPipe => {
                let stdin = proc.live_io.child.stdin_mut()?;
                #[cfg(unix)]
                {
                    use std::os::unix::io::AsRawFd;
                    let fd = stdin.as_raw_fd();
                    let _ = sys::set_fd_nonblocking(fd);
                }
                Some(stdin.write(bytes))
            }
            Self::Tls => Some(
                proc.live_io
                    .tls_stream
                    .as_mut()?
                    .write_process_input_once(bytes),
            ),
            Self::Network => {
                let datagram_address = proc.datagram_socket_addr;
                #[cfg(unix)]
                let datagram_unix_path = proc.datagram_unix_path.clone();
                proc.live_io.network_socket.as_mut()?.write_input_once(
                    bytes,
                    datagram_address,
                    #[cfg(unix)]
                    datagram_unix_path,
                )
            }
            Self::Serial => Some(proc.live_io.serial_port.as_mut()?.write(bytes)),
        }
    }

    /// Disable precisely the write capability that produced EPIPE.
    ///
    /// PTY and child-pipe writers have independent Rust owners and can be
    /// dropped. The other variants share one owner with the readable side, so
    /// [`ProcessInputDisposition::Disconnected`] disables future sends while
    /// retaining the resource for remaining output, just like GNU leaves
    /// `p->infd` alive after setting `p->outfd = -1`.
    fn disconnect(self, poller: Option<&polling::Poller>, proc: &mut Process) {
        match self {
            Self::Pty => {
                proc.live_io.pty_writer = None;
            }
            Self::ChildPipe => {
                if let (Some(poller), Some(stdin)) = (poller, proc.live_io.child.stdin()) {
                    ProcessManager::unregister_child_stdin_writable_from_poller(poller, stdin);
                }
                let _ = proc.live_io.child.close_stdin();
            }
            Self::Tls | Self::Network | Self::Serial => {}
        }
        proc.input_disposition = ProcessInputDisposition::Disconnected;
    }
}

/// GNU-compatible GnuTLS process initialization stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, IntoPrimitive)]
#[repr(i64)]
pub(crate) enum GnutlsInitStage {
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

impl NetworkSocket {
    fn kind_name(&self) -> &'static str {
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

    fn register_readable(&self, poller: &polling::Poller, id: ProcessId) -> Result<(), String> {
        match self {
            Self::TcpStream(stream) => ProcessManager::register_readable_source(poller, stream, id),
            Self::TcpListener(listener) => {
                ProcessManager::register_readable_source(poller, listener, id)
            }
            Self::UdpSocket(socket) => ProcessManager::register_readable_source(poller, socket, id),
            #[cfg(unix)]
            Self::SeqpacketStream(socket) => {
                ProcessManager::register_readable_source(poller, socket, id)
            }
            #[cfg(unix)]
            Self::SeqpacketListener(socket) => {
                ProcessManager::register_readable_source(poller, socket, id)
            }
            #[cfg(unix)]
            Self::UnixStream(stream) => {
                ProcessManager::register_readable_source(poller, stream, id)
            }
            #[cfg(unix)]
            Self::UnixListener(listener) => {
                ProcessManager::register_readable_source(poller, listener, id)
            }
            #[cfg(unix)]
            Self::UnixDatagram(socket) => {
                ProcessManager::register_readable_source(poller, socket, id)
            }
        }
    }

    fn register_writable(&self, poller: &polling::Poller, id: ProcessId) -> Result<(), String> {
        match self {
            Self::TcpStream(stream) => ProcessManager::register_writable_source(poller, stream, id),
            Self::TcpListener(_) => Err("Listener sockets are not writable process sources".into()),
            Self::UdpSocket(socket) => ProcessManager::register_writable_source(poller, socket, id),
            #[cfg(unix)]
            Self::SeqpacketStream(socket) => {
                ProcessManager::register_writable_source(poller, socket, id)
            }
            #[cfg(unix)]
            Self::SeqpacketListener(_) => {
                Err("Listener sockets are not writable process sources".into())
            }
            #[cfg(unix)]
            Self::UnixStream(stream) => {
                ProcessManager::register_writable_source(poller, stream, id)
            }
            #[cfg(unix)]
            Self::UnixListener(_) => {
                Err("Listener sockets are not writable process sources".into())
            }
            #[cfg(unix)]
            Self::UnixDatagram(socket) => {
                ProcessManager::register_writable_source(poller, socket, id)
            }
        }
    }

    fn unregister_readable(&self, poller: &polling::Poller) {
        match self {
            Self::TcpStream(stream) => {
                let _ = poller.delete(stream);
            }
            Self::TcpListener(listener) => {
                let _ = poller.delete(listener);
            }
            Self::UdpSocket(socket) => {
                let _ = poller.delete(socket);
            }
            #[cfg(unix)]
            Self::SeqpacketStream(socket) => {
                let _ = poller.delete(socket);
            }
            #[cfg(unix)]
            Self::SeqpacketListener(socket) => {
                let _ = poller.delete(socket);
            }
            #[cfg(unix)]
            Self::UnixStream(stream) => {
                let _ = poller.delete(stream);
            }
            #[cfg(unix)]
            Self::UnixListener(listener) => {
                let _ = poller.delete(listener);
            }
            #[cfg(unix)]
            Self::UnixDatagram(socket) => {
                let _ = poller.delete(socket);
            }
        }
    }

    fn read_stream_output(&mut self, buf: &mut [u8]) -> Option<std::io::Result<usize>> {
        match self {
            Self::TcpStream(stream) => Some(stream.read(buf)),
            Self::TcpListener(_) => None,
            Self::UdpSocket(_) => None,
            #[cfg(unix)]
            Self::SeqpacketStream(socket) => Some(socket.read(buf)),
            #[cfg(unix)]
            Self::SeqpacketListener(_) => None,
            #[cfg(unix)]
            Self::UnixStream(stream) => Some(stream.read(buf)),
            #[cfg(unix)]
            Self::UnixListener(_) => None,
            #[cfg(unix)]
            Self::UnixDatagram(_) => None,
        }
    }

    fn write_input_once(
        &mut self,
        bytes: &[u8],
        datagram_addr: Option<SocketAddr>,
        #[cfg(unix)] datagram_unix_path: Option<std::path::PathBuf>,
    ) -> Option<std::io::Result<usize>> {
        match self {
            Self::TcpStream(stream) => Some(stream.write(bytes)),
            Self::TcpListener(_) => None,
            Self::UdpSocket(socket) => Some(match datagram_addr {
                Some(addr) => socket.send_to(bytes, addr),
                None => Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "No datagram address",
                )),
            }),
            #[cfg(unix)]
            Self::SeqpacketStream(socket) => Some(socket.write(bytes)),
            #[cfg(unix)]
            Self::SeqpacketListener(_) => None,
            #[cfg(unix)]
            Self::UnixStream(stream) => Some(stream.write(bytes)),
            #[cfg(unix)]
            Self::UnixListener(_) => None,
            #[cfg(unix)]
            Self::UnixDatagram(socket) => Some(match datagram_unix_path {
                Some(path) => socket.send_to(bytes, path),
                None => Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "No datagram address",
                )),
            }),
        }
    }

    fn modify_interest(
        &self,
        poller: &polling::Poller,
        id: ProcessId,
        event: polling::Event,
    ) -> Result<(), String> {
        match self {
            Self::TcpStream(stream) => ProcessManager::modify_poll_source(poller, stream, event),
            Self::TcpListener(listener) => ProcessManager::modify_poll_source(
                poller,
                listener,
                polling::Event::readable(id as usize),
            ),
            Self::UdpSocket(socket) => ProcessManager::modify_poll_source(poller, socket, event),
            #[cfg(unix)]
            Self::SeqpacketStream(socket) => {
                ProcessManager::modify_poll_source(poller, socket, event)
            }
            #[cfg(unix)]
            Self::SeqpacketListener(socket) => ProcessManager::modify_poll_source(
                poller,
                socket,
                polling::Event::readable(id as usize),
            ),
            #[cfg(unix)]
            Self::UnixStream(stream) => ProcessManager::modify_poll_source(poller, stream, event),
            #[cfg(unix)]
            Self::UnixListener(listener) => ProcessManager::modify_poll_source(
                poller,
                listener,
                polling::Event::readable(id as usize),
            ),
            #[cfg(unix)]
            Self::UnixDatagram(socket) => ProcessManager::modify_poll_source(poller, socket, event),
        }
    }

    fn shutdown_write(&self) -> Option<std::io::Result<()>> {
        match self {
            Self::TcpStream(stream) => Some(stream.shutdown(Shutdown::Write)),
            Self::TcpListener(_) => None,
            Self::UdpSocket(_) => None,
            #[cfg(unix)]
            Self::SeqpacketStream(socket) => Some(socket.shutdown(Shutdown::Write)),
            #[cfg(unix)]
            Self::SeqpacketListener(_) => None,
            #[cfg(unix)]
            Self::UnixStream(stream) => Some(stream.shutdown(Shutdown::Write)),
            #[cfg(unix)]
            Self::UnixListener(_) => None,
            #[cfg(unix)]
            Self::UnixDatagram(_) => None,
        }
    }

    fn take_pending_connect_error(&self) -> Option<std::io::Result<Option<std::io::Error>>> {
        match self {
            Self::TcpStream(stream) => Some(stream.take_error()),
            #[cfg(unix)]
            Self::UnixStream(stream) => Some(stream.take_error()),
            _ => None,
        }
    }
}

use super::error::{EvalResult, Flow, signal};
use super::intern::{intern, resolve_sym};
use super::threads::ThreadManager;
use super::value::{Value, ValueKind, VecLikeType, equal_value, list_to_vec};
use crate::buffer::{
    BufferId, BufferManager, EmacsByteLen, EmacsBytePos, EmacsByteRange, LispCharPos1,
};
use crate::gc_trace::GcTrace;
use crate::heap_types::LispString;
use crate::window::FrameManager;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Unique identifier for a process.
pub type ProcessId = u64;

const DEFAULT_READ_PROCESS_OUTPUT_MAX: usize = 65_536;
const READ_PROCESS_OUTPUT_MAX_CEILING: usize = i32::MAX as usize;
const READ_OUTPUT_DELAY_INCREMENT_MS: u64 = 10;
const READ_OUTPUT_DELAY_MAX_MS: u64 = READ_OUTPUT_DELAY_INCREMENT_MS * 5;
const READ_OUTPUT_DELAY_MAX_MAX_MS: u64 = READ_OUTPUT_DELAY_INCREMENT_MS * 7;

/// Whether a live process participates in the exit confirmation query.
///
/// GNU stores this as the inverse `kill_without_query` bit.  Naming the two
/// policies directly keeps that inversion out of process creation and, more
/// importantly, makes the accepted-client default explicit: `make_process`
/// creates a querying client even when its listening server is `:noquery t`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExitQueryPolicy {
    Query,
    NoQuery,
}

impl ExitQueryPolicy {
    const GNU_MAKE_PROCESS: Self = Self::Query;

    fn from_lisp_query_flag(flag: Value) -> Self {
        if flag.is_truthy() {
            Self::Query
        } else {
            Self::NoQuery
        }
    }

    fn queries_on_exit(self) -> bool {
        self == Self::Query
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ProcessReadConfig {
    readmax: usize,
    adaptive_read_buffering: u8,
}

impl Default for ProcessReadConfig {
    fn default() -> Self {
        Self {
            readmax: DEFAULT_READ_PROCESS_OUTPUT_MAX,
            adaptive_read_buffering: 0,
        }
    }
}

thread_local! {
    /// Name registry keyed by process id, used by the printer to render
    /// `#<process NAME>` without threading a `ProcessManager` into the
    /// stateless print path (mirrors the terminal handle registry).  A process
    /// name never changes after creation and survives `delete-process`, so
    /// entries are inserted once and never removed.
    static PROCESS_NAME_REGISTRY: std::cell::RefCell<rustc_hash::FxHashMap<ProcessId, String>> =
        std::cell::RefCell::new(rustc_hash::FxHashMap::default());
}

/// Record a process id -> name mapping for the printer.
pub(crate) fn register_process_print_name(id: ProcessId, name: &str) {
    PROCESS_NAME_REGISTRY.with(|slot| {
        slot.borrow_mut().insert(id, name.to_string());
    });
}

/// Look up a process name for printing `#<process NAME>`.
///
/// Returns `None` only for an id that was never registered (it then prints as a
/// bare `#<process>` fallback).
pub(crate) fn print_process_handle(value: &Value) -> Option<String> {
    let id = value.as_process_id()?;
    let name = PROCESS_NAME_REGISTRY.with(|slot| slot.borrow().get(&id).cloned());
    Some(match name {
        Some(name) => format!("#<process {name}>"),
        None => "#<process>".to_string(),
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProcessOutputWaitTiming {
    Poll,
    For(Duration),
    Forever,
}

impl ProcessOutputWaitTiming {
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn is_poll(self) -> bool {
        matches!(self, Self::Poll)
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn is_finite(self) -> bool {
        matches!(self, Self::For(_))
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn is_forever(self) -> bool {
        matches!(self, Self::Forever)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ProcessOutputWaitRequest {
    timing: ProcessOutputWaitTiming,
    target_process: Option<ProcessId>,
    just_this_one: bool,
    allow_timers: bool,
}

impl ProcessOutputWaitRequest {
    pub(crate) fn new(
        timing: ProcessOutputWaitTiming,
        target_process: Option<ProcessId>,
        just_this_one: bool,
        allow_timers: bool,
    ) -> Self {
        Self {
            timing,
            target_process,
            just_this_one,
            allow_timers,
        }
    }

    pub(crate) fn timing(self) -> ProcessOutputWaitTiming {
        self.timing
    }

    pub(crate) fn target_process(self) -> Option<ProcessId> {
        self.target_process
    }

    pub(crate) fn just_this_one(self) -> bool {
        self.just_this_one
    }

    pub(crate) fn allow_timers(self) -> bool {
        self.allow_timers
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProcessOutputServiceRequest {
    None,
    Any { target: Option<ProcessId> },
    TargetOnly(ProcessId),
}

impl ProcessOutputServiceRequest {
    pub(crate) fn none() -> Self {
        Self::None
    }

    pub(crate) fn any(target: Option<ProcessId>) -> Self {
        Self::Any { target }
    }

    pub(crate) fn target_only(target: ProcessId) -> Self {
        Self::TargetOnly(target)
    }

    fn target_process(self) -> Option<ProcessId> {
        match self {
            Self::None | Self::Any { target: None } => None,
            Self::Any {
                target: Some(target),
            }
            | Self::TargetOnly(target) => Some(target),
        }
    }

    fn live_processes(self, live_processes: Vec<ProcessId>) -> Vec<ProcessId> {
        match self {
            Self::None => Vec::new(),
            Self::Any { .. } => live_processes,
            Self::TargetOnly(target) => vec![target],
        }
    }

    fn ready_processes(self, ready_processes: Vec<ProcessId>) -> Vec<ProcessId> {
        match self {
            Self::None => Vec::new(),
            Self::Any { .. } => dedupe_process_ids(ready_processes),
            Self::TargetOnly(target) => ready_processes
                .contains(&target)
                .then_some(target)
                .into_iter()
                .collect(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ProcessOutputServiceActivity {
    #[default]
    None,
    Any,
    Target,
}

impl ProcessOutputServiceActivity {
    fn record(self, target: bool) -> Self {
        if target || matches!(self, Self::Target) {
            Self::Target
        } else {
            Self::Any
        }
    }

    fn any(self) -> bool {
        matches!(self, Self::Any | Self::Target)
    }

    fn target(self) -> bool {
        matches!(self, Self::Target)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProcessOutputServiceOutcome {
    activity: ProcessOutputServiceActivity,
    /// Non-output servicing happened: a connect completed, a server accepted a
    /// connection, a sentinel/status notification ran, or an EOF was handled.
    /// GNU's `wait_reading_process_output` services all of these inside the
    /// wait WITHOUT terminating it — only actual output bytes make
    /// `got_some_output` positive (process.c:5588/6018), so only reads may
    /// complete an `accept-process-output`.
    serviced: bool,
}

impl ProcessOutputServiceOutcome {
    /// Record output read from a process (GNU `got_some_output = nread`).
    /// This is the only activity class that completes a process wait.
    pub(crate) fn record_activity(&mut self, target: bool) {
        self.activity = self.activity.record(target);
    }

    /// Record non-output servicing (connects, accepts, sentinels, EOF).
    /// Keeps the wait running, exactly like GNU.
    pub(crate) fn record_serviced(&mut self) {
        self.serviced = true;
    }

    pub(crate) fn absorb(&mut self, other: Self) {
        if other.has_target_process_activity() {
            self.record_activity(true);
        } else if other.has_any_process_activity() {
            self.record_activity(false);
        }
        if other.has_serviced_activity() {
            self.record_serviced();
        }
    }

    pub(crate) fn has_any_process_activity(self) -> bool {
        self.activity.any()
    }

    pub(crate) fn has_target_process_activity(self) -> bool {
        self.activity.target()
    }

    pub(crate) fn has_serviced_activity(self) -> bool {
        self.serviced
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProcessWaitEvents {
    notification_wakeup: bool,
    ready_processes: Vec<ProcessId>,
    writable_processes: Vec<ProcessId>,
}

impl ProcessWaitEvents {
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn from_sources(notification_wakeup: bool, ready_processes: Vec<ProcessId>) -> Self {
        Self::from_sources_with_writable(notification_wakeup, ready_processes, Vec::new())
    }

    pub(crate) fn from_sources_with_writable(
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
    pub(crate) fn notification_wakeup() -> Self {
        Self::from_sources(true, Vec::new())
    }

    pub(crate) fn ready_processes(processes: Vec<ProcessId>) -> Self {
        Self {
            notification_wakeup: false,
            ready_processes: processes,
            writable_processes: Vec::new(),
        }
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn writable_processes(processes: Vec<ProcessId>) -> Self {
        Self::from_sources_with_writable(false, Vec::new(), processes)
    }

    pub(crate) fn has_notification_wakeup(&self) -> bool {
        self.notification_wakeup
    }

    pub(crate) fn has_ready_processes(&self) -> bool {
        !self.ready_processes.is_empty()
    }

    pub(crate) fn has_writable_processes(&self) -> bool {
        !self.writable_processes.is_empty()
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn has_ready_process(&self, process: ProcessId) -> bool {
        self.ready_processes.contains(&process)
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn is_empty(&self) -> bool {
        !self.notification_wakeup
            && self.ready_processes.is_empty()
            && self.writable_processes.is_empty()
    }

    pub(crate) fn ready_processes_ref(&self) -> &[ProcessId] {
        &self.ready_processes
    }

    pub(crate) fn writable_processes_ref(&self) -> &[ProcessId] {
        &self.writable_processes
    }
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

impl ProcessKind {
    fn name(self) -> &'static str {
        self.into()
    }
}

/// The process kinds whose record can be brought into existence with no OS
/// device in hand.
///
/// `Serial` is deliberately absent.  GNU's `Fmake_serial_process` opens the
/// port at src/process.c:3212 -- before the buffer, before the coding chain and
/// before `serial_process_configure` -- and unwinds the whole record if that
/// open fails.  So a serial process record and an open device are the same
/// event, and the only constructor that can produce one takes the opened
/// [`sys::SerialPort`] by value: [`ProcessManager::create_serial_process`].
/// Before DIVERGENCES.md entry 147 `ProcessKind::Serial` could simply be passed
/// to the generic constructor, and it was -- which is exactly how
/// `make-serial-process` came to build process records for ports that were
/// never opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProcessKindWithoutDevice {
    Real,
    Network,
    Pipe,
}

impl From<ProcessKindWithoutDevice> for ProcessKind {
    fn from(kind: ProcessKindWithoutDevice) -> Self {
        match kind {
            ProcessKindWithoutDevice::Real => Self::Real,
            ProcessKindWithoutDevice::Network => Self::Network,
            ProcessKindWithoutDevice::Pipe => Self::Pipe,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
enum ProcessStatusSymbol {
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

impl ProcessStatusSymbol {
    fn from_symbol_value(value: Value) -> Option<Self> {
        value.as_symbol_name()?.parse().ok()
    }

    fn from_status_value(status: Value) -> Option<Self> {
        Self::from_symbol_value(process_status_symbol_value(status))
    }

    fn value(self) -> Value {
        Value::symbol(self.name())
    }

    fn name(self) -> &'static str {
        self.into()
    }

    #[cfg(test)]
    fn gnu_public_domain() -> [Self; 9] {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
enum ProcessTtyStream {
    Stdin,
    Stdout,
    Stderr,
}

impl ProcessTtyStream {
    fn from_value(value: &Value) -> Option<Self> {
        value.as_symbol_name()?.parse().ok()
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn name(self) -> &'static str {
        self.into()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
#[strum(prefix = ":", serialize_all = "kebab-case")]
enum ProcessKeyword {
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

impl ProcessKeyword {
    fn keyword(self) -> &'static str {
        self.into()
    }

    fn value(self) -> Value {
        Value::keyword(self.keyword())
    }

    fn from_keyword(name: &str) -> Option<Self> {
        name.strip_prefix(':')?.parse().ok()
    }

    fn from_value(value: &Value) -> Option<Self> {
        Self::from_keyword(keyword_name(value)?)
    }
}

fn process_keyword_already_seen(seen: &mut Vec<ProcessKeyword>, keyword: ProcessKeyword) -> bool {
    if seen.contains(&keyword) {
        true
    } else {
        seen.push(keyword);
        false
    }
}

/// Operating-system resources owned by a live process connection.
///
/// GNU keeps Lisp process identity/status alive after `remove_process`, but
/// `deactivate_process` closes every descriptor immediately.  Keeping all
/// native handles in one Rust owner gives Neomacs the same lifetime split:
/// `Process` is durable Lisp-visible state, while replacing this bundle with
/// `Default::default()` drops every live handle as one operation.
#[derive(Default)]
struct LiveProcessIo {
    /// Pollable child-status wakeup source, where the platform exposes one.
    child_status_source: Option<ChildStatusSource>,
    /// The OS child this process owns, and GNU's `p->alive` for it.
    ///
    /// One slot for both spawn shapes -- the `std::process::Child` a pipe (or
    /// a `pre_exec` pty) leaves behind and the `portable_pty` handle -- because
    /// they are the same obligation: each owns a `waitpid`, and GNU allows
    /// exactly one owner (src/process.c:1080-1088).  See `process/reap.rs`.
    child: ChildOwnership,
    /// OS-level output pipe for non-blocking reads (pipe mode).  With no
    /// explicit `:stderr`, this is one shared pipe carrying both stdout and
    /// stderr in the child's write order, as in GNU Emacs.
    child_stdout: Option<ChildOutputReader>,
    /// Writable endpoint owned by a `make-pipe-process` connection.
    module_pipe_writer: Option<os_pipe::PipeWriter>,
    /// Real process that received this pipe's writer as its `:stderr`.
    ///
    /// GNU obtains this ordering relation from process-alist insertion order.
    /// Keeping the actual transfer owner here avoids confusing it with later
    /// processes that merely retain the same Lisp `:stderr` reference after
    /// the one writable endpoint has already been consumed.
    stderr_pipe_owner: Option<ProcessId>,
    /// PTY master handle for resize and I/O (PTY mode).
    pty_master: Option<Box<dyn portable_pty::MasterPty + Send>>,
    /// PTY reader for non-blocking reads from the master side.
    pty_reader: Option<Box<dyn IoRead + Send>>,
    /// PTY writer for sending input to the master side.
    pty_writer: Option<Box<dyn std::io::Write + Send>>,
    network_socket: Option<NetworkSocket>,
    pending_network_connect: Option<PendingNetworkConnect>,
    /// TLS-wrapped stream for encrypted network connections.
    tls_stream: Option<TlsStream>,
    /// The open device behind a serial process.  GNU keeps one descriptor for
    /// both directions (`p->infd = fd; p->outfd = fd`, src/process.c:3216-3217),
    /// so this single slot is the read source AND the write source.
    ///
    /// It is `Option` only because `LiveProcessIo` is one struct for every
    /// process kind and `deactivate_process_io` empties it; a serial process
    /// cannot be BORN without one -- see [`ProcessManager::create_serial_process`].
    serial_port: Option<sys::SerialPort>,
}

enum ChildOutputReader {
    Stdout(std::process::ChildStdout),
    Shared(os_pipe::PipeReader),
}

impl std::io::Read for ChildOutputReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Stdout(stdout) => stdout.read(buffer),
            Self::Shared(pipe) => pipe.read(buffer),
        }
    }
}

#[cfg(unix)]
fn duplicate_module_pipe_writer(writer: &os_pipe::PipeWriter) -> Option<std::ffi::c_int> {
    use std::os::fd::AsRawFd;

    sys::dup_fd(writer.as_raw_fd())
}

#[cfg(windows)]
fn duplicate_module_pipe_writer(writer: &os_pipe::PipeWriter) -> Option<std::ffi::c_int> {
    use std::os::windows::io::IntoRawHandle;
    use windows_sys::Win32::Foundation::CloseHandle;

    unsafe extern "C" {
        fn _open_osfhandle(os_handle: isize, flags: std::ffi::c_int) -> std::ffi::c_int;
        fn _dup(fd: std::ffi::c_int) -> std::ffi::c_int;
        fn _close(fd: std::ffi::c_int) -> std::ffi::c_int;
    }

    const O_WRONLY: std::ffi::c_int = 0x0001;
    const O_BINARY: std::ffi::c_int = 0x8000;

    let handle = writer.try_clone().ok()?.into_raw_handle();
    let fd = unsafe { _open_osfhandle(handle as isize, O_WRONLY | O_BINARY) };
    if fd == -1 {
        unsafe {
            CloseHandle(handle as windows_sys::Win32::Foundation::HANDLE);
        }
        return None;
    }

    let duplicate = unsafe { _dup(fd) };
    unsafe {
        _close(fd);
    }
    (duplicate != -1).then_some(duplicate)
}

#[cfg(unix)]
impl std::os::fd::AsRawFd for ChildOutputReader {
    fn as_raw_fd(&self) -> std::os::fd::RawFd {
        match self {
            Self::Stdout(stdout) => stdout.as_raw_fd(),
            Self::Shared(pipe) => pipe.as_raw_fd(),
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

#[cfg(windows)]
fn peek_child_output_readiness(stdout: &ChildOutputReader) -> std::io::Result<Option<usize>> {
    use std::ffi::c_void;
    use std::os::windows::io::AsRawHandle;
    use std::ptr::null_mut;
    use windows_sys::Win32::Foundation::{
        ERROR_BROKEN_PIPE, ERROR_NO_DATA, ERROR_PIPE_NOT_CONNECTED, GetLastError,
    };

    unsafe extern "system" {
        fn PeekNamedPipe(
            named_pipe: *mut c_void,
            buffer: *mut c_void,
            buffer_size: u32,
            bytes_read: *mut u32,
            total_bytes_available: *mut u32,
            bytes_left_this_message: *mut u32,
        ) -> i32;
    }

    let mut available = 0u32;
    let ok = unsafe {
        PeekNamedPipe(
            stdout.as_raw_handle(),
            null_mut(),
            0,
            null_mut(),
            &mut available,
            null_mut(),
        )
    };
    if ok != 0 {
        return Ok(Some(available as usize));
    }

    let error = unsafe { GetLastError() };
    match error {
        ERROR_BROKEN_PIPE | ERROR_PIPE_NOT_CONNECTED => Ok(None),
        ERROR_NO_DATA => Ok(Some(0)),
        error => Err(std::io::Error::from_raw_os_error(error as i32)),
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
    fn terminate_and_reap_children(&mut self) {
        self.child.terminate_and_reap(signal_kill_number());
    }
}

impl Drop for LiveProcessIo {
    fn drop(&mut self) {
        self.terminate_and_reap_children();
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
    pub(crate) status_ticks: StatusChangeTicks,
    /// Start of the bounded Windows grace period for observing an owner exit
    /// before notifying an implicit stderr pipe whose EOF arrived first.
    #[cfg(windows)]
    stderr_pipe_owner_status_deferred_at: Option<Instant>,
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
    pub(crate) exit_query_policy: ExitQueryPolicy,
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
    input_disposition: ProcessInputDisposition,
    /// True after Lisp explicitly called `process-send-eof' on this process.
    /// GNU can publish the ensuing pipe status in the same wait that reads
    /// output from that explicit EOF, while naturally exiting pipe children can
    /// remain `run' until a later wait.  PTY subprocesses have their own
    /// same-wait status rule.
    pub eof_sent_to_process: bool,
    /// Native resources exist only while this process has active I/O.
    live_io: LiveProcessIo,
    /// Current peer address for datagram network processes, as Lisp.
    pub datagram_address: Value,
    /// Current peer address for datagram network processes, as a Rust socket address.
    pub datagram_socket_addr: Option<SocketAddr>,
    /// Current peer address for Unix datagram network processes.
    #[cfg(unix)]
    pub datagram_unix_path: Option<PathBuf>,
    /// GNU-compatible GnuTLS initialization stage for this process.
    pub(crate) gnutls_initstage: GnutlsInitStage,
    /// Deferred parameters set by `gnutls-asynchronous-parameters`.
    pub(crate) gnutls_boot_parameters: Value,
    /// End-of-output marker, matching GNU's `p->mark`.
    pub mark: Value,
    /// Working directory for the subprocess, derived from
    /// `default-directory` at the time the process was created.
    /// If `None`, the child inherits the Rust process's cwd.
    pub default_directory: Option<PathBuf>,
}

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

/// Manages the set of live processes.
///
/// Uses `polling::Poller` for efficient I/O multiplexing (epoll on Linux,
/// kqueue on macOS, wepoll on Windows) instead of sleep-based polling.
pub struct ProcessManager {
    processes: HashMap<ProcessId, Process>,
    deleted_processes: HashMap<ProcessId, Process>,
    next_id: ProcessId,
    /// GNU's file-scope `process_tick` (src/process.c:232-233), *"Number of
    /// events of change of status of a process"*.  GNU's `update_tick`
    /// counterpart (:234-235) has no field here on purpose: it is only ever
    /// read at two lines (:5524, :5845), both of them a performance
    /// short-circuit deciding whether to bother calling `status_notify`, and
    /// this port's walk is unconditional.  The invariant lives in the
    /// per-process [`StatusChangeTicks`].
    process_tick: u64,
    default_read_config: ProcessReadConfig,
    /// Environment variable overrides (for `setenv`/`getenv`).
    env_overrides: HashMap<LispString, Option<LispString>>,
    wait_backend: ProcessWaitBackend,
}

/// Transactional ownership of a `make-pipe-process` writer while a child is
/// being spawned.
///
/// Until `commit` records the child that inherited the writer, dropping this
/// guard restores the endpoint to the pipe process.  Every `?` and early return
/// in the pipe and PTY spawn paths therefore has the same recovery behavior.
struct StderrPipeWriterTransfer<'a> {
    processes: &'a mut ProcessManager,
    owner_id: ProcessId,
    stderr_pipe_id: Option<ProcessId>,
    writer: Option<os_pipe::PipeWriter>,
    committed: bool,
}

impl StderrPipeWriterTransfer<'_> {
    fn pipe_id(&self) -> Option<ProcessId> {
        self.stderr_pipe_id
    }

    fn writer(&self) -> Option<&os_pipe::PipeWriter> {
        self.writer.as_ref()
    }

    fn commit(&mut self) {
        if let Some(stderr_id) = self.stderr_pipe_id
            && let Some(stderr_proc) = self.processes.processes.get_mut(&stderr_id)
        {
            stderr_proc.live_io.stderr_pipe_owner = Some(self.owner_id);
        }
        drop(self.writer.take());
        self.committed = true;
    }
}

impl std::ops::Deref for StderrPipeWriterTransfer<'_> {
    type Target = ProcessManager;

    fn deref(&self) -> &Self::Target {
        self.processes
    }
}

impl std::ops::DerefMut for StderrPipeWriterTransfer<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.processes
    }
}

impl Drop for StderrPipeWriterTransfer<'_> {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        let Some(stderr_id) = self.stderr_pipe_id else {
            return;
        };
        let Some(writer) = self.writer.take() else {
            return;
        };
        let Some(stderr_proc) = self.processes.processes.get_mut(&stderr_id) else {
            return;
        };
        if stderr_proc.live_io.module_pipe_writer.is_none() {
            stderr_proc.live_io.module_pipe_writer = Some(writer);
        }
    }
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
    poller: Arc<polling::Poller>,
    notification_pending: Arc<AtomicBool>,
}

impl WaitNotifier {
    fn new(poller: Arc<polling::Poller>, notification_pending: Arc<AtomicBool>) -> Self {
        Self {
            poller,
            notification_pending,
        }
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

struct ProcessWaitBackend {
    /// I/O multiplexer for process descriptors and cross-thread notifications.
    ///
    /// Shared (`Arc`) so any cross-thread producer can wake a blocked
    /// `poller.wait()` via the cross-platform `Poller::notify()` — the basis for
    /// the unified single-poll wait loop (no per-OS wakeup pipe needed).
    poller: Option<Arc<polling::Poller>>,
    /// Semantic cause paired with the poller's eventless notify wakeup.
    ///
    /// A bool is sufficient because notifications are coalescing readiness,
    /// not a count: one wake tells the evaluator to service published work.
    notification_pending: Arc<AtomicBool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProcessWaitBackendInterest {
    ProcessesOnly,
    NotificationsOnly,
    NotificationsAndProcesses,
}

impl ProcessWaitBackendInterest {
    fn wants_notifications(self) -> bool {
        matches!(
            self,
            Self::NotificationsOnly | Self::NotificationsAndProcesses
        )
    }

    fn wants_processes(self) -> bool {
        matches!(self, Self::ProcessesOnly | Self::NotificationsAndProcesses)
    }
}

impl ProcessWaitBackend {
    fn new() -> Self {
        Self {
            poller: polling::Poller::new().ok().map(Arc::new),
            notification_pending: Arc::new(AtomicBool::new(false)),
        }
    }

    fn poller(&self) -> Option<&polling::Poller> {
        self.poller.as_deref()
    }

    /// A shared handle producers use to wake a blocked wait (cross-platform).
    fn notify_handle(&self) -> Option<WaitNotifier> {
        self.poller
            .clone()
            .map(|poller| WaitNotifier::new(poller, Arc::clone(&self.notification_pending)))
    }

    fn has_notifications(&self) -> bool {
        // Cross-platform: any live poller can be woken via `Poller::notify()`,
        // so the unified notification+process wait path is available on every OS.
        self.poller.is_some()
    }

    fn wait_for_events(
        &self,
        processes: &HashMap<ProcessId, Process>,
        timeout: std::time::Duration,
        interest: ProcessWaitBackendInterest,
    ) -> Option<ProcessWaitEvents> {
        if let Some(ref poller) = self.poller {
            if interest.wants_notifications()
                && self.notification_pending.swap(false, Ordering::AcqRel)
            {
                return Some(ProcessWaitEvents::notification_wakeup());
            }

            let deadline = Instant::now() + timeout;
            loop {
                let now = Instant::now();
                let wait_time = if timeout.is_zero() {
                    Duration::ZERO
                } else {
                    deadline.saturating_duration_since(now)
                };
                let mut events = polling::Events::new();
                match poller.wait(&mut events, Some(wait_time)) {
                    Ok(_) => {
                        let notification_wakeup = interest.wants_notifications()
                            && self.notification_pending.swap(false, Ordering::AcqRel);
                        let mut ready_processes = Vec::new();
                        let mut writable_processes = Vec::new();
                        for event in events.iter() {
                            if interest.wants_processes() {
                                let id = event.key as ProcessId;
                                let Some(process) = processes.get(&id) else {
                                    continue;
                                };
                                if event.readable
                                    && (process_has_readable_process_io(process)
                                        || process_has_observable_child_status(process))
                                {
                                    ready_processes.push(id);
                                }
                                if event.writable
                                    && (process.live_io.pending_network_connect.is_some()
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
                            || timeout.is_zero()
                            || Instant::now() >= deadline
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
                        if timeout.is_zero() || Instant::now() >= deadline {
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

/// Buffer work that GNU performs specifically for an accepted connection.
///
/// This is a plan rather than a raw `Value`: the listener's buffer object is
/// never itself a valid accepted-client buffer.  A default-filter client gets
/// a separately named buffer; a custom-filter client gets no buffer.
enum AcceptedClientBuffer {
    None,
    CreateNamed(String),
}

impl AcceptedClientBuffer {
    fn from_server(
        buffers: &BufferManager,
        server_buffer: Value,
        server_filter: Value,
        server_name: &str,
        client_name: &str,
    ) -> Self {
        if !server_filter.is_symbol_named(DEFAULT_PROCESS_FILTER_SYMBOL)
            && server_filter != Value::T
        {
            return Self::None;
        }

        let base_name = if server_buffer.is_nil() {
            Some(server_name.to_string())
        } else {
            server_buffer
                .as_buffer_id()
                .and_then(|id| buffers.get(id))
                .map(|buffer| buffer.name_runtime_string_owned())
        };
        let Some(base_name) = base_name else {
            // GNU's `buffer-name` returns nil for a dead listener buffer.
            return Self::None;
        };
        let Some(caller_suffix) = client_name.strip_prefix(server_name) else {
            return Self::None;
        };
        Self::CreateNamed(format!("{base_name}{caller_suffix}"))
    }

    fn materialize(self, buffers: &mut BufferManager) -> Value {
        match self {
            Self::None => Value::NIL,
            Self::CreateNamed(name) => {
                let id = buffers
                    .find_buffer_by_name(&name)
                    .unwrap_or_else(|| buffers.create_buffer(&name));
                Value::make_buffer(id)
            }
        }
    }
}

/// The complete allow-list of listener state inherited by an accepted client.
///
/// Deliberately absent: listener identity, log callback, buffer object, live
/// I/O, status, and [`ExitQueryPolicy`].  Adding one of those behaviors now
/// requires changing this type instead of accidentally growing a field-copy
/// block.  This mirrors GNU `server_accept_connection` rather than treating a
/// client as a clone of its server.
#[derive(Clone, Copy)]
struct AcceptedNetworkClientInheritance {
    filter: Value,
    sentinel: Value,
    plist: Value,
    coding: ProcessCodingSystems,
    inherit_coding_system_flag: bool,
    thread: Value,
}

/// Everything consumed to create one accepted network client.
struct AcceptedNetworkClientSpec {
    name: String,
    buffer: AcceptedClientBuffer,
    contact: Value,
    socket: NetworkSocket,
    inheritance: AcceptedNetworkClientInheritance,
}

struct AcceptedNetworkConnection {
    server_id: ProcessId,
    client_id: ProcessId,
    log: Value,
    sentinel: Value,
    log_message: String,
    sentinel_message: String,
}

fn accepted_network_process_name(server_name: &str, addr: SocketAddr) -> String {
    match addr {
        SocketAddr::V4(v4) => format!("{} <{}:{}>", server_name, v4.ip(), v4.port()),
        SocketAddr::V6(v6) => format!("{} <[{}]:{}>", server_name, v6.ip(), v6.port()),
    }
}

impl std::fmt::Debug for ProcessManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessManager")
            .field("processes", &self.processes)
            .field("next_id", &self.next_id)
            .finish()
    }
}

impl Default for ProcessManager {
    fn default() -> Self {
        Self::new()
    }
}

fn process_name_value(name: &str) -> Value {
    Value::heap_string(super::builtins::plain_str_to_lisp_string(name, true))
}

fn process_name_lisp_value(name: &LispString) -> Value {
    Value::heap_string(name.clone())
}

fn process_name_runtime(name: Value) -> String {
    name.as_lisp_string()
        .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
        .unwrap_or_else(|| "<invalid-process-name>".to_string())
}

fn process_is_datagram_network(proc: &Process) -> bool {
    let is_datagram = matches!(
        proc.live_io.network_socket.as_ref(),
        Some(NetworkSocket::UdpSocket(_))
    );
    #[cfg(unix)]
    let is_datagram = is_datagram
        || matches!(
            proc.live_io.network_socket.as_ref(),
            Some(NetworkSocket::UnixDatagram(_))
        );
    is_datagram
}

/// Durable GNU `DATAGRAM_CONN_P` semantics for cold control paths.
///
/// The live-resource check keeps low-level fixtures working; the contact type
/// remains after native I/O teardown. Keeping the plist lookup out of
/// [`process_is_datagram_network`] avoids adding a linear scan to the hot
/// datagram read path.
fn process_has_datagram_semantics(proc: &Process) -> bool {
    process_is_datagram_network(proc)
        || (proc.kind == ProcessKind::Network
            && process_contact_plist_get(proc.childp, ProcessKeyword::Type.value())
                == Value::symbol("datagram"))
}

fn process_type_value(kind: &ProcessKind) -> Value {
    Value::symbol(kind.name())
}

fn make_process_command_lisp_value(
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

fn process_command_lisp_argv(command: Value) -> Option<Vec<LispString>> {
    let items = list_to_vec(&command)?;
    items
        .iter()
        .map(|value| value.as_lisp_string().cloned())
        .collect::<Option<Vec<_>>>()
}

fn process_spawn_lisp_argv(proc: &Process) -> Option<Vec<LispString>> {
    let mut argv = process_command_lisp_argv(proc.command)?;
    if let (Some(executable), Some(program)) = (&proc.executable, argv.first_mut()) {
        *program = executable.clone();
    }
    Some(argv)
}

fn lisp_bytes_to_os_string(bytes: &[u8], _multibyte: bool) -> OsString {
    // Issue #131: on Unix the OS path is the string's bytes verbatim — for a
    // unibyte string those are the raw bytes, for a multibyte string they are the
    // Emacs internal encoding (valid UTF-8 for ordinary text), matching the
    // byte-faithful boundary in `fileio::lisp_file_name_to_path_buf`. A raw
    // eight-bit byte therefore reaches the kernel as itself rather than as an
    // in-Unicode storage sentinel.
    #[cfg(unix)]
    {
        OsString::from_vec(bytes.to_vec())
    }

    #[cfg(not(unix))]
    {
        OsString::from(crate::emacs_core::emacs_char::to_utf8_lossy(bytes))
    }
}

fn lisp_string_to_os_string(string: &LispString) -> OsString {
    lisp_bytes_to_os_string(string.as_bytes(), string.is_multibyte())
}

fn visible_default_directory_lisp(eval: &super::eval::Context) -> Option<LispString> {
    let visible = eval.visible_variable_value_or_nil("default-directory");
    if let Some(string) = visible.as_lisp_string() {
        return Some(string.clone());
    }
    super::fileio::default_directory_lisp_in_state(&eval.obarray, &[], &eval.buffers)
}

fn os_str_to_lisp_string(value: &OsStr) -> LispString {
    #[cfg(unix)]
    {
        LispString::from_unibyte(value.as_bytes().to_vec())
    }

    #[cfg(not(unix))]
    {
        LispString::from_utf8(value.to_string_lossy().as_ref())
    }
}

fn process_coding_symbol_name(value: Value) -> &'static str {
    match value.as_symbol_name() {
        Some(name) => name,
        None => "utf-8-unix",
    }
}

/// The codings that convert NOTHING -- neither the character code nor the end
/// of line.
///
/// `raw-text` is deliberately absent.  It drops character conversion like
/// `binary` does, but its eol_type is a VECTOR, so GNU's `decode_eol` DETECTS
/// the child's line endings for it (src/coding.c:6783-6806); measured under GNU
/// 31.0.90, a child writing `a\r\nb\r\n` read with `coding-system-for-read`
/// bound to `raw-text` lands as `(97 10 98 10)` while the same child read as
/// `binary` lands as `(97 13 10 98 13 10)`.  `binary` and `no-conversion` are
/// the only two shipped codings whose eol_type is `Qunix` without their name
/// saying so, which is exactly what makes them -- and only them -- the
/// convert-nothing case.  DIVERGENCES.md entry 131 recorded this conflation in
/// advance; entry 134 removed it.
/// It is keyed on the NAME rather than on the slot `Value` because the coding
/// system reaching it is as often one `detect_coding` produced -- GNU answers
/// `Qno_conversion` for a source with a null byte in it (src/coding.c:6688) --
/// as one the creation-time chain resolved.
fn process_coding_name_converts_nothing(name: &str) -> bool {
    matches!(name, "binary" | "no-conversion")
}

/// The ENCODE-side twin, where `raw-text` DOES belong.
///
/// The two axes are not symmetric: encoders never detect.  `consume_chars`
/// (src/coding.c:7623-7625) resolves a VECTOR eol_type to `Qunix` before any
/// encoder sees a character, so `raw-text`'s undecided end-of-line writes bare
/// LF -- which is what "convert nothing" means on this side.
fn process_encode_coding_converts_nothing(coding: Value) -> bool {
    coding.is_nil()
        || matches!(
            coding.as_symbol_name(),
            Some("binary" | "no-conversion" | "raw-text")
        )
}

/// The decode half of a subprocess's coding system, reduced to the single
/// decision the "bytes become buffer text" step actually needs.
///
/// GNU resolves this once per subprocess — `setup_coding_system (val,
/// &process_coding)` in `Fcall_process` (src/callproc.c:760) for a synchronous
/// child, `setup_process_coding_systems` (src/process.c:2573) for an
/// asynchronous one — and every byte the child writes afterwards goes through
/// that one `struct coding_system`.  Making the choice a value that the
/// insertion path *takes as an argument* is what keeps a decoder from being
/// invented at the point of insertion: there is no way to write subprocess
/// output into a buffer without first naming, in the type, how it is decoded.
///
/// (The hard-coded `utf-8-unix` that this type replaced made `call-process`
/// ignore `coding-system-for-read` entirely; see DIVERGENCES.md entry 128.)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProcessOutputDecoding {
    /// `binary` / `no-conversion`: the child's bytes reach the buffer
    /// unchanged (raw bytes become eight-bit characters in a multibyte
    /// buffer).  Both halves of the coding system are nil here -- the character
    /// code AND the end of line -- which is what separates these two from
    /// `raw-text`, whose eol_type is undecided and therefore detects.
    ///
    /// The name is carried because the decode still REPORTS: GNU sets
    /// `Vlast_coding_system_used` from `CODING_ID_NAME (coding->id)` after every
    /// run of process output (src/process.c:6421), `binary` included.
    Bytes(&'static str),
    /// Decode under this coding system, which is fully specified: GNU's
    /// `setup_coding_system` left `CODING_REQUIRE_DETECTION` clear, so the
    /// decoder is chosen and `coding->id` will not move under it.
    Coding(&'static str),
    /// `CODING_REQUIRE_DETECTION` (src/coding.h:553): the name here is a
    /// REQUEST, not an answer.
    ///
    /// `decode_coding_object` calls `detect_coding` before any decoder runs
    /// (src/coding.c:8129-8130), and `detect_coding` REPLACES the whole coding
    /// system with the one it found -- `setup_coding_system (found, coding)`,
    /// :6751.  So the bytes are not decoded under this name and must not be
    /// reported under it either: measured under GNU 31.0.90, a subprocess whose
    /// chain answers nil and whose child writes `caf <c3> <a9> CR LF x CR LF`
    /// ends with `(process-coding-system P)` = `(utf-8-dos . utf-8-dos)`, never
    /// `(undecided-dos . undecided-dos)`.
    ///
    /// The variant exists so that name cannot reach a decoder: the only way out
    /// of it is [`ProcessOutputDecoding::detected`], which needs the bytes and
    /// the `CodingSystemManager` and answers a [`ResolvedProcessDecoding`],
    /// which is the only thing that has a `decode`.
    Detect(&'static str),
}

impl ProcessOutputDecoding {
    /// Reduce a resolved decode coding-system value the way GNU's
    /// `setup_coding_system` (src/coding.c:5668-5676) does.
    ///
    /// Note the nil case: GNU rewrites a nil coding system to `undecided`,
    /// i.e. DETECT the coding — it does NOT mean "copy the bytes".  Measured,
    /// `(let ((default-process-coding-system nil)) (call-process "printf" nil t
    /// nil "caf\\303\\251"))` leaves GNU with four characters, not five.
    pub(crate) fn for_coding(coding: Value) -> Self {
        if coding.is_nil() {
            return Self::for_name("undecided");
        }
        Self::for_name(process_coding_symbol_name(coding))
    }

    /// The three states `setup_coding_system` can leave a `struct coding_system`
    /// in, as a function of the coding system's NAME.
    ///
    /// This is the one place the "does it still detect?" question is answered,
    /// which is what keeps the answer from drifting between the slot a process
    /// was created with and the slot the write-back later replaced it with:
    /// both go through here.
    pub(crate) fn for_name(name: &'static str) -> Self {
        if process_coding_name_converts_nothing(name) {
            Self::Bytes(name)
        } else if crate::encoding::coding_name_requires_detection(name) {
            Self::Detect(name)
        } else {
            Self::Coding(name)
        }
    }

    /// GNU's `detect_coding` (src/coding.c:6503-6759) as the step that turns a
    /// decoding into one that can actually decode.
    ///
    /// It runs BEFORE the decoder in GNU and it has to run before it here too,
    /// for a reason beyond the reported name: the coding system it picks is the
    /// one whose decoder decides how many trailing bytes are incomplete, so a
    /// chunk ending mid-sequence is held back only if the DETECTED coding would
    /// hold it back.  A `utf-8` answer keeps a truncated multibyte tail; an
    /// `iso-latin-1` answer -- GNU's answer for bytes that are not valid UTF-8
    /// -- keeps nothing, because every byte is a character.
    ///
    /// BLOCK is why those two do not collapse into a chicken and egg.  It is
    /// GNU's `CODING_MODE_LAST_BLOCK`, and with it the detector can tell a
    /// truncated tail from a malformed one without knowing the answer first
    /// (src/coding.c:1215).
    pub(crate) fn detected(
        self,
        coding_systems: &crate::emacs_core::coding::CodingSystemManager,
        bytes: &[u8],
        block: crate::emacs_core::coding::SourceBlock,
    ) -> ResolvedProcessDecoding {
        match self {
            Self::Bytes(name) => ResolvedProcessDecoding::Bytes(name),
            Self::Coding(name) => ResolvedProcessDecoding::Coding(name),
            Self::Detect(requested) => {
                // A nil `found` leaves the coding system exactly as it was,
                // decoder included -- and it will be offered the NEXT chunk
                // again, because the name it keeps still requires detection.
                let found =
                    crate::encoding::detected_coding_name(coding_systems, requested, bytes, block)
                        .unwrap_or(requested);
                if process_coding_name_converts_nothing(found) {
                    ResolvedProcessDecoding::Bytes(found)
                } else {
                    ResolvedProcessDecoding::Coding(found)
                }
            }
        }
    }

    /// The same decoding with character-code conversion removed but the
    /// end-of-line conversion kept — GNU's `raw_text_coding_system` (src/coding.c),
    /// which returns the `raw-text` subsidiary carrying CODING's own EOL type.
    ///
    /// `Fcall_process` applies this when the destination buffer is unibyte
    /// (src/callproc.c:754-759).  Measured into a unibyte buffer: a child
    /// writing CR LF under `utf-8-dos` still lands as bare LF, while the same
    /// child under `utf-8-unix` keeps its CR — so dropping the EOL half here
    /// would be as wrong as keeping the character half.
    pub(crate) fn without_character_conversion(self) -> Self {
        let coding = match self {
            // Already byte-faithful.
            Self::Bytes(_) => return self,
            Self::Coding(name) | Self::Detect(name) => name,
        };
        // Every answer below is a `raw-text` subsidiary, and `raw-text` does
        // NOT detect (GNU raises `CODING_REQUIRE_DETECTION` for the `undecided`
        // TYPE, not for an undecided end of line), so the downgrade closes the
        // detection question rather than carrying it: measured under GNU
        // 31.0.90, a subprocess with a unibyte buffer and a nil chain reports
        // `raw-text-dos` for `caf <c3> <a9> CR LF`, not `utf-8-dos`.
        Self::Coding(if coding == "dos" || coding.ends_with("-dos") {
            "raw-text-dos"
        } else if coding == "mac" || coding.ends_with("-mac") {
            "raw-text-mac"
        } else if coding == "unix" || coding.ends_with("-unix") {
            "raw-text-unix"
        } else {
            // No EOL type of its own: GNU's `raw-text`, whose EOL is undecided.
            "raw-text"
        })
    }

    /// GNU's `setup_process_coding_systems` (src/process.c:8380-8409): turn the
    /// coding system STORED on an asynchronous process into the decoder its
    /// bytes actually go through.
    ///
    /// This is the second of GNU's two stages, and it is the only part the five
    /// creation-time resolvers share.  `Fmake_process` says so in its own
    /// comment (src/process.c:1942-1944): "Here we don't setup the structure
    /// coding_system nor pay attention to unibyte mode.  They are done in
    /// create_process."
    pub(crate) fn for_process(decode_coding: Value, sink: ProcessOutputSink) -> Self {
        let decoding = Self::for_coding(decode_coding);
        match sink {
            ProcessOutputSink::DecodedText => decoding,
            // src/process.c:8398-8399, the same `raw_text_coding_system` call
            // `Fcall_process` makes inline at src/callproc.c:757-759.
            ProcessOutputSink::UnibyteProcessBuffer => decoding.without_character_conversion(),
        }
    }
}

/// A process decoding with nothing left to detect: GNU's `struct coding_system`
/// at the moment `detect_coding` returns and the decoder is about to run.
///
/// The type exists to make one thing unrepresentable: decoding, or reporting,
/// under a name that `detect_coding` would have replaced.  A
/// [`ProcessOutputDecoding`] has no `decode`; the only bridge is
/// [`ProcessOutputDecoding::detected`], which takes the bytes and the
/// `CodingSystemManager` -- GNU's `coding_categories` / `coding_priorities`
/// globals, which cannot be globals here (entry 143's reason: the obarray is
/// owned by a `Context` and the unit suite runs many `Context`s on parallel
/// threads), so the tables have to travel as a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResolvedProcessDecoding {
    /// `binary` / `no-conversion`: the child's bytes reach the buffer
    /// unchanged.  Reachable from `Detect` too -- GNU answers `Qno_conversion`
    /// for a source with a null byte in it (src/coding.c:6688).
    Bytes(&'static str),
    /// Decode under this coding system, which is now concrete.
    Coding(&'static str),
}

impl ResolvedProcessDecoding {
    /// The coding-system name the decoder will run with, BEFORE `decode_eol`
    /// gets a chance to adjust it.
    ///
    /// The read-boundary carryover rules are keyed on this and not on the name
    /// the process was configured with, because in GNU they are properties of
    /// the DECODER `detect_coding` selected.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Bytes(name) | Self::Coding(name) => name,
        }
    }

    /// Turn a run of child output bytes into the text that is inserted, and
    /// keep the coding system the decode ENDED on while doing it.
    ///
    /// GNU cannot lose that answer: the decode runs through the process's own
    /// `struct coding_system`, so `detect_coding`'s and
    /// `adjust_coding_eol_type`'s rewrites of `coding->id` ARE the record.
    /// Here each run is a separate call, so the answer has to be returned, and
    /// [`ProcessRunCoding`] is what the caller must then do something with.
    ///
    /// It takes a `&mut Context` because GNU's decoder does.  There is no
    /// restricted process decoder in GNU to mirror: `read_and_insert_process_output`
    /// (src/process.c:6502), the filter branch (:6562) and `Fcall_process`
    /// (src/callproc.c:856) all expand `decode_coding_c_string`, whose body is
    /// `decode_coding_object` (src/coding.h:750-755) -- the function
    /// `decode-coding-string` reaches -- and `decode_coding_object` evaluates
    /// the coding system's `:post-read-conversion` at :8180-8194.
    pub(crate) fn decode_in_context(
        self,
        ctx: &mut crate::emacs_core::eval::Context,
        bytes: &[u8],
        state: &mut crate::encoding::CodingDecoderState,
        block: crate::emacs_core::coding::SourceBlock,
    ) -> Result<ProcessDecodedRun, Flow> {
        match self {
            // `binary` and `no-conversion` have a CONCRETE `Qunix` eol type
            // (GNU gives them `:eol-type unix` outright), so `decode_eol`
            // returns immediately and never adjusts the name -- and they have
            // no decoder to share, because `setup_coding_system` gives them
            // `decode_coding_raw_text`, which is a copy.  GNU's own buffer
            // branch skips the conversion engine outright for this case
            // (`! CODING_MAY_REQUIRE_DECODING`, src/process.c:6478-6484).
            // Nothing is lost by not routing them through the engine: a coding
            // system with no character-code conversion cannot carry a
            // `:post-read-conversion` either, since GNU attaches those to the
            // coding system's attributes and `binary`/`no-conversion` are
            // defined without one (lisp/international/mule-conf.el).
            Self::Bytes(name) => Ok(ProcessDecodedRun {
                text: LispString::from_unibyte(bytes.to_vec()),
                coding: ProcessRunCoding { used: name },
                // `decode_coding_raw_text` copies its source, so every byte is
                // consumed and `coding->carryover_bytes` stays zero.
                carryover: Vec::new(),
            }),
            Self::Coding(name) => {
                let run =
                    crate::encoding::decode_process_run_in_context(ctx, bytes, name, state, block)?;
                Ok(ProcessDecodedRun {
                    text: run.text,
                    coding: ProcessRunCoding { used: run.used },
                    carryover: run.carryover,
                })
            }
        }
    }
}

/// One run of a subprocess's output as the READ leaves it: the bytes the
/// decoder is about to be given, the coding system `detect_coding` has already
/// settled on, and the tail the read boundary held back.
///
/// GNU has no such object because GNU decodes inside `read_process_output`
/// with `p` in hand.  Here the decoder is the evaluator's --
/// `decode_coding_object` evaluates `:post-read-conversion` Lisp
/// (src/coding.c:8180-8194) -- and the evaluator OWNS the `ProcessManager`
/// (`Context.processes`), so the read has to let go of the process before the
/// decode can run.  This type is that hand-off.
///
/// What it makes unrepresentable is "subprocess output decoded by some other
/// decoder": it carries no text and has no way to make any.  The only way out
/// of it is [`Context::read_process_output_recording_coding`], which decodes
/// through [`crate::encoding::decode_process_run_in_context`] and then runs the
/// write-back.
#[derive(Debug)]
pub(crate) struct PendingProcessRun {
    coding: ResolvedProcessDecoding,
    /// The previous read's carryover followed by this read's bytes, which is
    /// GNU's `chars` buffer after `nbytes += carryover` (src/process.c:6331).
    ///
    /// It is the WHOLE thing and not a prefix.  Where the last complete
    /// character ends is the decoder's answer -- `coding->consumed`,
    /// src/coding.c:7477 -- and the decoder has not run yet, so a read that
    /// split this buffer here would be guessing.  Entry 159 handed that guess
    /// over as a residual and it was a table of byte-length rules keyed on the
    /// coding system's NAME.
    bytes: Vec<u8>,
    /// GNU `coding->spec`, carried across the hand-off so an ISO-2022
    /// designation set by one read is still in force in the next; see
    /// [`ProcessCodingState::store_decoder`].
    decoder: crate::encoding::CodingDecoderState,
    /// GNU `coding->mode & CODING_MODE_LAST_BLOCK` as this read left it, which
    /// the decode needs as well as the detection did: with it set the tail no
    /// decoder could consume is flushed as eight-bit characters
    /// (src/coding.c:7434-7462) instead of becoming the next read's carryover.
    block: crate::emacs_core::coding::SourceBlock,
}

impl PendingProcessRun {
    /// The undecoded bytes, for the one caller that must not decode: a unit
    /// fixture with no `Context` in reach.
    pub(crate) fn undecoded_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// What one decoded run of process output has to be reported as.
///
/// GNU keeps this in the process's own `struct coding_system` and reads it back
/// out in `read_process_output_set_last_coding_system` (src/process.c:6417-6425),
/// which does TWO writes with it: `Vlast_coding_system_used`, and -- when the
/// decode ended up on a different coding system than it started from --
/// `p->decode_coding_system` itself.  The second write is what makes both a
/// detected character code and a detected end-of-line type sticky for the rest
/// of the process's life.
///
/// It is ONE name and not a name plus an adjustment, because in GNU it is one
/// field: `coding->id`, which `setup_coding_system (found, coding)`
/// (src/coding.c:6751) and `adjust_coding_eol_type` (:6805) both overwrite, and
/// which `CODING_ID_NAME` then reads back.  Keeping the two rewrites apart here
/// is what let the first one go unreported while the second one moved.
///
/// It is a separate value from the decoded text because the only way to be
/// sticky in a per-read decoder is to hand the answer back; see
/// [`Context::read_process_output_recording_coding`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ProcessRunCoding {
    /// GNU `CODING_ID_NAME (coding->id)` after the decode, already carrying the
    /// unibyte-buffer `raw_text_coding_system` downgrade
    /// (`ProcessOutputDecoding::for_process`), the detected character code and
    /// the resolved end-of-line type.
    used: &'static str,
}

/// The text one run of process output decoded to, together with
/// [`ProcessRunCoding`].
#[derive(Debug)]
pub(crate) struct ProcessDecodedRun {
    pub(crate) text: LispString,
    pub(crate) coding: ProcessRunCoding,
    /// GNU `coding->carryover`: what the DECODER could not consume.  It
    /// arrives with the text rather than with the read, because until the
    /// decoder has run nobody knows where the last complete character ended --
    /// see `crate::encoding::SourceConsumed`.
    pub(crate) carryover: Vec<u8>,
}

impl ProcessDecodedRun {
    /// The coding system the run is reported as having used.
    pub(crate) fn coding_used(&self) -> &'static str {
        self.coding.used
    }
}

/// Where an asynchronous process's bytes land, reduced to the single
/// distinction GNU's `setup_process_coding_systems` draws
/// (src/process.c:8395-8400).
///
/// GNU re-runs that function on every `set-process-buffer`
/// (src/process.c:1312), `set-process-filter` (:1404) and
/// `set-process-coding-system` (:8036), so the answer is a function of the
/// process's CURRENT buffer and filter, never of the ones it happened to be
/// created with -- measured: a process created against a multibyte buffer and
/// then handed a unibyte one by `set-process-buffer` decodes as `raw-text-dos`.
/// Deriving it at read time, as this type does, is that same function with no
/// cache left to invalidate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessOutputSink {
    /// The internal default filter is inserting into a live UNIBYTE buffer, so
    /// character-code conversion is dropped and only the EOL conversion of the
    /// resolved coding survives.
    UnibyteProcessBuffer,
    /// Everything else: a multibyte process buffer, no process buffer at all,
    /// or a Lisp filter -- a filter is handed a decoded string, so GNU leaves
    /// the coding system alone for it (`EQ (p->filter,
    /// Qinternal_default_process_filter)`, src/process.c:8395).
    DecodedText,
}

impl ProcessOutputSink {
    fn of(proc: &Process, buffers: &BufferManager) -> Self {
        if !matches!(
            ProcessFilterDispatch::from_lisp(proc.filter),
            ProcessFilterDispatch::Default
        ) {
            return Self::DecodedText;
        }
        let Some(buffer_id) = proc.buffer.as_buffer_id() else {
            return Self::DecodedText;
        };
        match buffers.get(buffer_id) {
            Some(buffer) if !buffer.get_multibyte() => Self::UnibyteProcessBuffer,
            _ => Self::DecodedText,
        }
    }
}

/// What `read_and_dispose_of_process_output` does with one read
/// (src/process.c:6518-6585).
///
/// GNU writes two branches (:6557-6559) and then splits the first one again
/// with an early return, so there are three outcomes and this has three
/// variants.  It is a DIFFERENT question from [`ProcessOutputSink`], and the
/// two are answered from the same two facts in two different C functions,
/// which is why they are two types: the sink says what a decode PRODUCES
/// (`setup_process_coding_systems`, :8380-8400, which does not consult
/// `fast-read-process-output' at all), this says whether the decode happens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessReadBranch {
    /// `read_and_insert_process_output` (src/process.c:6459-6460) reached with a
    /// LIVE buffer to insert into, which is where `fast-read-process-output'
    /// and the default filter send a read (:6557-6558).
    InsertIntoBuffer,
    /// The same function reached with no live buffer, where its first
    /// statement returns before `decode_coding_c_string` (:6502) and before
    /// `read_process_output_set_last_coding_system` (:6506):
    ///
    /// ```c
    ///   if (!nread || NILP (p->buffer) || !BUFFER_LIVE_P (XBUFFER (p->buffer)))
    ///     return;
    /// ```
    ///
    /// (:6464-6465.)  The bytes were read -- `read_process_output` still
    /// returns `nbytes` and its caller still counts it as activity (:6345,
    /// :6027) -- and are then dropped undecoded, so no `:post-read-conversion'
    /// runs, `last-coding-system-used' is not written and the process's own
    /// decode coding system is not made sticky.
    DiscardUndecoded,
    /// The filter branch (:6560-6575): `decode_coding_c_string` runs
    /// unconditionally -- zero bytes included -- and the filter is called only
    /// for a non-empty result (`SBYTES (text) > 0`, :6567).
    CallFilter,
}

impl ProcessReadBranch {
    /// GNU's choice of branch, taken from the process's CURRENT filter and
    /// buffer because GNU re-runs `setup_process_coding_systems` on every
    /// `set-process-buffer` (src/process.c:1312) and `set-process-filter`
    /// (:1404) and asks `p->buffer` again inside the read itself.
    fn of(proc: &Process, buffers: &BufferManager, fast_read_process_output: bool) -> Self {
        // `fast_read_process_output && EQ (p->filter,
        // Qinternal_default_process_filter)` (src/process.c:6557-6558): both
        // conjuncts, because a user who sets `fast-read-process-output' to nil
        // is asking for the filter branch even with the default filter.
        if !fast_read_process_output
            || !matches!(
                ProcessFilterDispatch::from_lisp(proc.filter),
                ProcessFilterDispatch::Default
            )
        {
            return Self::CallFilter;
        }
        // `NILP (p->buffer) || !BUFFER_LIVE_P (XBUFFER (p->buffer))`, the two
        // disjuncts of :6464 that are properties of the process rather than of
        // the read.  A process whose buffer was killed answers the second one,
        // which is why this cannot be settled once at `make-process` time.
        match proc.buffer.as_buffer_id().and_then(|id| buffers.get(id)) {
            Some(_) => Self::InsertIntoBuffer,
            None => Self::DiscardUndecoded,
        }
    }
}

/// Whether one read's bytes are converted at all -- GNU's
/// `read_and_insert_process_output` first statement, whole
/// (src/process.c:6464-6465).
///
/// It exists so that the three disjuncts of that one `if` are asked in one
/// place.  Two of them are properties of the process and are already spent by
/// [`ProcessReadBranch::of`]; the third, `!nread`, is a property of the read
/// and is only known here.  Splitting them across two call sites is how this
/// area drifted before: entry 166 closed `!nread` with an inline test next to
/// the read and left the other two unasked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProcessRunDisposition {
    /// `decode_coding_c_string` runs (:6502 on the buffer branch, :6562 on the
    /// filter branch), and `read_process_output_set_last_coding_system` with
    /// it (:6506, :6565).
    Decode,
    /// GNU returns before either, and the bytes are dropped where they lie.
    Discard,
}

/// Everything a read has to know about where its output is going.
///
/// GNU derives both halves from the process's CURRENT filter and buffer, and
/// re-derives them on every `set-process-filter` / `set-process-buffer`; this
/// type is that derivation with no cache left to invalidate.  It is one
/// parameter rather than two because a read that had one and not the other
/// could decode into the wrong shape or skip a decode GNU makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessOutputDestination {
    sink: ProcessOutputSink,
    branch: ProcessReadBranch,
}

impl ProcessOutputDestination {
    /// GNU's two decisions for one read: `setup_process_coding_systems`
    /// (src/process.c:8380-8409) and `read_and_dispose_of_process_output`'s
    /// branch (:6557-6559).  They are taken together because they are taken
    /// from the same two facts, and separately because GNU asks them in two
    /// functions with two different rules -- the sink ignores
    /// `fast-read-process-output' and the branch does not, so a unibyte
    /// process buffer can reach the FILTER branch.
    fn of(proc: &Process, buffers: &BufferManager, fast_read_process_output: bool) -> Self {
        Self {
            sink: ProcessOutputSink::of(proc, buffers),
            branch: ProcessReadBranch::of(proc, buffers, fast_read_process_output),
        }
    }

    /// The destination a Lisp filter gives: GNU's filter branch, and a decoded
    /// (multibyte unless the coding system is `CODING_FOR_UNIBYTE`) string.
    pub(crate) fn to_filter() -> Self {
        Self {
            sink: ProcessOutputSink::DecodedText,
            branch: ProcessReadBranch::CallFilter,
        }
    }

    fn sink(self) -> ProcessOutputSink {
        self.sink
    }

    /// GNU's `if (!nread || NILP (p->buffer) || !BUFFER_LIVE_P (...)) return;`
    /// (src/process.c:6464-6465), with `nbytes` supplying the disjunct the
    /// branch could not know.  `nbytes` is GNU's `nread`, i.e. this read's
    /// bytes plus the carryover it was prepended to (`nbytes += carryover`,
    /// :6331) -- not the raw `emacs_read` return.
    fn disposition_for(self, nbytes: usize) -> ProcessRunDisposition {
        match self.branch {
            // The filter branch has no such statement: it decodes zero bytes
            // as readily as a thousand (:6562), which is entry 166's last
            // block.
            ProcessReadBranch::CallFilter => ProcessRunDisposition::Decode,
            ProcessReadBranch::DiscardUndecoded => ProcessRunDisposition::Discard,
            ProcessReadBranch::InsertIntoBuffer if nbytes == 0 => ProcessRunDisposition::Discard,
            ProcessReadBranch::InsertIntoBuffer => ProcessRunDisposition::Decode,
        }
    }
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
enum LastBlock {
    /// No read has returned zero bytes yet.
    #[default]
    NotReached,
    /// A read returned zero bytes and raised the flag.  Every later zero-byte
    /// read returns immediately.
    Reached,
}

/// What a zero-byte read found the latch in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LastBlockArrival {
    /// This read raised the flag, so it falls THROUGH to the decode.
    JustRaised,
    /// The flag was already up: `return nbytes` with nothing decoded.
    AlreadyRaised,
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
    carryover: Vec<u8>,
    last_block: LastBlock,
    decoder: crate::encoding::CodingDecoderState,
}

impl ProcessCodingState {
    /// GNU's `p->decoding_carryover` as the next read sees it.
    fn carryover_len(&self) -> usize {
        self.carryover.len()
    }

    /// GNU's `p->decoding_carryover = 0` (src/process.c:6312), which happens
    /// before the read decides anything: the tail is MOVED into the run that
    /// is about to be decoded, so a `:post-read-conversion` that re-enters this
    /// process through `accept-process-output` finds it at zero.
    fn take_carryover(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.carryover)
    }

    /// `read_process_output_set_last_coding_system`'s half of the write-back
    /// (src/process.c:6448-6457), which runs AFTER the decode.
    fn store_carryover(&mut self, carryover: Vec<u8>) {
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
    fn store_decoder(&mut self, decoder: crate::encoding::CodingDecoderState) {
        self.decoder = decoder;
    }

    /// The decoder state a run starts from.
    fn decoder(&self) -> crate::encoding::CodingDecoderState {
        self.decoder.clone()
    }

    /// The three lines at src/process.c:6315-6321, as one answer.
    fn reach_last_block(&mut self) -> LastBlockArrival {
        match self.last_block {
            LastBlock::Reached => LastBlockArrival::AlreadyRaised,
            LastBlock::NotReached => {
                self.last_block = LastBlock::Reached;
                LastBlockArrival::JustRaised
            }
        }
    }

    /// A process whose descriptor is being replaced starts over, the way a
    /// fresh `setup_coding_system` would.
    fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Encode the data passed to `process-send-string`/`process-send-region`
/// through a process's ENCODE coding system, mirroring GNU `send_process`
/// (src/process.c).  A `binary`/`raw-text`/`no-conversion`/nil encode coding
/// (or an unset one) leaves the bytes untouched; every other coding goes through
/// the shared string encoder, which performs character-code conversion and the
/// EOL conversion the coding's eol_type requests.
fn encode_process_send_input(
    processes: &ProcessManager,
    id: ProcessId,
    input: &LispString,
    eol_conversion: crate::emacs_core::coding::EolConversion,
) -> LispString {
    let coding = processes
        .get_any(id)
        .map(|proc| proc.coding_encode)
        .unwrap_or(Value::NIL);
    if process_encode_coding_converts_nothing(coding) {
        return input.clone();
    }
    let bytes = crate::encoding::encode_lisp_string(
        input,
        process_coding_symbol_name(coding),
        eol_conversion,
    );
    LispString::from_unibyte(bytes)
}

/// Detect, and choose the read boundary for, one run of an asynchronous
/// process's output -- everything GNU does inside `read_process_output` that
/// does NOT need the evaluator.
///
/// It stops one step short of GNU, and the step it stops short of is the decode
/// itself: that runs Lisp (`:post-read-conversion`, src/coding.c:8180-8194) and
/// so cannot happen while the `ProcessManager` is borrowed out of the `Context`
/// that owns it.  What comes back is a [`PendingProcessRun`], which has no text
/// and no way to make any.
///
/// SINK is a required parameter, not something this function may work out for
/// itself: GNU decides it in `setup_process_coding_systems` against the
/// process's live buffer and filter, and the answer changes under
/// `set-process-buffer` / `set-process-filter`.  Naming it at the call site is
/// what keeps the two stages of GNU's decision from collapsing into one
/// invented rule here -- the previous code read `proc.coding_decode` and
/// classified it inline, which is how `nil` came to mean "copy the bytes"
/// (GNU's `setup_coding_system` rewrites nil to `undecided`, i.e. DETECT,
/// src/coding.c:5675-5676) and how the unibyte rule went missing entirely.
fn pending_process_output_run(
    proc: &mut Process,
    coding_systems: &crate::emacs_core::coding::CodingSystemManager,
    sink: ProcessOutputSink,
    bytes: &[u8],
    block: crate::emacs_core::coding::SourceBlock,
) -> PendingProcessRun {
    let decoding = ProcessOutputDecoding::for_process(proc.coding_decode, sink);
    // GNU's `p->decoding_carryover = 0` (src/process.c:6312) followed by
    // `memcpy (chars, SDATA (p->decoding_buf), carryover)` (:6255) and
    // `nbytes += carryover` (:6331).  Taking the tail rather than copying it is
    // what makes the clear and the prepend one act: a
    // `:post-read-conversion` may call `accept-process-output` on this very
    // process, and GNU's re-entrant read finds the carryover at zero.
    let mut combined = proc.coding_state.take_carryover();
    combined.reserve(bytes.len());
    combined.extend_from_slice(bytes);
    if let ProcessOutputDecoding::Bytes(name) = decoding {
        return PendingProcessRun {
            coding: ResolvedProcessDecoding::Bytes(name),
            bytes: combined,
            decoder: crate::encoding::CodingDecoderState::default(),
            block,
        };
    }

    // Detection sees the WHOLE buffer that is about to be decoded, carryover
    // included.  Both halves are GNU's: `coding->src_bytes` is the carryover
    // plus this read (src/process.c:6243-6254, `nbytes += carryover` at :6331),
    // and `detect_coding` runs before the decoder that reports
    // `coding->consumed` (src/coding.c:8129-8130).
    //
    // BLOCK is GNU's `CODING_MODE_LAST_BLOCK`, which `read_process_output`
    // raises only at EOF (src/process.c:6321).  Detection needs it to tell
    // "these bytes are not UTF-8" from "this chunk stopped in the middle of a
    // character" (src/coding.c:1215), and the decode needs it to decide what to
    // do with the tail -- so the one flag is spent twice here exactly as GNU
    // spends its one flag twice.
    let resolved = decoding.detected(coding_systems, &combined, block);
    PendingProcessRun {
        coding: resolved,
        bytes: combined,
        decoder: proc.coding_state.decoder(),
        block,
    }
}

fn process_read_buffer_len(proc: &Process) -> usize {
    proc.readmax.clamp(1, READ_PROCESS_OUTPUT_MAX_CEILING)
}

fn update_process_adaptive_read_buffering(proc: &mut Process, nbytes: usize, full_read: bool) {
    if nbytes == 0 || proc.adaptive_read_buffering == 0 {
        return;
    }

    let mut delay_ms = proc.read_output_delay.as_millis().min(u64::MAX as u128) as u64;
    if nbytes < 256 {
        delay_ms =
            (delay_ms + 2 * READ_OUTPUT_DELAY_INCREMENT_MS).min(READ_OUTPUT_DELAY_MAX_MAX_MS);
    } else if delay_ms > 0 && full_read {
        delay_ms = delay_ms.saturating_sub(READ_OUTPUT_DELAY_INCREMENT_MS);
    }

    proc.read_output_delay = Duration::from_millis(delay_ms);
    proc.read_output_skip = delay_ms > 0;
}

fn reset_adaptive_read_delay_after_process_write(proc: &mut Process) {
    if proc.read_output_delay > Duration::ZERO && proc.adaptive_read_buffering == 1 {
        proc.read_output_delay = Duration::ZERO;
        proc.read_output_skip = false;
    }
}

/// GNU's `read_process_output` from the `emacs_read` call down to the
/// `read_and_dispose_of_process_output` hand-off (src/process.c:6281-6339),
/// which is three decisions and not one:
///
/// ```c
///   p->decoding_carryover = 0;
///   if (nbytes <= 0)
///     {
///       if (nbytes < 0 || coding->mode & CODING_MODE_LAST_BLOCK)
///         { SAFE_FREE_UNBIND_TO (count, Qnil); return nbytes; }
///       coding->mode |= CODING_MODE_LAST_BLOCK;
///     }
///   ...
///   nbytes += carryover;
///   read_and_dispose_of_process_output (p, chars, nbytes, coding);
///   ...
///   return nbytes;
/// ```
///
/// A read ERROR returns without raising the flag -- which is not a corner
/// case: when the child on the far end of a PTY exits, Linux answers the
/// master with `EIO` rather than with a zero-byte read, so a pty process never
/// has a last block at all.  A zero-byte read on a PIPE does raise it, and
/// then falls THROUGH to a decode of `0 + carryover` bytes.  When that total is
/// zero the decode still happens (on the filter branch) and the function still
/// returns 0, which is what its caller reads as end of file -- one read, both
/// facts, which is why [`ProcessBytesRead::EofAfterLastBlock`] is one variant.
/// What one `emacs_read` answered, in the three cases GNU's
/// `if (nbytes < 0 || ...)` distinguishes (src/process.c:6315).
///
/// `std::io::Result<usize>` is NOT that type, and the difference is not
/// academic.  `portable_pty` deliberately rewrites a pty master's `EIO` --
/// which is how Linux reports that the slave side is gone -- into `Ok(0)`:
///
/// ```text
///   Err(ref e) if e.raw_os_error() == Some(libc::EIO) => {
///       // EIO indicates that the slave pty has been closed.
///       // Treat this as EOF so that std::io::Read::read_to_string
///       // and similar functions gracefully terminate ...
///       Ok(0)
///   }
/// ```
///
/// (portable-pty-0.9.0/src/unix.rs:93-103.)  That rewrite erases exactly the
/// bit `CODING_MODE_LAST_BLOCK` turns on, so a source has to say which of the
/// three it means rather than hand over an `io::Result` and let the coding
/// layer guess.  Measured under GNU Emacs 31.0.90: a `:connection-type 'pty`
/// process runs its `:post-read-conversion` once per chunk, a
/// `:connection-type 'pipe` process runs it once more.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProcessReadOutcome {
    /// `nbytes > 0`.
    Bytes(usize),
    /// `nbytes == 0` on a source that can have one: a pipe, a socket, a serial
    /// device.  GNU raises `CODING_MODE_LAST_BLOCK` for it and falls through
    /// to a decode of zero bytes.
    EndOfStream,
    /// A PTY slave closure.  `portable_pty` reports the platform's `EIO` as
    /// `Ok(0)`, but GNU sees the original read error: it does not raise the
    /// decoder's last-block flag, while the wait loop still treats the source
    /// as closed rather than as a broken pipe connection.
    PtyClosed,
    /// GNU's `nbytes < 0`: `read_process_output` returns without raising the
    /// flag and without decoding anything (src/process.c:6315-6318).
    Failed,
    /// `EWOULDBLOCK`, which GNU's caller passes over (:6045).
    WouldBlock,
}

impl ProcessReadOutcome {
    /// A source whose end of file really is a zero-byte read: a pipe, a
    /// socket, a serial device.
    fn from_stream_read(result: &std::io::Result<usize>) -> Self {
        match result {
            Ok(0) => Self::EndOfStream,
            Ok(n) => Self::Bytes(*n),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Self::WouldBlock,
            Err(_) => Self::Failed,
        }
    }

    /// A pty master, where `portable_pty` has already spent the `EIO` this
    /// port needs.  Its `Ok(0)` is GNU's `nbytes < 0`, so there is no last
    /// block on a pty.
    fn from_pty_read(result: &std::io::Result<usize>) -> Self {
        match Self::from_stream_read(result) {
            Self::EndOfStream => Self::PtyClosed,
            other => other,
        }
    }
}

fn process_output_read_from_io_result(
    proc: &mut Process,
    coding_systems: &crate::emacs_core::coding::CodingSystemManager,
    destination: ProcessOutputDestination,
    outcome: ProcessReadOutcome,
    bytes: &[u8],
    full_read_len: usize,
) -> ProcessBytesRead {
    match outcome {
        ProcessReadOutcome::EndOfStream => match proc.coding_state.reach_last_block() {
            LastBlockArrival::AlreadyRaised => ProcessBytesRead::Eof,
            LastBlockArrival::JustRaised => {
                // `nbytes += carryover` (:6331).  GNU returns that total, so a
                // non-empty carryover is still a READ as far as the caller is
                // concerned, and only an empty one is the end of file.
                let bytes_read = proc.coding_state.carryover_len();
                match destination.disposition_for(bytes_read) {
                    ProcessRunDisposition::Discard => {
                        discard_process_run_undecoded(proc);
                        if bytes_read == 0 {
                            ProcessBytesRead::Eof
                        } else {
                            ProcessBytesRead::Discarded { bytes_read }
                        }
                    }
                    ProcessRunDisposition::Decode => {
                        let run = pending_process_output_run(
                            proc,
                            coding_systems,
                            destination.sink(),
                            &[],
                            crate::emacs_core::coding::SourceBlock::Last,
                        );
                        if bytes_read > 0 {
                            ProcessBytesRead::Data { run, bytes_read }
                        } else {
                            ProcessBytesRead::EofAfterLastBlock { run }
                        }
                    }
                }
            }
        },
        ProcessReadOutcome::Bytes(n) => {
            update_process_adaptive_read_buffering(proc, n, n == full_read_len);
            process_run_from_bytes(proc, coding_systems, destination, &bytes[..n])
        }
        ProcessReadOutcome::WouldBlock => ProcessBytesRead::WouldBlock,
        ProcessReadOutcome::PtyClosed => ProcessBytesRead::Eof,
        ProcessReadOutcome::Failed => ProcessBytesRead::Failed,
    }
}

/// The one thing GNU still does to the process for a read it will not decode.
///
/// `p->decoding_carryover = 0` is at src/process.c:6312, ABOVE both the
/// zero-byte test and the branch, so it is spent on every read that reaches
/// `read_and_dispose_of_process_output` -- and the only place a new carryover
/// is written is `read_process_output_set_last_coding_system` (:6449-6455),
/// which a discarded read never reaches.  So a process whose buffer is killed
/// mid-stream loses the tail its last decode held back, exactly as GNU does.
///
/// Nothing else on the process is touched: `coding->spec` is the decoder's and
/// no decoder ran, and `CODING_MODE_LAST_BLOCK` was already raised at :6321,
/// above the branch, by [`ProcessCodingState::reach_last_block`].
fn discard_process_run_undecoded(proc: &mut Process) {
    drop(proc.coding_state.take_carryover());
}

/// One non-final read's bytes, routed through GNU's
/// `read_and_dispose_of_process_output` (src/process.c:6518-6585).
///
/// Every door into that function goes through here: the `io::Result` one and
/// the datagram one, which has to record the sender's address before it can
/// hand the bytes on.  Sharing it is the point -- a second place that built a
/// [`PendingProcessRun`] itself would be a second place deciding whether a run
/// is decoded, and that decision has drifted every time this area has grown
/// one.
fn process_run_from_bytes(
    proc: &mut Process,
    coding_systems: &crate::emacs_core::coding::CodingSystemManager,
    destination: ProcessOutputDestination,
    bytes: &[u8],
) -> ProcessBytesRead {
    let bytes_read = bytes.len();
    // GNU's `nread`: this read plus the carryover it is prepended to
    // (`nbytes += carryover`, :6331).
    let nread = bytes_read + proc.coding_state.carryover_len();
    match destination.disposition_for(nread) {
        ProcessRunDisposition::Discard => {
            discard_process_run_undecoded(proc);
            ProcessBytesRead::Discarded { bytes_read }
        }
        ProcessRunDisposition::Decode => ProcessBytesRead::Data {
            run: pending_process_output_run(
                proc,
                coding_systems,
                destination.sink(),
                bytes,
                crate::emacs_core::coding::SourceBlock::More,
            ),
            bytes_read,
        },
    }
}

fn env_var_name_bytes_eq(left: &[u8], right: &[u8]) -> bool {
    #[cfg(windows)]
    {
        left.eq_ignore_ascii_case(right)
    }

    #[cfg(not(windows))]
    {
        left == right
    }
}

fn update_process_mark(buffers: &mut BufferManager, proc: &mut Process) -> EvalResult {
    let Some(buffer_id) = proc.buffer.as_buffer_id() else {
        return super::marker::builtin_set_marker_in_buffers(buffers, vec![proc.mark, Value::NIL]);
    };
    let Some(buffer) = buffers.get(buffer_id) else {
        return super::marker::builtin_set_marker_in_buffers(buffers, vec![proc.mark, Value::NIL]);
    };
    let position = Value::fixnum(buffer.z_lisp_char_pos().as_i64());
    super::marker::builtin_set_marker_in_buffers(buffers, vec![proc.mark, position, proc.buffer])
}

fn process_status_run_value() -> Value {
    Value::symbol("run")
}

fn process_status_connect_value() -> Value {
    Value::symbol("connect")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProcessWriteFlush {
    Drained,
    Blocked,
    NoSource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProcessWriteAttempt {
    Written(usize),
    WouldBlock,
    NoSource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProcessWriteInterest {
    Readable,
    ReadableAndWritable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ProcessWriteQueueEntry {
    object: Value,
    offset: usize,
    len: usize,
}

impl ProcessWriteQueueEntry {
    fn bytes(self) -> Option<Vec<u8>> {
        let string = self.object.as_lisp_string()?;
        let bytes = string.as_bytes();
        let start = self.offset.min(bytes.len());
        let end = start.saturating_add(self.len).min(bytes.len());
        Some(bytes[start..end].to_vec())
    }

    fn advance(self, written: usize) -> Self {
        let written = written.min(self.len);
        Self {
            object: self.object,
            offset: self.offset.saturating_add(written),
            len: self.len.saturating_sub(written),
        }
    }
}

fn write_queue_push_entry(queue: Value, entry: ProcessWriteQueueEntry, front: bool) -> Value {
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

fn write_queue_push(queue: Value, input_obj: Value, front: bool) -> Value {
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

fn write_queue_pop(queue: Value) -> (Value, Option<ProcessWriteQueueEntry>) {
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

fn parse_make_network_tls_parameters(
    value: Value,
) -> Result<Option<super::tls::GnutlsBootParameters>, Flow> {
    if value.is_nil() {
        return Ok(None);
    }
    let items = list_to_vec(&value).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), value],
        )
    })?;
    let Some((&credential_type, rest)) = items.split_first() else {
        return Ok(None);
    };
    parse_gnutls_boot_parameters(credential_type, Value::list(rest.to_vec())).map(Some)
}

fn process_status_stop_value(signal_num: i64) -> Value {
    Value::list(vec![Value::symbol("stop"), Value::fixnum(signal_num)])
}

fn process_status_exit_value(code: i32) -> Value {
    Value::list(vec![Value::symbol("exit"), Value::fixnum(code as i64)])
}

fn process_status_failed_value(code: i32) -> Value {
    Value::list(vec![Value::symbol("failed"), Value::fixnum(code as i64)])
}

fn process_status_failed_message_value(message: String) -> Value {
    Value::list(vec![Value::symbol("failed"), Value::string(message)])
}

/// Convert a finished `std::process::ExitStatus` to an Emacs process status:
/// `(exit CODE)` for a normal exit, `(signal N ...)` for signal death (GNU
/// distinguishes the two via `WIFSIGNALED`/`WTERMSIG`).
///
/// Unix reaps through `sys::poll_child_status`, which decodes the raw status
/// word itself and carries `WUNTRACED | WCONTINUED` with it; this is the
/// non-Unix arm of `ReapableChild::probe`.
#[cfg(not(unix))]
fn process_status_from_exit(status: &std::process::ExitStatus) -> Value {
    if let Some(code) = status.code() {
        return process_status_exit_value(code);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(sig) = status.signal() {
            return process_status_signal_value_with_core(sig, status.core_dumped());
        }
    }
    process_status_exit_value(1)
}

/// Map a `sys::ChildWait` (the outcome of a non-blocking child-status probe, or
/// a decoded `waitpid` status) to an Emacs process-status value, or `None` when
/// there is no state change to report.
#[cfg(unix)]
/// Classify one `waitpid` answer the way GNU's `handle_child_signal` does:
/// nothing to record, a change that leaves `p->alive` set, or the
/// `WIFSIGNALED (status) || WIFEXITED (status)` that clears it
/// (src/process.c:7750-7752).
///
/// `NoChild` is `ECHILD` -- nobody has that child -- and it is REAPED rather
/// than "no change".  It used to be `None`, which meant a child some other
/// reaper had taken stayed `run` in this port forever, and meant the delete
/// path then signalled its pid; see `process/reap.rs`.
#[cfg(unix)]
fn process_child_status_change_from_wait(wait: sys::ChildWait) -> ChildStatusChange {
    match wait {
        sys::ChildWait::Running | sys::ChildWait::Undecoded => ChildStatusChange::NoChange,
        sys::ChildWait::NoChild => ChildStatusChange::Gone,
        sys::ChildWait::Exited(code) => ChildStatusChange::Reaped(process_status_exit_value(code)),
        sys::ChildWait::Signaled { sig, core } => {
            ChildStatusChange::Reaped(process_status_signal_value_with_core(sig, core))
        }
        sys::ChildWait::Stopped(sig) => {
            ChildStatusChange::StillOurs(process_status_stop_value(sig as i64))
        }
        sys::ChildWait::Continued => ChildStatusChange::StillOurs(process_status_run_value()),
        sys::ChildWait::Error => ChildStatusChange::Reaped(process_status_exit_value(1)),
    }
}

fn process_status_signal_value(signal_num: i32) -> Value {
    process_status_signal_value_with_core(signal_num, false)
}

fn process_status_signal_value_with_core(signal_num: i32, core_dumped: bool) -> Value {
    Value::list(vec![
        Value::symbol("signal"),
        Value::fixnum(signal_num as i64),
        if core_dumped { Value::T } else { Value::NIL },
    ])
}

/// Convert a finished `portable_pty::ExitStatus` (PTY child) to an Emacs process
/// status. GNU distinguishes signal death from a normal exit via
/// `WIFSIGNALED`/`WTERMSIG`; portable_pty preserves this as `signal()`/`exit_code()`.
#[cfg(unix)]
fn process_status_from_pty_exit(status: &portable_pty::ExitStatus) -> Value {
    if let Some(sig_name) = status.signal() {
        let signum = sys::signal_number_from_description(sig_name).unwrap_or(0);
        return process_status_signal_value(signum);
    }
    process_status_exit_value(status.exit_code() as i32)
}

#[cfg(not(unix))]
fn process_status_from_pty_exit(status: &portable_pty::ExitStatus) -> Value {
    if status.success() {
        process_status_exit_value(0)
    } else {
        process_status_exit_value(status.exit_code() as i32)
    }
}

/// GNU `status_message` (process.c): the human-readable sentinel/buffer message
/// for a finished process status. Signal/stop death reports the `strsignal`
/// description with its first character down-cased; a non-zero exit reports
/// "exited abnormally with code N"; a zero exit reports "finished".
fn gnu_process_status_message(status: Value) -> String {
    match ProcessStatusSymbol::from_status_value(status) {
        Some(ProcessStatusSymbol::Exit) => {
            let code = process_status_code_value(status);
            if code == 0 {
                "finished\n".to_string()
            } else {
                format!("exited abnormally with code {code}\n")
            }
        }
        Some(ProcessStatusSymbol::Failed) => {
            if let Some(code) = process_status_code_lisp_value(status) {
                format!("failed with code {}\n", process_status_code_message(code))
            } else {
                "failed with code 0\n".to_string()
            }
        }
        Some(ProcessStatusSymbol::Signal) | Some(ProcessStatusSymbol::Stop) => {
            let code = process_status_code_value(status);
            let desc = sys::signal_description(code as i32);
            let suffix = if process_status_core_dumped_value(status) {
                " (core dumped)\n"
            } else {
                "\n"
            };
            format!("{desc}{suffix}")
        }
        Some(symbol) => symbol.name().to_string(),
        None => "finished\n".to_string(),
    }
}

fn gnu_process_status_message_for_process(proc: &Process) -> String {
    gnu_process_status_message_for_status(proc, proc.status)
}

/// GNU `status_message (p)` for a status that is not (yet) in `p->status`.
///
/// Every GNU caller of `status_message` runs `update_status` on the line
/// above it (:1143/:1189/:6726/:7453/:7915), so the message is always built
/// from the SETTLED status.  This port settles at some of those sites and
/// only records at others, so the status travels as an argument.
fn gnu_process_status_message_for_status(proc: &Process, status: Value) -> String {
    if proc.kind == ProcessKind::Network
        && ProcessStatusSymbol::from_status_value(status) == Some(ProcessStatusSymbol::Exit)
    {
        return if process_status_code_value(status) == 0 {
            "deleted\n".to_string()
        } else {
            "connection broken by remote peer\n".to_string()
        };
    }
    gnu_process_status_message(status)
}

/// What became of the child `spawn_child_with_environment` tried to start.
///
/// GNU's parent never learns that the exec failed.  `child_setup` runs in the
/// forked child; when `execvp` fails it calls `exec_failed`
/// (src/callproc.c:1206-1216), which writes `emacs_perror`'s diagnostic to its
/// own STDERR and `_exit`s with `EXIT_ENOENT` (127) for `ENOENT` and
/// `EXIT_CANNOT_INVOKE` (126) otherwise (src/process.h:273-274).  A failed exec
/// is therefore a STATE of the process -- some output, then an exit status --
/// and not an error of the launcher.
///
/// Modelling it as `Err(String)` is what made the caller recover the exit code
/// by searching the message text for `"os error 2"`.  The errno travels here
/// instead, so the 127/126 split is GNU's comparison rather than a substring
/// match on a platform's phrasing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChildSpawnOutcome {
    /// The child exists and its fds are installed on the process record.
    Spawned,
    /// No child exists: `execvp` failed with this errno.
    ExecFailed(i32),
}

impl ChildSpawnOutcome {
    /// GNU's `exec_failed` exit code (src/callproc.c:1215).
    fn exec_failure_exit_code(errno: i32) -> i32 {
        if errno == libc::ENOENT { 127 } else { 126 }
    }
}

/// GNU's `initial_argv0`: the name Emacs was invoked as, which `emacs_perror`
/// prefixes to every diagnostic (src/sysdep.c:2870-2871, "emacs" when unset).
fn emacs_invocation_argv0() -> String {
    std::env::args_os()
        .next()
        .map(|arg| std::path::Path::new(&arg).display().to_string())
        .unwrap_or_else(|| "emacs".to_string())
}

/// Write the line GNU's failed child writes, to the file its stderr would
/// have been.
///
/// `exec_failed` reaches `emacs_perror` (src/sysdep.c:2867-2887), which writes
/// `"<argv0>: <program>: <strerror>\n"` to STDERR.  For a pty process that
/// STDERR is the pty slave, so the parent reads the line as ordinary process
/// output before the exit status arrives.  No child ever exists here --
/// `Command::spawn` reports the exec failure to the parent and reaps it -- so
/// the parent opens the slave under the same name the child would have had on
/// fd 2 and writes the same bytes.  `O_NOCTTY` keeps that open from claiming a
/// controlling terminal for the editor.
#[cfg(unix)]
fn write_exec_failure_diagnostic_to_tty(
    tty: &std::path::Path,
    program: &std::ffi::OsStr,
    errno: i32,
) {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let message = format!(
        "{}: {}: {}\n",
        emacs_invocation_argv0(),
        std::path::Path::new(program).display(),
        sys::errno_description(errno)
    );
    if let Ok(mut slave) = std::fs::OpenOptions::new()
        .write(true)
        .custom_flags(libc::O_NOCTTY)
        .open(tty)
    {
        let _ = slave.write_all(message.as_bytes());
        let _ = slave.flush();
    }
}

fn process_status_core_dumped_value(status: Value) -> bool {
    list_to_vec(&status)
        .and_then(|items| items.get(2).copied())
        .is_some_and(|value| value.is_truthy())
}

fn process_status_symbol_value(status: Value) -> Value {
    list_to_vec(&status)
        .and_then(|items| items.first().copied())
        .unwrap_or(status)
}

fn process_status_code_value(status: Value) -> i64 {
    process_status_code_lisp_value(status)
        .and_then(|value| value.as_fixnum())
        .unwrap_or(0)
}

fn process_status_code_lisp_value(status: Value) -> Option<Value> {
    list_to_vec(&status).and_then(|items| items.get(1).copied())
}

fn process_status_code_message(code: Value) -> String {
    code.as_fixnum()
        .map(|value| value.to_string())
        .or_else(|| code.as_utf8_str().map(str::to_string))
        .unwrap_or_else(|| "0".to_string())
}

/// The multibyteness of the two buffers GNU's connection-process coding
/// resolvers ask about.
///
/// Two fields rather than one, because GNU asks a DIFFERENT buffer for the two
/// halves of the pair.  Measured under GNU Emacs 31.0.90: `make-pipe-process`
/// with a unibyte CURRENT buffer and a multibyte process buffer answers
/// `(utf-8-unix . nil)` -- the encode half short-circuited and the decode half
/// did not.  A single "the buffer is unibyte" flag cannot express that row.
///
/// `Fmake_serial_process` asks the process buffer for both halves
/// (:3258-3260, :3272-3274), which is a third combination, but it cannot be
/// observed: serial's fallthrough is nil too.  That is why
/// `SerialProcessCodingEnvironment` has no short-circuit field and this type
/// never reaches it.
#[derive(Clone, Copy, Debug)]
struct ProcessBufferMultibyteness {
    /// The process's own buffer, or -- when it has none -- `buffer-defaults`,
    /// which is the fallback GNU spells out in every one of the three
    /// primitives (`NILP (buffer) && NILP (BVAR (&buffer_defaults, ...))`,
    /// src/process.c:2534, :3259, :3316-3317).
    process_buffer: bool,
    /// The buffer that was current when the primitive ran.
    current_buffer: bool,
}

/// Whether each half of a connection process's chain takes GNU's
/// unibyte-buffer short circuit -- the `val = Qnil` arm that skips the alist
/// and the process default outright.
///
/// GNU's comment says why the arm exists at all (src/process.c:2535-2538):
///
/// ```text
///   /* We dare not decode end-of-line format by setting VAL to
///      Qraw_text, because the existing Emacs Lisp libraries
///      assume that they receive bare code including a sequence of
///      CR LF.  */
/// ```
///
/// It is NOT `Fcall_process`'s unibyte rule with the opposite sign.  That one
/// downgrades to `raw_text_coding_system (val)` and belongs to the SECOND
/// stage, `setup_process_coding_systems` (src/process.c:8395-8399), which every
/// asynchronous process still runs -- see entry 131.  This one only decides
/// what the user-visible `process-coding-system` slot holds.
///
/// The constructors are the two primitives that can observe it, because
/// choosing which buffer answers for which half is the whole content of this
/// type.
#[derive(Clone, Copy, Debug)]
struct ConnectionProcessUnibyteShortCircuit {
    decode: bool,
    encode: bool,
}

impl ConnectionProcessUnibyteShortCircuit {
    /// `Fmake_pipe_process`: decode asks the process buffer
    /// (src/process.c:2533-2534), encode asks `current_buffer`
    /// (src/process.c:2559-2560).
    fn pipe(multibyte: ProcessBufferMultibyteness) -> Self {
        Self {
            decode: !multibyte.process_buffer,
            encode: !multibyte.current_buffer,
        }
    }

    /// `set_network_socket_coding_system`: decode asks the process buffer
    /// (src/process.c:3314-3317), encode asks `current_buffer`
    /// (src/process.c:3348-3349).
    fn network(multibyte: ProcessBufferMultibyteness) -> Self {
        Self {
            decode: !multibyte.process_buffer,
            encode: !multibyte.current_buffer,
        }
    }

    fn takes(self, half: ProcessCodingHalf) -> bool {
        match half {
            ProcessCodingHalf::Decode => self.decode,
            ProcessCodingHalf::Encode => self.encode,
        }
    }
}

/// The dynamic Lisp variables the connection primitives can consult, captured
/// before the primitive starts creating anything.
///
/// It exists because `builtin_make_pipe_process_impl` and
/// `builtin_make_serial_process_impl` run below the evaluator, on split
/// `&mut ProcessManager` / `&mut BufferManager` borrows, and so cannot read a
/// dynamic variable themselves.  Making it a REQUIRED parameter of both is
/// what stops a pipe or serial process from being created without its ambient
/// coding environment being supplied -- the same hole entry 131 closed for
/// `make-process` and left open here.
///
/// The per-primitive environments are built from it, and each drops what its
/// resolver may not see.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ConnectionProcessCodingVariables {
    coding_system_for_read: Value,
    coding_system_for_write: Value,
    default_process_coding_system: Value,
}

impl ConnectionProcessCodingVariables {
    /// The environment of an editor in which none of the coding variables is
    /// set.  Only for callers that have no evaluator to read them from; a real
    /// `make-pipe-process` / `make-serial-process` always goes through the
    /// `builtin_make_*` wrapper.  Nothing outside the tests has one, and the
    /// `cfg` says so rather than leaving a way to create a pipe process with a
    /// blank environment by accident.
    #[cfg(test)]
    pub(crate) fn unbound() -> Self {
        Self {
            coding_system_for_read: Value::NIL,
            coding_system_for_write: Value::NIL,
            default_process_coding_system: Value::NIL,
        }
    }

    fn pipe(self, multibyte: ProcessBufferMultibyteness) -> PipeProcessCodingEnvironment {
        PipeProcessCodingEnvironment {
            coding_system_for_read: self.coding_system_for_read,
            coding_system_for_write: self.coding_system_for_write,
            default_process_coding_system: self.default_process_coding_system,
            short_circuit: ConnectionProcessUnibyteShortCircuit::pipe(multibyte),
        }
    }

    /// `default-process-coding-system` is dropped here, and no buffer is asked
    /// about: `Fmake_serial_process`'s chain has neither step.
    fn serial(self) -> SerialProcessCodingEnvironment {
        SerialProcessCodingEnvironment {
            coding_system_for_read: self.coding_system_for_read,
            coding_system_for_write: self.coding_system_for_write,
        }
    }
}

/// The ambient inputs GNU's `Fmake_pipe_process` coding resolver reads
/// (src/process.c:2517-2570).
///
/// There is deliberately no `operation_coding_system` field.  A pipe process
/// can never reach `process-coding-system-alist`: GNU initialises
/// `coding_systems` to `Qt` at src/process.c:2520 and never assigns it, so the
/// `CONSP (coding_systems)` arm at :2542 and :2563 is dead code.  Measured --
/// `(let ((process-coding-system-alist '(("pw137" binary . binary)))) ...)`
/// leaves GNU at `(utf-8-unix . utf-8-unix)`.  Leaving the field out is what
/// stops the dead arm from being revived by a well-meaning reader who noticed
/// that `Fmake_process` has one.
#[derive(Clone, Copy, Debug)]
struct PipeProcessCodingEnvironment {
    coding_system_for_read: Value,
    coding_system_for_write: Value,
    default_process_coding_system: Value,
    short_circuit: ConnectionProcessUnibyteShortCircuit,
}

/// The ambient inputs GNU's `Fmake_serial_process` coding resolver reads
/// (src/process.c:3247-3275) -- which is to say, the two dynamic overrides and
/// nothing else.
///
/// Three fields the other two environments carry are missing, and each omission
/// is load-bearing:
///
/// * no `operation_coding_system`: there is not even a `coding_systems`
///   variable in `Fmake_serial_process` to make an alist lookup out of, so a
///   serial process can never reach `process-coding-system-alist`;
/// * no `default_process_coding_system`: the chain has no tail at all.  After
///   the overrides `val` is simply left at the `Qnil` it was initialised to at
///   src/process.c:3249 and :3263.  Measured -- with
///   `default-process-coding-system` bound to `(latin-1 . koi8-r)` GNU still
///   answers `(nil . nil)`;
/// * no `short_circuit`: GNU does spell the unibyte-buffer arm out for both
///   halves (:3258-3260, :3272-3274), but since the fallthrough is also `Qnil`
///   the arm cannot change the answer.  It is unobservable, and a field that
///   cannot change an answer is a field a reader will eventually key something
///   real off.  Measured: a unibyte process buffer and a multibyte one give
///   the same `(nil . nil)`.
#[derive(Clone, Copy, Debug)]
struct SerialProcessCodingEnvironment {
    coding_system_for_read: Value,
    coding_system_for_write: Value,
}

/// The ambient inputs GNU's `set_network_socket_coding_system` consults
/// (src/process.c:3291-3367).  Keeping them together prevents a connection path
/// (TCP, local, datagram, deferred DNS/TLS) from silently omitting one level of
/// the precedence chain.
///
/// This is the only one of the three with an `operation_coding_system` field:
/// the network resolver really does call `find-operation-coding-system`, with
/// `open-network-stream` and only when HOST and SERVICE are both non-nil
/// (src/process.c:3325-3330).  `open-network-stream`'s `target-idx` is 3
/// (src/coding.c:11788), so `network-coding-system-alist` is matched against
/// the SERVICE, not the process name.
#[derive(Clone, Copy, Debug)]
struct NetworkProcessCodingEnvironment {
    coding_system_for_read: Value,
    coding_system_for_write: Value,
    operation_coding_system: Value,
    default_process_coding_system: Value,
    short_circuit: ConnectionProcessUnibyteShortCircuit,
}

/// What step one of a connection primitive's chain answered, and whether the
/// chain goes on.
///
/// The distinction is not decoration: it is where `Fmake_process` and the three
/// connection primitives genuinely part company, and the difference is
/// invisible unless the two states are separated.  GNU writes the connection
/// primitives as ONE `else if` chain, so a non-nil `tem` skips every later arm
/// and a `:coding` whose half is nil answers NIL:
///
/// ```c
///     if (!NILP (tem))            { val = tem; if (CONSP (val)) val = XCAR (val); }
///     else if (!NILP (Vcoding_system_for_read))  val = Vcoding_system_for_read;
///     else if (/* unibyte buffer */)             val = Qnil;
///     else                        { /* alist, default */ }        /* :2523-2548 */
/// ```
///
/// `Fmake_process` writes the same first two arms and then a SEPARATE
/// `if (NILP (val))` for the tail (src/process.c:1950-1976), so there a
/// `:coding` whose half is nil falls through to the alist and the default.
/// Measured, both under GNU 31.0.90, with `coding-system-for-read` bound to
/// `binary` and `:coding '(nil . latin-1)`:
///
/// ```text
/// make-pipe-process    => (nil . latin-1)
/// make-serial-process  => (nil . latin-1)
/// make-network-process => (nil . latin-1)
/// make-process         => (utf-8-unix . latin-1)     ; the tail ran
/// ```
///
/// A helper that returned a bare `Value` could not say which of those two it
/// meant, and the first thing built on one that did was wrong in exactly this
/// row.
enum ConnectionCodingStep {
    /// The chain is over.  A supplied `:coding` ends it whatever its half holds;
    /// a non-nil dynamic override ends it too.
    Answered(Value),
    /// Nothing has answered yet, so the rest of THIS primitive's chain runs.
    Continue,
}

/// Step one of a connection primitive's chain, for one direction
/// (src/process.c:2523-2532, :3247-3257, :3301-3313).
fn connection_process_coding_step(
    coding: Value,
    half: ProcessCodingHalf,
    read_override: Value,
    write_override: Value,
) -> ConnectionCodingStep {
    if !coding.is_nil() {
        return ConnectionCodingStep::Answered(half.of(coding));
    }
    let dynamic = match half {
        ProcessCodingHalf::Decode => read_override,
        ProcessCodingHalf::Encode => write_override,
    };
    if dynamic.is_nil() {
        ConnectionCodingStep::Continue
    } else {
        ConnectionCodingStep::Answered(dynamic)
    }
}

/// The two buffers a connection primitive's short circuit can ask about, read
/// out of the buffer table.
///
/// A process with no buffer answers GNU's `buffer_defaults` arm, which is
/// multibyte in every editor this runs in.
fn process_buffer_multibyteness(
    buffers: &BufferManager,
    process_buffer: Value,
) -> ProcessBufferMultibyteness {
    ProcessBufferMultibyteness {
        process_buffer: process_buffer
            .as_buffer_id()
            .and_then(|id| buffers.get(id))
            .map(|buffer| buffer.get_multibyte())
            .unwrap_or(true),
        current_buffer: buffers
            .current_buffer()
            .map(|buffer| buffer.get_multibyte())
            .unwrap_or(true),
    }
}

/// GNU validates a connection process's coding systems where it INSTALLS them,
/// not where it parses `:coding`: `setup_process_coding_systems` runs
/// `setup_coding_system` on each half, and that is what signals
/// `coding-system-error` for an undefined name (src/coding.c:5678, reached from
/// src/process.c:2573, :3277).  So the value that has to be checked is the one
/// the chain produced -- a bad `coding-system-for-read` signals for
/// `make-pipe-process` exactly as a bad `:coding` does.  Measured under GNU
/// 31.0.90.
fn validate_resolved_process_coding_systems(
    coding_systems: Option<&super::coding::CodingSystemManager>,
    resolved: ProcessCodingSystems,
) -> Result<(), Flow> {
    validate_process_coding_component(coding_systems, resolved.decode)?;
    validate_process_coding_component(coding_systems, resolved.encode)
}

/// GNU `Fmake_pipe_process`'s coding resolver, src/process.c:2517-2570.
fn resolve_pipe_process_coding_systems(
    coding: Value,
    env: PipeProcessCodingEnvironment,
) -> ProcessCodingSystems {
    let resolve = |half: ProcessCodingHalf| {
        match connection_process_coding_step(
            coding,
            half,
            env.coding_system_for_read,
            env.coding_system_for_write,
        ) {
            ConnectionCodingStep::Answered(value) => value,
            ConnectionCodingStep::Continue if env.short_circuit.takes(half) => Value::NIL,
            // src/process.c:2542-2547 (decode) and :2563-2568 (encode); the
            // `CONSP (coding_systems)` arm above each is dead, so what is left
            // is `default-process-coding-system` and then nil.
            ConnectionCodingStep::Continue if env.default_process_coding_system.is_cons() => {
                half.of(env.default_process_coding_system)
            }
            ConnectionCodingStep::Continue => Value::NIL,
        }
    };
    ProcessCodingSystems {
        decode: resolve(ProcessCodingHalf::Decode),
        encode: resolve(ProcessCodingHalf::Encode),
    }
}

/// GNU `Fmake_serial_process`'s coding resolver, src/process.c:3247-3275.
///
/// The whole chain is the override.  Everything after it in GNU's C lands on
/// the `Qnil` the variable already held, so `nil` -- which
/// `setup_coding_system` reads as `undecided`, i.e. DETECT (src/coding.c:
/// 5675-5676) -- is a serial process's normal answer, not an omission.
fn resolve_serial_process_coding_systems(
    coding: Value,
    env: SerialProcessCodingEnvironment,
) -> ProcessCodingSystems {
    let resolve = |half: ProcessCodingHalf| match connection_process_coding_step(
        coding,
        half,
        env.coding_system_for_read,
        env.coding_system_for_write,
    ) {
        ConnectionCodingStep::Answered(value) => value,
        ConnectionCodingStep::Continue => Value::NIL,
    };
    ProcessCodingSystems {
        decode: resolve(ProcessCodingHalf::Decode),
        encode: resolve(ProcessCodingHalf::Encode),
    }
}

/// Create a network process record, running GNU's check on its coding pair at
/// GNU's moment: after the socket exists.
///
/// Every one of the socket strategies reaches this after its connect, listen or
/// bind has succeeded, which is where `connect_network_socket` calls
/// `setup_process_coding_systems` (src/process.c:3761).  The ordering is
/// measurable and it is the reason the check is not done up front with the
/// resolution: against a refused port, with `coding-system-for-read` bound to
/// an undefined name, GNU reports `(file-error "make client process failed")`
/// and never gets as far as the coding system.
fn create_network_process_record(
    eval: &mut super::eval::Context,
    name: LispString,
    buffer: Value,
    coding: ProcessCodingSystems,
) -> Result<ProcessId, Flow> {
    validate_resolved_process_coding_systems(Some(&eval.coding_systems), coding)?;
    Ok(eval.processes.create_process_with_kind_lisp(
        name,
        buffer,
        LispString::from_utf8("network"),
        Vec::new(),
        ProcessKindWithoutDevice::Network,
        coding,
    ))
}

/// GNU `set_network_socket_coding_system`'s resolver, src/process.c:3291-3367.
///
/// `url-open-stream` dynamically binds `coding-system-for-read` to `binary`;
/// losing that binding decodes response bytes as UTF-8 and makes the URL buffer
/// shorter than its HTTP Content-Length.
fn resolve_network_process_coding_systems(
    coding: Value,
    env: NetworkProcessCodingEnvironment,
) -> ProcessCodingSystems {
    let resolve = |half: ProcessCodingHalf| {
        match connection_process_coding_step(
            coding,
            half,
            env.coding_system_for_read,
            env.coding_system_for_write,
        ) {
            ConnectionCodingStep::Answered(value) => value,
            ConnectionCodingStep::Continue if env.short_circuit.takes(half) => Value::NIL,
            // src/process.c:3325-3336 (decode) and :3352-3366 (encode).
            ConnectionCodingStep::Continue if env.operation_coding_system.is_cons() => {
                half.of(env.operation_coding_system)
            }
            ConnectionCodingStep::Continue if env.default_process_coding_system.is_cons() => {
                half.of(env.default_process_coding_system)
            }
            ConnectionCodingStep::Continue => Value::NIL,
        }
    };
    ProcessCodingSystems {
        decode: resolve(ProcessCodingHalf::Decode),
        encode: resolve(ProcessCodingHalf::Encode),
    }
}

fn find_network_operation_coding_system(
    eval: &mut super::eval::Context,
    name: &LispString,
    buffer: Value,
    host: Value,
    service: Value,
) -> EvalResult {
    if host.is_nil()
        || service.is_nil()
        || eval
            .visible_variable_value_or_nil("network-coding-system-alist")
            .is_nil()
    {
        return Ok(Value::NIL);
    }

    // GNU calls `(find-operation-coding-system 'open-network-stream NAME
    // BUFFER HOST SERVICE)`.  Root every heap value because a user-supplied
    // network-coding callback can run arbitrary Lisp and trigger GC.
    let args = vec![
        Value::symbol("open-network-stream"),
        Value::heap_string(name.clone()),
        buffer,
        host,
        service,
    ];
    let roots = eval.save_specpdl_roots();
    for value in &args {
        eval.push_specpdl_root(*value);
    }
    let result = super::builtins::builtin_find_operation_coding_system(eval, args);
    eval.restore_specpdl_roots(roots);
    result
}

/// Which direction of a coding pair a resolution step is answering.
///
/// GNU writes the two directions out twice, as near-mirror blocks
/// (src/process.c:1950-1977 and :1979-2008), and the only differences are which
/// half of a cons is taken and which dynamic variable acts as the override.
/// Naming the direction lets the one function stand for both blocks without
/// inviting the far more dangerous kind of sharing -- between the five
/// different per-caller resolvers, which genuinely disagree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProcessCodingHalf {
    Decode,
    Encode,
}

impl ProcessCodingHalf {
    fn of(self, pair: Value) -> Value {
        if !pair.is_cons() {
            return pair;
        }
        match self {
            Self::Decode => pair.cons_car(),
            Self::Encode => pair.cons_cdr(),
        }
    }
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
pub(crate) struct ProcessCodingSystems {
    decode: Value,
    encode: Value,
}

impl ProcessCodingSystems {
    /// The pair GNU's `make_process` leaves behind before any of the five
    /// resolvers has run: both slots nil, which `setup_coding_system` reads as
    /// `undecided` -- DETECT (src/coding.c:5675-5676).
    ///
    /// This is for process records that are not one of GNU's five primitives:
    /// internal bookkeeping records and unit-test fixtures that never carry
    /// bytes.  It is not a fallback for a resolver that was not written; that
    /// mistake is what entries 128, 131 and 137 are about, and the reason this
    /// constructor is named after GNU's initial state rather than after a
    /// default is so that using it says which claim is being made.  Today only
    /// test fixtures make that claim, and the `cfg` keeps it that way until
    /// something in the runtime has a reason to.
    #[cfg(test)]
    pub(crate) fn gnu_make_process_initial() -> Self {
        Self {
            decode: Value::NIL,
            encode: Value::NIL,
        }
    }

    /// The pair an accepted connection takes from its server.
    ///
    /// `server_accept_connection` does NOT re-run the network resolver: it
    /// copies the listening process's slots, "as the coding system of the new
    /// process should reflect the settings at the time the server socket was
    /// opened; not the current settings" (src/process.c:5152-5158).
    pub(crate) fn inherited_from_server(decode: Value, encode: Value) -> Self {
        Self { decode, encode }
    }
}

/// The ambient inputs GNU's `Fmake_process` coding resolver reads
/// (src/process.c:1950-2008).
///
/// It is a bundle rather than four arguments because the resolution has to run
/// at GNU's point in the creation sequence -- after the executable search, so
/// that a missing program still signals `file-missing` before a bad coding
/// system signals `coding-system-error` (measured on both editors) -- while
/// `find-operation-coding-system` can run arbitrary Lisp and therefore cannot
/// run under the split `&mut ProcessManager` / `&mut BufferManager` borrows the
/// creator holds.  Making it a REQUIRED parameter of the creator is what stops
/// a real subprocess from being created with a coding system nobody resolved.
#[derive(Clone, Copy, Debug)]
pub(crate) struct MakeProcessCodingEnvironment {
    coding_system_for_read: Value,
    coding_system_for_write: Value,
    default_process_coding_system: Value,
    /// What `(find-operation-coding-system 'start-process NAME BUFFER
    /// COMMAND...)` answered, or nil when GNU would not have asked -- see
    /// `make_process_consults_coding_alist`.
    operation_coding_system: Value,
}

impl MakeProcessCodingEnvironment {
    /// The environment of an editor in which none of the coding variables is
    /// set.  Only for callers that have no evaluator to read them from; a real
    /// `make-process` always goes through `builtin_make_process`.
    fn unbound() -> Self {
        Self {
            coding_system_for_read: Value::NIL,
            coding_system_for_write: Value::NIL,
            default_process_coding_system: Value::NIL,
            operation_coding_system: Value::NIL,
        }
    }

    /// What `Fmake_process` hands on when `:stderr` is a BUFFER and it builds a
    /// pipe process for it with `CALLN (Fmake_pipe_process, ...)`
    /// (src/process.c:1883): the ambient variables, not its own answer.  The
    /// operation coding is dropped because `Fmake_pipe_process` cannot reach
    /// `find-operation-coding-system` at all.
    fn connection_variables(self) -> ConnectionProcessCodingVariables {
        ConnectionProcessCodingVariables {
            coding_system_for_read: self.coding_system_for_read,
            coding_system_for_write: self.coding_system_for_write,
            default_process_coding_system: self.default_process_coding_system,
        }
    }
}

/// GNU consults `process-coding-system-alist` only when the chain reaches it:
/// `:coding` did not answer this half AND the matching `coding-system-for-*`
/// override is nil (src/process.c:1959 for decode, :1987 for encode).  Keeping
/// the predicate beside the resolver is what stops a function-valued alist
/// entry from running when GNU would never have called it -- measured: with
/// `:command nil` GNU skips the lookup entirely (src/process.c:1970), and with
/// `:coding 'utf-8` bound over a matching alist entry it never asks.
fn make_process_consults_coding_alist(coding: Value, env: MakeProcessCodingEnvironment) -> bool {
    [ProcessCodingHalf::Decode, ProcessCodingHalf::Encode]
        .into_iter()
        .any(|half| make_process_coding_override(coding, half, env).is_nil())
}

/// Step one of GNU's `Fmake_process` chain, for one direction: the `:coding`
/// keyword when it was supplied at all (its car for decode, its cdr for encode,
/// itself when it is not a cons), else the dynamic
/// `coding-system-for-read`/`-write` override (src/process.c:1950-1957 and
/// :1978-1985).
///
/// A SUPPLIED `:coding` shuts the dynamic override out even when its own half
/// is nil -- GNU's `else` at :1956 is only reached when `tem` itself is nil.
/// But unlike the three connection primitives, `Fmake_process` then runs its
/// tail from a SEPARATE `if (NILP (val))` at :1958, so a nil half does fall
/// through to the alist and the default.  It returns a bare `Value` rather than
/// a `ConnectionCodingStep` precisely because "nil" and "nothing answered" are
/// the same thing here and are NOT the same thing there.  Measured:
/// `:coding '(nil . latin-1)` under `coding-system-for-read` bound to `binary`
/// decodes as `utf-8-unix` for `make-process` and as nil for the other three.
fn make_process_coding_override(
    coding: Value,
    half: ProcessCodingHalf,
    env: MakeProcessCodingEnvironment,
) -> Value {
    if coding.is_nil() {
        match half {
            ProcessCodingHalf::Decode => env.coding_system_for_read,
            ProcessCodingHalf::Encode => env.coding_system_for_write,
        }
    } else {
        half.of(coding)
    }
}

/// GNU `Fmake_process`'s coding resolver, src/process.c:1950-2008.
///
/// This is `Fmake_process`'s chain and no other's.  `Fcall_process` has no
/// `:coding` step and validates its answer on the spot (src/callproc.c:732,
/// :753); `Fmake_pipe_process` and `Fmake_serial_process` never reach
/// `find-operation-coding-system` at all -- their `coding_systems` is
/// initialised to `Qt` and never assigned (src/process.c:2520, :3298), so the
/// `CONSP (coding_systems)` arm is dead code there -- and they, like
/// `set_network_socket_coding_system`, short-circuit the whole tail to nil when
/// the buffer is unibyte, deliberately, so that "the existing Emacs Lisp
/// libraries ... receive bare code including a sequence of CR LF"
/// (src/process.c:2535-2539).  `Fmake_process` has no such short-circuit: its
/// comment at :1942-1944 says the unibyte question is settled later, in
/// `create_process`.  Sharing one implementation between them would have to
/// parameterise every one of those differences, which is another way of saying
/// there is nothing left to share.
fn resolve_make_process_coding_systems(
    coding: Value,
    env: MakeProcessCodingEnvironment,
) -> ProcessCodingSystems {
    let resolve = |half: ProcessCodingHalf| {
        let chosen = make_process_coding_override(coding, half, env);
        if !chosen.is_nil() {
            return chosen;
        }
        // src/process.c:1972-1975 (decode) and :2003-2006 (encode).
        if env.operation_coding_system.is_cons() {
            half.of(env.operation_coding_system)
        } else if env.default_process_coding_system.is_cons() {
            half.of(env.default_process_coding_system)
        } else {
            Value::NIL
        }
    };
    ProcessCodingSystems {
        decode: resolve(ProcessCodingHalf::Decode),
        encode: resolve(ProcessCodingHalf::Encode),
    }
}

fn validate_process_coding_component(
    coding_systems: Option<&super::coding::CodingSystemManager>,
    value: Value,
) -> Result<(), Flow> {
    if let Some(coding_systems) = coding_systems {
        super::coding::builtin_check_coding_system(coding_systems, vec![value]).map(|_| ())
    } else if value.is_nil() || value.as_symbol_name().is_some() {
        Ok(())
    } else {
        Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), value],
        ))
    }
}

fn copy_process_plist(plist: Value) -> EvalResult {
    super::builtins::builtin_copy_sequence(vec![plist])
}

fn apply_connection_process_flags(proc: &mut Process, noquery: bool, stop: bool) {
    if noquery {
        proc.exit_query_policy = ExitQueryPolicy::NoQuery;
    }
    if stop {
        proc.command = Value::T;
    }
}

fn serial_contact_value(contact: Value, current: Value, keyword: ProcessKeyword) -> Value {
    let key = keyword.value();
    if !process_contact_plist_member(contact, key).is_nil() {
        process_contact_plist_get(contact, key)
    } else {
        process_contact_plist_get(current, key)
    }
}

fn serial_expect_fixnum(value: Value) -> Result<i64, Flow> {
    value.as_fixnum().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("fixnump"), value],
        )
    })
}

fn serial_value_eq_symbol(value: Value, symbol: &str) -> bool {
    crate::emacs_core::value::eq_value(&value, &Value::symbol(symbol))
}

/// GNU `serial_open`, src/sysdep.c:2980-2990, plus its error boundary.
///
/// `report_file_error ("Opening serial port", port)` (:2984) is the same
/// errno classification every other failed open in Emacs gets, so a missing
/// device is `file-missing`, an unreadable one is `permission-denied` and
/// everything else is `file-error` -- all three measured against GNU 31.0.90.
fn open_serial_port(port: Value, port_name: &LispString) -> Result<sys::SerialPort, Flow> {
    sys::SerialPort::open(&lisp_string_to_os_string(port_name)).map_err(|err| {
        signal_file_errno(
            "Opening serial port",
            port,
            err.raw_os_error().unwrap_or(libc::EIO),
        )
    })
}

/// GNU `serial_configure`, src/sysdep.c:3151-3309: the keyword half, run in the
/// window [`sys::SerialPort::configure`] opens between `tcgetattr` and
/// `tcsetattr`.
///
/// Returns the new `childp` plist -- GNU's `childp2`, which it writes back only
/// at the very end (:3308), so a rejected keyword leaves both the device and
/// the process contact exactly as they were.
///
/// The three failure classes are three different Lisp errors and they are
/// ordered, which is the whole reason this runs inside a closure rather than
/// before or after the device work:
///
/// ```text
/// :port "/dev/null" :speed 9600 :bytesize 5
///   GNU => (file-error "Failed tcgetattr" "Inappropriate ioctl for device")
/// ```
///
/// The `:bytesize` message never appears, because the read failed first.
fn configure_serial_device(device: &sys::SerialPort, current: Value, contact: Value) -> EvalResult {
    let mut childp = Value::NIL;
    let outcome = device.configure(|attributes| -> Result<(), Flow> {
        childp = apply_serial_settings(attributes, current, contact)?;
        Ok(())
    });
    match outcome {
        Ok(()) => Ok(childp),
        Err(sys::SerialConfigureFailure::Settings(flow)) => Err(flow),
        Err(sys::SerialConfigureFailure::Device { step, errno }) => {
            // GNU names the failing call and passes `Qnil` as the file name for
            // both (src/sysdep.c:3165-3166, :3304-3305).
            let message = match step {
                sys::SerialConfigureStep::ReadAttributes => "Failed tcgetattr",
                sys::SerialConfigureStep::WriteAttributes => "Failed tcsetattr",
            };
            Err(signal_file_errno(message, Value::NIL, errno))
        }
    }
}

/// GNU's five keyword arms, src/sysdep.c:3175-3300, each applied to the local
/// attribute copy as it is validated -- exactly GNU's interleaving, so an
/// invalid `:stopbits` still leaves the speed and byte size already applied to
/// the copy and still never reaches the device.
fn apply_serial_settings(
    attributes: &mut sys::SerialAttributes,
    current: Value,
    contact: Value,
) -> EvalResult {
    let mut childp = copy_process_plist(current)?;

    let speed = serial_contact_value(contact, current, ProcessKeyword::Speed);
    let speed_num = serial_expect_fixnum(speed)?;
    attributes
        .set_speed(speed_num)
        // GNU's only step whose error names the offending value:
        // `report_file_error ("Failed cfsetspeed", tem)`, src/sysdep.c:3183.
        .map_err(|err| signal_file_errno("Failed cfsetspeed", speed, err.errno))?;
    childp = process_contact_plist_put(childp, ProcessKeyword::Speed.value(), speed)?;

    let mut bytesize = serial_contact_value(contact, current, ProcessKeyword::Bytesize);
    if bytesize.is_nil() {
        bytesize = Value::fixnum(8);
    }
    let bytesize_num = serial_expect_fixnum(bytesize)?;
    let byte_size = match bytesize_num {
        7 => sys::SerialByteSize::Seven,
        8 => sys::SerialByteSize::Eight,
        _ => {
            return Err(signal(
                "error",
                vec![Value::string(":bytesize must be nil (8), 7, or 8")],
            ));
        }
    };
    attributes.set_byte_size(byte_size);
    childp = process_contact_plist_put(childp, ProcessKeyword::Bytesize.value(), bytesize)?;

    let parity_value = serial_contact_value(contact, current, ProcessKeyword::Parity);
    let parity = if parity_value.is_nil() {
        sys::SerialParity::None
    } else if serial_value_eq_symbol(parity_value, "even") {
        sys::SerialParity::Even
    } else if serial_value_eq_symbol(parity_value, "odd") {
        sys::SerialParity::Odd
    } else {
        return Err(signal(
            "error",
            vec![Value::string(
                ":parity must be nil (no parity), `even', or `odd'",
            )],
        ));
    };
    attributes.set_parity(parity);
    childp = process_contact_plist_put(childp, ProcessKeyword::Parity.value(), parity_value)?;

    let mut stopbits = serial_contact_value(contact, current, ProcessKeyword::Stopbits);
    if stopbits.is_nil() {
        stopbits = Value::fixnum(1);
    }
    let stopbits_num = serial_expect_fixnum(stopbits)?;
    let stop_bits = match stopbits_num {
        1 => sys::SerialStopBits::One,
        2 => sys::SerialStopBits::Two,
        _ => {
            return Err(signal(
                "error",
                vec![Value::string(":stopbits must be nil (1 stopbit), 1, or 2")],
            ));
        }
    };
    attributes.set_stop_bits(stop_bits);
    childp = process_contact_plist_put(childp, ProcessKeyword::Stopbits.value(), stopbits)?;

    let flowcontrol_value = serial_contact_value(contact, current, ProcessKeyword::Flowcontrol);
    let flow_control = if flowcontrol_value.is_nil() {
        sys::SerialFlowControl::None
    } else if serial_value_eq_symbol(flowcontrol_value, "hw") {
        sys::SerialFlowControl::Hardware
    } else if serial_value_eq_symbol(flowcontrol_value, "sw") {
        sys::SerialFlowControl::Software
    } else {
        return Err(signal(
            "error",
            vec![Value::string(
                ":flowcontrol must be nil (no flowcontrol), `hw', or `sw'",
            )],
        ));
    };
    attributes.set_flow_control(flow_control);
    childp = process_contact_plist_put(
        childp,
        ProcessKeyword::Flowcontrol.value(),
        flowcontrol_value,
    )?;

    // GNU's `summary` is built one character at a time as each arm validates
    // (`summary[0]`, `[1]`, `[2]`), and put into the contact last (:3307).
    let parity_summary = match parity {
        sys::SerialParity::None => "N",
        sys::SerialParity::Even => "E",
        sys::SerialParity::Odd => "O",
    };
    let summary = format!("{bytesize_num}{parity_summary}{stopbits_num}");
    process_contact_plist_put(
        childp,
        ProcessKeyword::Summary.value(),
        Value::string(&summary),
    )
}

/// What a read off a process's file descriptor produced, BEFORE it has been
/// decoded and before GNU's `read_process_output_set_last_coding_system` has
/// run on it.
///
/// The [`PendingProcessRun`] in `Data` is the thing that still has to be
/// decoded -- through the shared engine, which needs the evaluator -- and then
/// written back onto the process and into `last-coding-system-used`.  The only
/// way to get rid of it is [`Context::read_process_output_recording_coding`],
/// which does both.  So a caller that wants the TEXT can reach neither a second
/// decoder nor a missing write-back: the stages are separated by types, not by
/// a convention.
#[derive(Debug)]
enum ProcessBytesRead {
    Data {
        run: PendingProcessRun,
        bytes_read: usize,
    },
    /// GNU's EOF read on the filter branch of a pipe: `emacs_read` returned
    /// nothing with `CODING_MODE_LAST_BLOCK` not yet raised, so
    /// `read_process_output` raises it and falls THROUGH to a decode of zero
    /// bytes (src/process.c:6313-6321) before returning 0 -- which its caller
    /// reads as end of file (:6345, :6027).
    ///
    /// One read, both facts, so one variant.  The run still has to go through
    /// the decoder, because a zero-byte `decode_coding_object` still runs the
    /// coding system's `:post-read-conversion` (src/coding.c:8180-8194) and
    /// still writes `last-coding-system-used`
    /// (`read_process_output_set_last_coding_system`, src/process.c:6421) --
    /// and the hook may insert text of its own, which GNU counts into
    /// `coding->produced_char` (:8194) and hands to the filter like any other
    /// run.
    EofAfterLastBlock {
        run: PendingProcessRun,
    },
    /// GNU's `read_and_insert_process_output` early return
    /// (src/process.c:6464-6465) with bytes in hand: the read happened and its
    /// caller must count it (`read_process_output` returns `nbytes`, :6345),
    /// but nothing was converted, so there is no run to decode and nothing to
    /// report.  A variant rather than an empty [`PendingProcessRun`], because
    /// a discarded read has no coding system -- `detect_coding` never ran on
    /// it either.
    Discarded {
        bytes_read: usize,
    },
    WouldBlock,
    /// The descriptor read failed for a reason other than temporary
    /// unavailability.  GNU keeps this distinct from a zero-byte EOF so the
    /// wait loop can publish the connection-failure status instead of a
    /// successful close.
    Failed,
    Eof,
    NoSource,
}

/// A [`PendingProcessRun`] once the shared decoder has run on it: the text, the
/// coding system the decode ended on, and the two things the write-back still
/// owes the process record.
#[derive(Debug)]
struct DecodedPendingProcessRun {
    run: ProcessDecodedRun,
    decoder: crate::encoding::CodingDecoderState,
}

/// The same read, after the coding system it used has been recorded.
///
/// This is what every driver on the `Context` side sees, and it no longer
/// carries a coding system because the recording has already happened.
#[derive(Debug)]
enum ProcessOutputRead {
    Data { data: LispString, bytes_read: usize },
    WouldBlock,
    Failed,
    Eof,
    NoSource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProcessOutputDrainDisposition {
    Output,
    Blocked,
    Terminal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProcessOutputSource {
    Pty,
    ChildStdout,
    Network,
    /// The `termios` device a `make-serial-process` opened.  GNU reads it
    /// through the same `read_process_output` as every other process
    /// (`p->infd` is the serial fd, src/process.c:3216).
    Serial,
}

fn process_output_source(proc: &Process) -> Option<ProcessOutputSource> {
    if proc.live_io.pty_reader.is_some() {
        Some(ProcessOutputSource::Pty)
    } else if proc.live_io.child_stdout.is_some() {
        Some(ProcessOutputSource::ChildStdout)
    } else if proc.live_io.tls_stream.is_some() || proc.live_io.network_socket.is_some() {
        Some(ProcessOutputSource::Network)
    } else if proc.live_io.serial_port.is_some() {
        Some(ProcessOutputSource::Serial)
    } else {
        None
    }
}

/// GNU `wait_reading_process_output` ends a WAIT_PROC wait when the target's
/// INTERNAL status is neither `Qrun` nor a pending connect (process.c: the
/// `wait_proc && !EQ (wait_proc->status, Qrun) && !connecting_status` drain +
/// break). GNU's internal statuses differ from the `process-status`
/// projection: a listen server is stored as `Qlisten` (ends the wait —
/// verified empirically: `accept-process-output` on a server returns at
/// once), while an io-paused connection (`stop-process` on a netconn, GNU
/// `p->command = Qt`) stays internally `Qrun` and does NOT end the wait even
/// though `process-status` projects it as `stop`.
///
/// neomacs's storage model: connected netconns AND servers both store `run`
/// (projected to `open`/`listen` by `process_public_status_symbol` via
/// `process_contact_server_p`), io-pause is a separate flag
/// (`process_stopped_for_io`), and real children store their observed status
/// directly. Map GNU's internal-status rule onto that:
pub(crate) fn process_status_ends_target_wait(process: &Process) -> bool {
    match ProcessStatusSymbol::from_status_value(process.status) {
        // Stored `run`: GNU internal Qrun for real children and connected
        // netconns (keep waiting) -- but a server's GNU-internal status is
        // Qlisten, which ends the wait.
        Some(ProcessStatusSymbol::Run) => {
            process.kind == ProcessKind::Network && process_contact_server_p(process)
        }
        // Pending :nowait connect keeps waiting (GNU connecting_status).
        Some(ProcessStatusSymbol::Connect) => false,
        // Stored `open` (paths that store the projection directly) is GNU
        // internal Qrun for a connection.
        Some(ProcessStatusSymbol::Open) => false,
        // listen / stop (a genuinely stopped child) / exit / signal / failed /
        // closed all end the wait.
        Some(
            ProcessStatusSymbol::Listen
            | ProcessStatusSymbol::Stop
            | ProcessStatusSymbol::Exit
            | ProcessStatusSymbol::Signal
            | ProcessStatusSymbol::Failed
            | ProcessStatusSymbol::Closed,
        ) => true,
        None => false,
    }
}

fn process_status_is_run(status: &Value) -> bool {
    ProcessStatusSymbol::from_status_value(*status) == Some(ProcessStatusSymbol::Run)
}

fn process_status_allows_send(status: &Value) -> bool {
    matches!(
        ProcessStatusSymbol::from_status_value(*status),
        Some(ProcessStatusSymbol::Run | ProcessStatusSymbol::Open)
    )
}

fn process_is_listening(proc: &Process) -> bool {
    ProcessStatusSymbol::from_status_value(process_public_status_symbol(proc))
        == Some(ProcessStatusSymbol::Listen)
}

/// GNU `send_process`'s gate (src/process.c:6725-6728) and
/// `Fprocess_send_eof`'s (:7451-7455).  Both read `p->status` on the line
/// AFTER `update_status` has written it, so the value they test is the
/// settled one -- which is why this reads `process_effective_status` and not
/// the raw field.  Reached only through `ObservedProcess::allows_send`, so
/// the recording has been made by then.
fn process_allows_send(proc: &Process) -> bool {
    !process_is_listening(proc) && process_status_allows_send(&process_effective_status(proc))
}

fn process_status_is_connect(status: &Value) -> bool {
    ProcessStatusSymbol::from_status_value(*status) == Some(ProcessStatusSymbol::Connect)
}

fn process_status_is_terminal_for_notify(status: &Value) -> bool {
    matches!(
        ProcessStatusSymbol::from_status_value(*status),
        Some(ProcessStatusSymbol::Exit | ProcessStatusSymbol::Signal | ProcessStatusSymbol::Closed)
    )
}

fn process_status_is_exit_or_signal(status: &Value) -> bool {
    matches!(
        ProcessStatusSymbol::from_status_value(*status),
        Some(ProcessStatusSymbol::Exit | ProcessStatusSymbol::Signal)
    )
}

fn process_status_has_readable_process_io(status: &Value) -> bool {
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

fn process_stopped_for_io(proc: &Process) -> bool {
    proc.command == Value::T
}

const DEFAULT_PROCESS_FILTER_SYMBOL: &str = "internal-default-process-filter";

/// The three behaviors GNU assigns to a process filter Lisp value.
///
/// Keep the original [`Value`] on [`Process`] so `process-filter` can return
/// it exactly.  Classify it at the I/O boundary so Lisp `t` changes read
/// interest instead of ever being treated as a callable value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProcessFilterDispatch {
    Default,
    Suspended,
    Callback(Value),
}

impl ProcessFilterDispatch {
    fn from_lisp(filter: Value) -> Self {
        if filter.is_nil() || filter.is_symbol_named(DEFAULT_PROCESS_FILTER_SYMBOL) {
            Self::Default
        } else if filter.is_t() {
            Self::Suspended
        } else {
            Self::Callback(filter)
        }
    }

    fn accepts_output(self) -> bool {
        !matches!(self, Self::Suspended)
    }
}

fn process_filter_accepts_output(proc: &Process) -> bool {
    ProcessFilterDispatch::from_lisp(proc.filter).accepts_output()
}

fn is_standalone_pipe_process(proc: &Process) -> bool {
    proc.kind == ProcessKind::Pipe
        && proc.live_io.child_stdout.is_some()
        && !proc.live_io.child.has_child()
}

fn process_has_readable_process_io(proc: &Process) -> bool {
    !process_stopped_for_io(proc)
        && process_filter_accepts_output(proc)
        && process_status_has_readable_process_io(&proc.status)
}

fn process_has_observable_child_status(proc: &Process) -> bool {
    matches!(
        ProcessStatusSymbol::from_status_value(proc.status),
        Some(ProcessStatusSymbol::Run | ProcessStatusSymbol::Stop)
    ) && (proc.os_pid.is_some() || proc.live_io.child.has_child())
}

fn process_defers_pty_status_after_explicit_coding(proc: &Process) -> bool {
    proc.coding_explicitly_set
        && (proc.live_io.child.is_pty_handle() || proc.live_io.pty_reader.is_some())
}

fn process_defers_status_poll_while_readable_pty(proc: &Process) -> bool {
    process_output_source(proc) == Some(ProcessOutputSource::Pty)
        && process_has_readable_process_io(proc)
        && process_command_is_shell_command(proc)
}

fn process_command_is_shell_command(proc: &Process) -> bool {
    process_command_lisp_argv(proc.command).is_some_and(|argv| {
        argv.get(1)
            .is_some_and(|arg| arg.as_bytes() == b"-c" || arg.as_bytes() == b"/c")
    })
}

fn process_publishes_status_after_ready_output(proc: &Process) -> bool {
    proc.eof_sent_to_process
        || matches!(
            process_output_source(proc),
            Some(ProcessOutputSource::ChildStdout)
        )
        || (process_output_source(proc) == Some(ProcessOutputSource::Pty)
            && !process_command_is_shell_command(proc))
}

fn process_should_defer_explicit_coding_status_after_output(
    proc: &Process,
    saw_output: bool,
) -> bool {
    saw_output
        && process_defers_pty_status_after_explicit_coding(proc)
        && !proc.explicit_coding_status_deferred_once
}

fn process_is_harness_record_without_write_source(proc: &Process) -> bool {
    proc.os_pid.is_none()
        && !proc.live_io.child.has_pipe_child()
        && proc.live_io.pty_writer.is_none()
        && proc.live_io.tls_stream.is_none()
        && proc.live_io.network_socket.is_none()
        && proc.live_io.serial_port.is_none()
}

impl super::eval::Context {
    fn wait_while_network_process_connecting(&mut self, id: ProcessId) -> Result<(), Flow> {
        while self.processes.get(id).is_some_and(|proc| {
            proc.kind == ProcessKind::Network && process_status_is_connect(&proc.status)
        }) {
            let _ = self.wait_for_process_output(ProcessOutputWaitRequest::new(
                ProcessOutputWaitTiming::For(Duration::from_millis(20)),
                Some(id),
                false,
                true,
            ))?;
        }
        Ok(())
    }

    fn send_process_input_reentrant(
        &mut self,
        id: ProcessId,
        input: &LispString,
    ) -> Result<(), Flow> {
        if !self.processes.queue_input(id, input)? {
            return Err(signal("error", vec![Value::string("Process not found")]));
        }

        loop {
            match self.processes.flush_process_write_queue(id)? {
                ProcessWriteFlush::Drained => return Ok(()),
                ProcessWriteFlush::NoSource => {
                    if self
                        .processes
                        .get_any(id)
                        .is_some_and(process_is_harness_record_without_write_source)
                    {
                        return Ok(());
                    }
                    let name = self
                        .processes
                        .get_any(id)
                        .map(|proc| process_name_runtime(proc.name))
                        .unwrap_or_else(|| id.to_string());
                    return Err(signal(
                        "error",
                        vec![Value::string(format!(
                            "Output file descriptor of {name} is closed"
                        ))],
                    ));
                }
                ProcessWriteFlush::Blocked => {
                    let _ = self.wait_for_process_output(ProcessOutputWaitRequest::new(
                        ProcessOutputWaitTiming::For(Duration::from_millis(20)),
                        None,
                        false,
                        true,
                    ))?;
                }
            }
        }
    }
}

fn pending_network_connect_id(
    processes: &ProcessManager,
    process: Value,
) -> Result<Option<ProcessId>, Flow> {
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &process)?;
    Ok(processes
        .get(id)
        .is_some_and(|proc| {
            proc.kind == ProcessKind::Network && proc.live_io.pending_network_connect.is_some()
        })
        .then_some(id))
}

fn process_uses_contact_plist(proc: &Process) -> bool {
    matches!(
        proc.kind,
        ProcessKind::Network | ProcessKind::Pipe | ProcessKind::Serial
    )
}

fn process_contact_plist_get(contact: Value, key: Value) -> Value {
    super::builtins::builtin_plist_get(vec![contact, key]).unwrap_or(Value::NIL)
}

fn process_contact_plist_put(contact: Value, key: Value, value: Value) -> EvalResult {
    super::builtins::builtin_plist_put(vec![contact, key, value])
}

fn process_contact_plist_member(contact: Value, key: Value) -> Value {
    crate::emacs_core::plist::plist_member(contact, &key)
}

fn process_contact_server_p(proc: &Process) -> bool {
    process_contact_plist_get(proc.childp, ProcessKeyword::Server.value()).is_truthy()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NetworkSocketOption {
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
struct NetworkSocketOptionSpec {
    keyword: ProcessKeyword,
    option: NetworkSocketOption,
    value: Value,
}

#[derive(Debug)]
enum PendingNetworkConnect {
    Tcp {
        remaining_addrs: Vec<SocketAddr>,
        socket_options: Vec<NetworkSocketOptionSpec>,
    },
    Dns(PendingDnsRequest),
    #[cfg(unix)]
    Local,
}

#[derive(Debug)]
struct PendingDnsRequest {
    host: String,
    receiver: mpsc::Receiver<Result<Vec<SocketAddr>, String>>,
    ready: Arc<AtomicBool>,
    socket_options: Vec<NetworkSocketOptionSpec>,
}

impl PendingDnsRequest {
    fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }
}

fn pending_network_connect_has_ready_async_dns(pending: &PendingNetworkConnect) -> bool {
    matches!(pending, PendingNetworkConnect::Dns(request) if request.is_ready())
}

fn process_has_ready_async_dns(proc: &Process) -> bool {
    proc.live_io
        .pending_network_connect
        .as_ref()
        .is_some_and(pending_network_connect_has_ready_async_dns)
}

#[derive(Debug)]
struct PendingNetworkConnectStarted {
    stream: TcpStream,
    remote_addr: SocketAddr,
    remaining_addrs: Vec<SocketAddr>,
}

#[derive(Debug)]
enum PendingNetworkConnectStart {
    Started(PendingNetworkConnectStarted),
    Failed(i32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PendingNetworkConnectCompletion {
    None,
    Retrying,
    Connected { sentinel: Value },
    Failed { sentinel: Value, code: i32 },
    DnsFailed,
}

impl NetworkSocketOption {
    fn from_keyword(keyword: ProcessKeyword) -> Option<Self> {
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

fn network_socket_options_include(
    options: &[NetworkSocketOptionSpec],
    option: NetworkSocketOption,
) -> bool {
    options.iter().any(|spec| spec.option == option)
}

fn collect_network_socket_options(args: &[Value]) -> Vec<NetworkSocketOptionSpec> {
    let mut options = Vec::new();
    let mut i = 0usize;
    while i < args.len() {
        let key = &args[i];
        let value = args.get(i + 1).cloned().unwrap_or(Value::NIL);
        if let Some(keyword) = ProcessKeyword::from_value(key)
            && let Some(option) = NetworkSocketOption::from_keyword(keyword)
        {
            options.push(NetworkSocketOptionSpec {
                keyword,
                option,
                value,
            });
        }
        i += 2;
    }
    options
}

fn network_server_backlog(server_value: Value) -> Result<i32, Flow> {
    if server_value == Value::T {
        return Ok(5);
    }
    match server_value.as_fixnum() {
        Some(backlog) => {
            i32::try_from(backlog).map_err(|_| signal_wrong_type_integerp(server_value))
        }
        None => Err(signal_wrong_type_integerp(server_value)),
    }
}

fn signal_bad_network_option_value(keyword: ProcessKeyword) -> Flow {
    signal(
        "error",
        vec![Value::string(format!(
            "Bad option value for {}",
            keyword.keyword()
        ))],
    )
}

fn signal_network_option_io_error(
    keyword: ProcessKeyword,
    value: Value,
    err: std::io::Error,
) -> Flow {
    signal(
        LispCondition::FileError,
        vec![
            Value::string("Cannot set network option"),
            keyword.value(),
            value,
            Value::string(err.to_string()),
        ],
    )
}

fn network_option_i32_value(keyword: ProcessKeyword, value: Value) -> Result<i32, Flow> {
    match value.as_fixnum().and_then(|n| i32::try_from(n).ok()) {
        Some(n) => Ok(n),
        None => Err(signal_bad_network_option_value(keyword)),
    }
}

#[cfg(unix)]
fn apply_network_socket_option_to_socket(
    socket: &Socket,
    spec: NetworkSocketOptionSpec,
) -> EvalResult {
    let value = spec.value;
    let result = match spec.option {
        #[cfg(any(target_os = "linux", target_os = "android"))]
        NetworkSocketOption::Bindtodevice => {
            if value.is_nil() {
                socket.bind_device(None)
            } else if let Some(name) = value.as_lisp_string() {
                socket.bind_device(Some(name.as_bytes()))
            } else {
                return Err(signal_bad_network_option_value(spec.keyword));
            }
        }
        NetworkSocketOption::Broadcast => socket.set_broadcast(value.is_truthy()),
        NetworkSocketOption::Dontroute => {
            sys::set_socket_dontroute(socket.as_raw_fd(), value.is_truthy())
        }
        NetworkSocketOption::Keepalive => socket.set_keepalive(value.is_truthy()),
        NetworkSocketOption::Linger => {
            let onoff = !value.is_nil();
            let linger = value
                .as_fixnum()
                .and_then(|n| i32::try_from(n).ok())
                .unwrap_or(0);
            sys::set_socket_linger(socket.as_raw_fd(), onoff, linger)
        }
        NetworkSocketOption::Oobinline => socket.set_out_of_band_inline(value.is_truthy()),
        #[cfg(any(target_os = "linux", target_os = "android"))]
        NetworkSocketOption::Priority => {
            let priority = network_option_i32_value(spec.keyword, value)?;
            sys::set_socket_priority(socket.as_raw_fd(), priority)
        }
        NetworkSocketOption::Reuseaddr => socket.set_reuse_address(value.is_truthy()),
        NetworkSocketOption::Nodelay => socket.set_tcp_nodelay(value.is_truthy()),
    };

    result
        .map(|_| Value::T)
        .map_err(|err| signal_network_option_io_error(spec.keyword, value, err))
}

#[cfg(not(unix))]
fn apply_network_socket_option_to_socket(
    _socket: &Socket,
    spec: NetworkSocketOptionSpec,
) -> EvalResult {
    Err(signal(
        "error",
        vec![Value::string(format!(
            "Unsupported network option {}",
            spec.keyword.keyword()
        ))],
    ))
}

fn apply_network_socket_options(
    socket: &Socket,
    options: &[NetworkSocketOptionSpec],
) -> Result<(), Flow> {
    for spec in options.iter().copied() {
        apply_network_socket_option_to_socket(socket, spec)?;
    }
    Ok(())
}

fn apply_network_socket_option_to_process(
    proc: &mut Process,
    spec: NetworkSocketOptionSpec,
) -> EvalResult {
    if let Some(socket) = proc.live_io.network_socket.as_ref() {
        return match socket {
            NetworkSocket::TcpStream(stream) => {
                apply_network_socket_option_to_socket(&SockRef::from(stream), spec)
            }
            NetworkSocket::TcpListener(listener) => {
                apply_network_socket_option_to_socket(&SockRef::from(listener), spec)
            }
            NetworkSocket::UdpSocket(socket) => {
                apply_network_socket_option_to_socket(&SockRef::from(socket), spec)
            }
            #[cfg(unix)]
            NetworkSocket::SeqpacketStream(socket) => {
                apply_network_socket_option_to_socket(&SockRef::from(socket), spec)
            }
            #[cfg(unix)]
            NetworkSocket::SeqpacketListener(socket) => {
                apply_network_socket_option_to_socket(&SockRef::from(socket), spec)
            }
            #[cfg(unix)]
            NetworkSocket::UnixStream(stream) => {
                apply_network_socket_option_to_socket(&SockRef::from(stream), spec)
            }
            #[cfg(unix)]
            NetworkSocket::UnixListener(listener) => {
                apply_network_socket_option_to_socket(&SockRef::from(listener), spec)
            }
            #[cfg(unix)]
            NetworkSocket::UnixDatagram(socket) => {
                apply_network_socket_option_to_socket(&SockRef::from(socket), spec)
            }
        };
    }

    if let Some(tls) = proc.live_io.tls_stream.as_ref() {
        return apply_network_socket_option_to_socket(&SockRef::from(tls.tcp_stream()), spec);
    }

    Err(signal(
        "error",
        vec![Value::string("Process has no socket")],
    ))
}

fn tcp_socket_domain(addr: SocketAddr) -> Domain {
    if addr.is_ipv4() {
        Domain::IPV4
    } else {
        Domain::IPV6
    }
}

fn network_socket_io_error(message: &str, err: std::io::Error) -> Flow {
    network_socket_io_error_with_name(message, Value::NIL, err)
}

/// Translate a socket errno through the same boundary as GNU
/// `report_file_errno`.  NAME is the original network contact plist for
/// connection failures, so callers receive both libc's bare strerror text and
/// the keyword arguments that identify the failed connection.
fn network_socket_io_error_with_name(message: &str, name: Value, err: std::io::Error) -> Flow {
    let errno = err.raw_os_error().unwrap_or(libc::EIO);
    signal_file_errno(message, name, errno)
}

fn bind_tcp_listener_socket(
    addr: SocketAddr,
    backlog: i32,
    options: &[NetworkSocketOptionSpec],
) -> Result<TcpListener, Flow> {
    let socket = Socket::new(tcp_socket_domain(addr), Type::STREAM, Some(Protocol::TCP))
        .map_err(|err| network_socket_io_error("Cannot create server socket", err))?;
    apply_network_socket_options(&socket, options)?;
    let sock_addr = SockAddr::from(addr);
    socket
        .bind(&sock_addr)
        .map_err(|err| network_socket_io_error("Cannot bind server socket", err))?;
    socket
        .listen(backlog)
        .map_err(|err| network_socket_io_error("Cannot listen on server socket", err))?;
    socket
        .set_nonblocking(true)
        .map_err(|err| network_socket_io_error("set_nonblocking", err))?;
    Ok(socket.into())
}

fn connect_tcp_stream_socket(
    addr: SocketAddr,
    options: &[NetworkSocketOptionSpec],
    contact: Value,
) -> Result<TcpStream, Flow> {
    let socket = Socket::new(tcp_socket_domain(addr), Type::STREAM, Some(Protocol::TCP))
        .map_err(|err| network_socket_io_error("Cannot create client socket", err))?;
    apply_network_socket_options(&socket, options)?;
    let sock_addr = SockAddr::from(addr);
    socket.connect(&sock_addr).map_err(|err| {
        network_socket_io_error_with_name("make client process failed", contact, err)
    })?;
    socket
        .set_nonblocking(true)
        .map_err(|err| network_socket_io_error("set_nonblocking", err))?;
    Ok(socket.into())
}

fn io_error_status_code(err: &std::io::Error) -> i32 {
    err.raw_os_error().unwrap_or(1)
}

fn start_nonblocking_tcp_stream_socket(
    addr: SocketAddr,
    options: &[NetworkSocketOptionSpec],
) -> Result<Result<TcpStream, std::io::Error>, Flow> {
    let socket = Socket::new(tcp_socket_domain(addr), Type::STREAM, Some(Protocol::TCP))
        .map_err(|err| network_socket_io_error("Cannot create client socket", err))?;
    apply_network_socket_options(&socket, options)?;
    socket
        .set_nonblocking(true)
        .map_err(|err| network_socket_io_error("set_nonblocking", err))?;
    let sock_addr = SockAddr::from(addr);
    match socket.connect(&sock_addr) {
        Ok(()) => Ok(Ok(socket.into())),
        Err(err) if sys::net::connect_is_pending(&err) => Ok(Ok(socket.into())),
        Err(err) => Ok(Err(err)),
    }
}

fn start_pending_tcp_stream_connect(
    addrs: Vec<SocketAddr>,
    options: &[NetworkSocketOptionSpec],
) -> Result<PendingNetworkConnectStart, Flow> {
    let mut last_error_code = libc::ECONNREFUSED;
    let mut iter = addrs.into_iter();
    while let Some(addr) = iter.next() {
        match start_nonblocking_tcp_stream_socket(addr, options)? {
            Ok(stream) => {
                return Ok(PendingNetworkConnectStart::Started(
                    PendingNetworkConnectStarted {
                        stream,
                        remote_addr: addr,
                        remaining_addrs: iter.collect(),
                    },
                ));
            }
            Err(err) => {
                last_error_code = io_error_status_code(&err);
            }
        }
    }
    Ok(PendingNetworkConnectStart::Failed(last_error_code))
}

fn bind_udp_socket(
    addr: SocketAddr,
    options: &[NetworkSocketOptionSpec],
) -> Result<UdpSocket, Flow> {
    let socket = Socket::new(tcp_socket_domain(addr), Type::DGRAM, Some(Protocol::UDP))
        .map_err(|err| network_socket_io_error("Cannot create datagram socket", err))?;
    apply_network_socket_options(&socket, options)?;
    let sock_addr = SockAddr::from(addr);
    socket
        .bind(&sock_addr)
        .map_err(|err| network_socket_io_error("Cannot bind datagram socket", err))?;
    socket
        .set_nonblocking(true)
        .map_err(|err| network_socket_io_error("set_nonblocking", err))?;
    Ok(socket.into())
}

fn udp_unspecified_addr_for(remote: SocketAddr) -> SocketAddr {
    match remote {
        SocketAddr::V4(_) => SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
        SocketAddr::V6(_) => SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0),
    }
}

fn datagram_zero_address_for(addr: SocketAddr) -> Value {
    let raw_len = match addr {
        SocketAddr::V4(_) => sys::net::sockaddr_in_payload_len(),
        SocketAddr::V6(_) => sys::net::sockaddr_in6_payload_len(),
    };
    Value::cons(Value::fixnum(0), int_vector(&vec![0_i64; raw_len]))
}

#[cfg(unix)]
fn datagram_zero_unix_address() -> Value {
    let raw_len = sys::net::sockaddr_un_payload_len();
    Value::cons(Value::fixnum(0), int_vector(&vec![0_i64; raw_len]))
}

fn bind_udp_client_socket(
    remote: SocketAddr,
    options: &[NetworkSocketOptionSpec],
) -> Result<UdpSocket, Flow> {
    bind_udp_socket(udp_unspecified_addr_for(remote), options)
}

fn network_socket_type_addrinfo_socktype(socket_type: NetworkSocketType) -> i32 {
    use dns_lookup::SockType;

    match socket_type {
        NetworkSocketType::Stream => SockType::Stream.into(),
        NetworkSocketType::Datagram => SockType::DGram.into(),
        #[cfg(unix)]
        NetworkSocketType::Seqpacket => sys::net::sock_seqpacket(),
    }
}

fn network_addrinfo_error_detail(err: dns_lookup::LookupError) -> String {
    let io_error: std::io::Error = err.into();
    let detail = io_error.to_string();
    detail
        .strip_prefix("failed to lookup address information: ")
        .unwrap_or(&detail)
        .to_string()
}

fn network_addrinfo_item_error_detail(err: std::io::Error) -> String {
    err.to_string()
}

fn resolve_network_socket_addrs_raw(
    host: &str,
    port: u16,
    family: NetworkProcessFamily,
    socket_type: NetworkSocketType,
) -> Result<Vec<SocketAddr>, String> {
    use dns_lookup::AddrInfoHints;

    let normalized_host = host.split('\0').next().unwrap_or_default();
    let service = port.to_string();
    let hints = AddrInfoHints {
        address: family.addrinfo_family(),
        socktype: network_socket_type_addrinfo_socktype(socket_type),
        ..AddrInfoHints::default()
    };
    let iter = dns_lookup::getaddrinfo(Some(normalized_host), Some(&service), Some(hints))
        .map_err(network_addrinfo_error_detail)?;
    let mut addrs = Vec::new();
    for info in iter {
        let info = info.map_err(network_addrinfo_item_error_detail)?;
        addrs.push(info.sockaddr);
    }
    if addrs.is_empty() {
        Err("No address associated with hostname".to_string())
    } else {
        Ok(addrs)
    }
}

fn resolve_network_socket_addrs(
    host: &str,
    port: u16,
    family: NetworkProcessFamily,
    socket_type: NetworkSocketType,
) -> Result<Vec<SocketAddr>, Flow> {
    let normalized_host = host.split('\0').next().unwrap_or_default();
    resolve_network_socket_addrs_raw(host, port, family, socket_type).map_err(|detail| {
        signal(
            "error",
            vec![Value::string(format!("{normalized_host}/{port} {detail}"))],
        )
    })
}

fn start_async_network_dns_lookup(
    host: String,
    port: u16,
    family: NetworkProcessFamily,
    socket_type: NetworkSocketType,
    socket_options: Vec<NetworkSocketOptionSpec>,
    notifier: Option<WaitNotifier>,
) -> PendingDnsRequest {
    let (sender, receiver) = mpsc::channel();
    let ready = Arc::new(AtomicBool::new(false));
    if hostname_fails_without_dns_lookup(&host) {
        let _ = sender.send(Err("Name or service not known".to_string()));
        ready.store(true, Ordering::Release);
        return PendingDnsRequest {
            host,
            receiver,
            ready,
            socket_options,
        };
    }
    let thread_ready = Arc::clone(&ready);
    let thread_host = host.clone();
    std::thread::spawn(move || {
        let result = resolve_network_socket_addrs_raw(&thread_host, port, family, socket_type);
        let _ = sender.send(result);
        thread_ready.store(true, Ordering::Release);
        if let Some(notifier) = notifier
            && let Err(error) = notifier.notify()
        {
            tracing::error!(%error, "failed to wake evaluator after asynchronous DNS lookup");
        }
    });
    PendingDnsRequest {
        host,
        receiver,
        ready,
        socket_options,
    }
}

fn hostname_fails_without_dns_lookup(host: &str) -> bool {
    let host = host.split('\0').next().unwrap_or_default();
    if host.is_empty() || host.len() > 253 || host.chars().any(char::is_whitespace) {
        return true;
    }
    let host = host.strip_suffix('.').unwrap_or(host);
    if host.is_empty() {
        return false;
    }
    host.split('.').any(|label| {
        label.is_empty() || label.len() > 63 || label.starts_with('-') || label.ends_with('-')
    })
}

fn nowait_tcp_immediate_addrs(
    host_value: Value,
    host: &str,
    port: u16,
    family: NetworkProcessFamily,
) -> Option<Vec<SocketAddr>> {
    if host_value.is_nil() || host_value.as_symbol_name() == Some("local") || host == "localhost" {
        let ip = family.loopback_host().parse::<IpAddr>().ok()?;
        return Some(vec![SocketAddr::new(ip, port)]);
    }
    let ip = host.parse::<IpAddr>().ok()?;
    let family_matches = !matches!(
        (family, ip),
        (NetworkProcessFamily::Ipv4, IpAddr::V6(_)) | (NetworkProcessFamily::Ipv6, IpAddr::V4(_))
    );
    family_matches.then_some(vec![SocketAddr::new(ip, port)])
}

fn bind_udp_socket_host(
    host: &str,
    port: u16,
    family: NetworkProcessFamily,
    options: &[NetworkSocketOptionSpec],
) -> Result<UdpSocket, Flow> {
    let mut last_error = None;
    for addr in resolve_network_socket_addrs(host, port, family, NetworkSocketType::Datagram)? {
        match bind_udp_socket(addr, options) {
            Ok(socket) => return Ok(socket),
            Err(err) => last_error = Some(err),
        }
    }
    Err(last_error.unwrap_or_else(|| {
        signal(
            LispCondition::FileError,
            vec![Value::string("Cannot bind datagram socket")],
        )
    }))
}

fn connect_udp_socket_host(
    host: &str,
    port: u16,
    family: NetworkProcessFamily,
    options: &[NetworkSocketOptionSpec],
) -> Result<(UdpSocket, SocketAddr), Flow> {
    let mut last_error = None;
    for addr in resolve_network_socket_addrs(host, port, family, NetworkSocketType::Datagram)? {
        match bind_udp_client_socket(addr, options) {
            Ok(socket) => return Ok((socket, addr)),
            Err(err) => last_error = Some(err),
        }
    }
    Err(last_error.unwrap_or_else(|| {
        signal(
            LispCondition::FileError,
            vec![Value::string("make datagram process failed")],
        )
    }))
}

fn bind_tcp_listener_host(
    host: &str,
    port: u16,
    family: NetworkProcessFamily,
    backlog: i32,
    options: &[NetworkSocketOptionSpec],
) -> Result<TcpListener, Flow> {
    let mut last_error = None;
    for addr in resolve_network_socket_addrs(host, port, family, NetworkSocketType::Stream)? {
        match bind_tcp_listener_socket(addr, backlog, options) {
            Ok(listener) => return Ok(listener),
            Err(err) => last_error = Some(err),
        }
    }
    Err(last_error.unwrap_or_else(|| {
        signal(
            LispCondition::FileError,
            vec![Value::string("Cannot bind server socket")],
        )
    }))
}

fn connect_tcp_stream_host(
    host: &str,
    port: u16,
    family: NetworkProcessFamily,
    options: &[NetworkSocketOptionSpec],
    contact: Value,
) -> Result<TcpStream, Flow> {
    let mut last_error = None;
    for addr in resolve_network_socket_addrs(host, port, family, NetworkSocketType::Stream)? {
        match connect_tcp_stream_socket(addr, options, contact) {
            Ok(stream) => return Ok(stream),
            Err(err) => last_error = Some(err),
        }
    }
    Err(last_error.unwrap_or_else(|| {
        signal(
            LispCondition::FileError,
            vec![Value::string("make client process failed")],
        )
    }))
}

fn tcp_server_socket_options(options: &[NetworkSocketOptionSpec]) -> Vec<NetworkSocketOptionSpec> {
    let mut effective = options.to_vec();
    if !network_socket_options_include(&effective, NetworkSocketOption::Reuseaddr) {
        effective.push(NetworkSocketOptionSpec {
            keyword: ProcessKeyword::Reuseaddr,
            option: NetworkSocketOption::Reuseaddr,
            value: Value::T,
        });
    }
    effective
}

#[cfg(unix)]
fn bind_unix_listener_socket(
    path: &Path,
    backlog: i32,
    options: &[NetworkSocketOptionSpec],
) -> Result<UnixListener, Flow> {
    let socket = Socket::new(Domain::UNIX, Type::STREAM, None)
        .map_err(|err| network_socket_io_error("Cannot create server socket", err))?;
    apply_network_socket_options(&socket, options)?;
    let sock_addr = SockAddr::unix(path)
        .map_err(|err| network_socket_io_error("Cannot bind server socket", err))?;
    socket
        .bind(&sock_addr)
        .map_err(|err| network_socket_io_error("Cannot bind server socket", err))?;
    socket
        .listen(backlog)
        .map_err(|err| network_socket_io_error("Cannot listen on server socket", err))?;
    socket
        .set_nonblocking(true)
        .map_err(|err| network_socket_io_error("set_nonblocking", err))?;
    Ok(socket.into())
}

#[cfg(unix)]
fn connect_unix_stream_socket(
    path: &Path,
    options: &[NetworkSocketOptionSpec],
) -> Result<UnixStream, Flow> {
    let socket = Socket::new(Domain::UNIX, Type::STREAM, None)
        .map_err(|err| network_socket_io_error("Cannot create client socket", err))?;
    apply_network_socket_options(&socket, options)?;
    let sock_addr = SockAddr::unix(path)
        .map_err(|err| network_socket_io_error("make client process failed", err))?;
    socket
        .connect(&sock_addr)
        .map_err(|err| network_socket_io_error("make client process failed", err))?;
    socket
        .set_nonblocking(true)
        .map_err(|err| network_socket_io_error("set_nonblocking", err))?;
    Ok(socket.into())
}

#[cfg(unix)]
fn start_nonblocking_unix_stream_socket(
    path: &Path,
    options: &[NetworkSocketOptionSpec],
) -> Result<Result<UnixStream, std::io::Error>, Flow> {
    let socket = Socket::new(Domain::UNIX, Type::STREAM, None)
        .map_err(|err| network_socket_io_error("Cannot create client socket", err))?;
    apply_network_socket_options(&socket, options)?;
    socket
        .set_nonblocking(true)
        .map_err(|err| network_socket_io_error("set_nonblocking", err))?;
    let sock_addr = SockAddr::unix(path)
        .map_err(|err| network_socket_io_error("make client process failed", err))?;
    match socket.connect(&sock_addr) {
        Ok(()) => Ok(Ok(socket.into())),
        Err(err) if sys::net::connect_is_pending(&err) => Ok(Ok(socket.into())),
        Err(err) => Ok(Err(err)),
    }
}

#[cfg(unix)]
fn bind_unix_seqpacket_listener_socket(
    path: &Path,
    backlog: i32,
    options: &[NetworkSocketOptionSpec],
) -> Result<Socket, Flow> {
    let socket = Socket::new(Domain::UNIX, Type::SEQPACKET, None)
        .map_err(|err| network_socket_io_error("Cannot create server socket", err))?;
    apply_network_socket_options(&socket, options)?;
    let sock_addr = SockAddr::unix(path)
        .map_err(|err| network_socket_io_error("Cannot bind server socket", err))?;
    socket
        .bind(&sock_addr)
        .map_err(|err| network_socket_io_error("Cannot bind server socket", err))?;
    socket
        .listen(backlog)
        .map_err(|err| network_socket_io_error("Cannot listen on server socket", err))?;
    socket
        .set_nonblocking(true)
        .map_err(|err| network_socket_io_error("set_nonblocking", err))?;
    Ok(socket)
}

#[cfg(unix)]
fn connect_unix_seqpacket_socket(
    path: &Path,
    options: &[NetworkSocketOptionSpec],
) -> Result<Socket, Flow> {
    let socket = Socket::new(Domain::UNIX, Type::SEQPACKET, None)
        .map_err(|err| network_socket_io_error("Cannot create client socket", err))?;
    apply_network_socket_options(&socket, options)?;
    let sock_addr = SockAddr::unix(path)
        .map_err(|err| network_socket_io_error("make client process failed", err))?;
    socket
        .connect(&sock_addr)
        .map_err(|err| network_socket_io_error("make client process failed", err))?;
    socket
        .set_nonblocking(true)
        .map_err(|err| network_socket_io_error("set_nonblocking", err))?;
    Ok(socket)
}

#[cfg(unix)]
fn bind_unix_datagram_socket(
    path: &Path,
    options: &[NetworkSocketOptionSpec],
) -> Result<UnixDatagram, Flow> {
    let socket = Socket::new(Domain::UNIX, Type::DGRAM, None)
        .map_err(|err| network_socket_io_error("Cannot create datagram socket", err))?;
    apply_network_socket_options(&socket, options)?;
    let sock_addr = SockAddr::unix(path)
        .map_err(|err| network_socket_io_error("Cannot bind datagram socket", err))?;
    socket
        .bind(&sock_addr)
        .map_err(|err| network_socket_io_error("Cannot bind datagram socket", err))?;
    socket
        .set_nonblocking(true)
        .map_err(|err| network_socket_io_error("set_nonblocking", err))?;
    Ok(socket.into())
}

#[cfg(unix)]
fn unbound_unix_datagram_socket(options: &[NetworkSocketOptionSpec]) -> Result<UnixDatagram, Flow> {
    let socket = Socket::new(Domain::UNIX, Type::DGRAM, None)
        .map_err(|err| network_socket_io_error("Cannot create datagram socket", err))?;
    apply_network_socket_options(&socket, options)?;
    socket
        .set_nonblocking(true)
        .map_err(|err| network_socket_io_error("set_nonblocking", err))?;
    Ok(socket.into())
}

impl ProcessManager {
    fn register_readable_source(
        poller: &polling::Poller,
        source: impl polling::AsRawSource,
        id: ProcessId,
    ) -> Result<(), String> {
        // SAFETY: ProcessManager only registers descriptors owned by the
        // corresponding Process record.  `unregister_process_poll_sources`
        // removes every registered descriptor from this poller before the
        // Process drops or replaces the descriptor.
        unsafe {
            poller
                .add_with_mode(
                    source,
                    polling::Event::readable(id as usize),
                    polling::PollMode::Level,
                )
                .map_err(|e| format!("Failed to register socket: {e}"))
        }
    }

    fn register_writable_source(
        poller: &polling::Poller,
        source: impl polling::AsRawSource,
        id: ProcessId,
    ) -> Result<(), String> {
        // SAFETY: ProcessManager only registers descriptors owned by the
        // corresponding Process record.  `unregister_process_poll_sources`
        // removes every registered descriptor from this poller before the
        // Process drops or replaces the descriptor.
        unsafe {
            poller
                .add_with_mode(
                    source,
                    polling::Event::writable(id as usize),
                    polling::PollMode::Level,
                )
                .map_err(|e| format!("Failed to register socket: {e}"))
        }
    }

    fn modify_poll_source(
        poller: &polling::Poller,
        source: impl polling::AsSource,
        event: polling::Event,
    ) -> Result<(), String> {
        poller
            .modify_with_mode(source, event, polling::PollMode::Level)
            .map_err(|e| format!("Failed to modify process fd interest: {e}"))
    }

    #[cfg(unix)]
    fn register_readable_raw_fd(
        poller: &polling::Poller,
        fd: std::os::unix::io::RawFd,
        id: ProcessId,
    ) -> Result<(), String> {
        // SAFETY: `fd` is borrowed from a process-owned descriptor that
        // remains alive until `unregister_process_poll_sources` removes it
        // from the poller.
        let borrowed = unsafe { std::os::unix::io::BorrowedFd::borrow_raw(fd) };
        Self::register_readable_source(poller, &borrowed, id)
    }

    #[cfg(unix)]
    fn register_writable_raw_fd(
        poller: &polling::Poller,
        fd: std::os::unix::io::RawFd,
        id: ProcessId,
    ) -> Result<(), String> {
        let borrowed = unsafe { std::os::unix::io::BorrowedFd::borrow_raw(fd) };
        Self::register_writable_source(poller, &borrowed, id)
    }

    #[cfg(unix)]
    fn modify_raw_fd_interest(
        poller: &polling::Poller,
        fd: std::os::unix::io::RawFd,
        event: polling::Event,
    ) -> Result<(), String> {
        let borrowed = unsafe { std::os::unix::io::BorrowedFd::borrow_raw(fd) };
        Self::modify_poll_source(poller, borrowed, event)
    }

    #[cfg(unix)]
    fn register_child_stdout_with_poller(
        poller: &polling::Poller,
        stdout: &ChildOutputReader,
        id: ProcessId,
    ) {
        use std::os::unix::io::AsRawFd;
        let fd = stdout.as_raw_fd();
        // Set non-blocking before registering.
        let _ = sys::set_fd_nonblocking(fd);
        // Use process id as the event key so we know which process is ready.
        let _ = Self::register_readable_raw_fd(poller, fd, id);
    }

    #[cfg(not(unix))]
    fn register_child_stdout_with_poller(
        _poller: &polling::Poller,
        _stdout: &ChildOutputReader,
        _id: ProcessId,
    ) {
        // GNU Emacs does not pass Windows subprocess pipe handles to Winsock
        // select.  Its w32 layer uses a reader thread plus event objects.  Until
        // Neomacs has the same backend, child pipe output is serviced by the
        // regular non-blocking wait pass instead of the socket poller.
    }

    #[cfg(unix)]
    fn unregister_child_stdout_from_poller(poller: &polling::Poller, stdout: &ChildOutputReader) {
        use std::os::unix::io::AsRawFd;
        let fd = stdout.as_raw_fd();
        let borrowed = unsafe { std::os::unix::io::BorrowedFd::borrow_raw(fd) };
        let _ = poller.delete(borrowed);
    }

    #[cfg(not(unix))]
    fn unregister_child_stdout_from_poller(_poller: &polling::Poller, _stdout: &ChildOutputReader) {
        // See `register_child_stdout_with_poller`.
    }

    /// Mirror GNU `set_process_filter_masks`: Lisp filter `t` removes only the
    /// process's read interest, leaving child-status and write sources active.
    /// Resuming the filter restores read interest without consuming bytes that
    /// accumulated while suspended.
    fn set_process_output_read_interest(&self, id: ProcessId, enabled: bool) {
        let Some(poller) = self.wait_backend.poller() else {
            return;
        };
        let Some(proc) = self.processes.get(&id) else {
            return;
        };

        if let Some(stdout) = proc.live_io.child_stdout.as_ref() {
            if enabled {
                Self::register_child_stdout_with_poller(poller, stdout, id);
            } else {
                Self::unregister_child_stdout_from_poller(poller, stdout);
            }
            return;
        }
        let wants_write = proc.input_disposition == ProcessInputDisposition::Connected
            && (proc.live_io.pending_network_connect.is_some() || !proc.write_queue.is_nil());
        let event = match (enabled, wants_write) {
            (true, true) => Some(polling::Event::all(id as usize)),
            (true, false) => Some(polling::Event::readable(id as usize)),
            (false, true) => Some(polling::Event::writable(id as usize)),
            (false, false) => None,
        };

        if let Some(tls) = proc.live_io.tls_stream.as_ref() {
            match event {
                Some(event) => {
                    if Self::modify_poll_source(poller, tls.tcp_stream(), event).is_err() {
                        let _ = Self::register_readable_source(poller, tls.tcp_stream(), id);
                        let _ = Self::modify_poll_source(poller, tls.tcp_stream(), event);
                    }
                }
                None => {
                    let _ = poller.delete(tls.tcp_stream());
                }
            }
            return;
        }
        if let Some(socket) = proc.live_io.network_socket.as_ref() {
            match event {
                Some(event) => {
                    if socket.modify_interest(poller, id, event).is_err() {
                        let _ = socket.register_readable(poller, id);
                        let _ = socket.modify_interest(poller, id, event);
                    }
                }
                None => socket.unregister_readable(poller),
            }
            return;
        }
        if let Some(port) = proc.live_io.serial_port.as_ref() {
            match event {
                Some(event) => {
                    if port.modify_interest(poller, event).is_err() {
                        let _ = port.register_readable(poller, id);
                        let _ = port.modify_interest(poller, event);
                    }
                }
                None => port.unregister(poller),
            }
            return;
        }
        #[cfg(unix)]
        if let Some(master) = proc
            .live_io
            .pty_master
            .as_ref()
            .and_then(|master| master.as_raw_fd())
        {
            match event {
                Some(event) => {
                    if Self::modify_raw_fd_interest(poller, master, event).is_err() {
                        let _ = Self::register_readable_raw_fd(poller, master, id);
                        let _ = Self::modify_raw_fd_interest(poller, master, event);
                    }
                }
                None => {
                    let borrowed = unsafe { std::os::unix::io::BorrowedFd::borrow_raw(master) };
                    let _ = poller.delete(borrowed);
                }
            }
        }
    }

    #[cfg(unix)]
    fn register_child_stdin_writable_with_poller(
        poller: &polling::Poller,
        stdin: &std::process::ChildStdin,
        id: ProcessId,
    ) {
        use std::os::unix::io::AsRawFd;
        let fd = stdin.as_raw_fd();
        let _ = sys::set_fd_nonblocking(fd);
        let _ = Self::register_writable_raw_fd(poller, fd, id);
    }

    #[cfg(not(unix))]
    fn register_child_stdin_writable_with_poller(
        _poller: &polling::Poller,
        _stdin: &std::process::ChildStdin,
        _id: ProcessId,
    ) {
        // Windows subprocess stdin is not integrated into the poller yet.
    }

    #[cfg(unix)]
    fn unregister_child_stdin_writable_from_poller(
        poller: &polling::Poller,
        stdin: &std::process::ChildStdin,
    ) {
        use std::os::unix::io::AsRawFd;
        let fd = stdin.as_raw_fd();
        let borrowed = unsafe { std::os::unix::io::BorrowedFd::borrow_raw(fd) };
        let _ = poller.delete(borrowed);
    }

    #[cfg(not(unix))]
    fn unregister_child_stdin_writable_from_poller(
        _poller: &polling::Poller,
        _stdin: &std::process::ChildStdin,
    ) {
        // See `register_child_stdin_writable_with_poller`.
    }

    fn unregister_process_poll_sources(poller: Option<&polling::Poller>, proc: &Process) {
        let Some(poller) = poller else {
            return;
        };

        if let Some(stdout) = proc.live_io.child_stdout.as_ref() {
            Self::unregister_child_stdout_from_poller(poller, stdout);
        }
        if let Some(stdin) = proc.live_io.child.stdin() {
            Self::unregister_child_stdin_writable_from_poller(poller, stdin);
        }
        if let Some(status_source) = proc.live_io.child_status_source.as_ref() {
            status_source.unregister_from_poller(poller);
        }
        if let Some(tls) = proc.live_io.tls_stream.as_ref() {
            let _ = poller.delete(tls.tcp_stream());
        }
        if let Some(socket) = proc.live_io.network_socket.as_ref() {
            socket.unregister_readable(poller);
        }
        if let Some(port) = proc.live_io.serial_port.as_ref() {
            port.unregister(poller);
        }
        #[cfg(unix)]
        if let Some(master) = proc
            .live_io
            .pty_master
            .as_ref()
            .and_then(|master| master.as_raw_fd())
        {
            let borrowed = unsafe { std::os::unix::io::BorrowedFd::borrow_raw(master) };
            let _ = poller.delete(borrowed);
        }
    }

    /// GNU `deactivate_process` translated into the Rust ownership model.
    ///
    /// Poll registrations borrow native descriptors, so unregister them
    /// before dropping their single aggregate owner.  Durable Lisp identity,
    /// status, callbacks, and captured output remain on `Process`.
    fn deactivate_process_io(poller: Option<&polling::Poller>, proc: &mut Process) {
        Self::unregister_process_poll_sources(poller, proc);
        drop(std::mem::take(&mut proc.live_io));
        #[cfg(windows)]
        {
            proc.stderr_pipe_owner_status_deferred_at = None;
        }
        proc.gnutls_initstage = GnutlsInitStage::Empty;
        proc.gnutls_boot_parameters = Value::NIL;
    }

    fn deactivate_terminal_process_io(&mut self, id: ProcessId) {
        if let Some(proc) = self.processes.get_mut(&id) {
            Self::deactivate_process_io(self.wait_backend.poller(), proc);
        }
    }

    /// GNU `deactivate_process` (src/process.c:4812), dispatched on what this
    /// port's `Process::live_io` actually holds.  See [`ProcessIoTeardown`].
    fn apply_process_io_teardown(&mut self, id: ProcessId, teardown: ProcessIoTeardown) {
        match teardown {
            ProcessIoTeardown::Terminal => self.deactivate_terminal_process_io(id),
            ProcessIoTeardown::Network => self.deactivate_network_process_io(id),
        }
    }

    pub fn new() -> Self {
        Self {
            processes: HashMap::new(),
            deleted_processes: HashMap::new(),
            next_id: 1,
            process_tick: 0,
            default_read_config: ProcessReadConfig::default(),
            env_overrides: HashMap::new(),
            wait_backend: ProcessWaitBackend::new(),
        }
    }

    fn set_default_read_config(&mut self, config: ProcessReadConfig) {
        self.default_read_config = config;
    }

    pub(crate) fn adaptive_read_timeout(&self) -> Option<Duration> {
        self.processes
            .values()
            .filter(|process| process.adaptive_read_buffering != 0)
            .filter_map(|process| {
                (!process.read_output_delay.is_zero()).then_some(process.read_output_delay)
            })
            .min()
            .map(|delay| delay.min(Duration::from_millis(READ_OUTPUT_DELAY_MAX_MS)))
    }

    fn clear_adaptive_read_skip_if_needed(&mut self, id: ProcessId) -> bool {
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

    /// Create a new process record.  Returns the process id.
    pub(crate) fn create_process(
        &mut self,
        name: String,
        buffer: Value,
        command: String,
        args: Vec<String>,
        coding: ProcessCodingSystems,
    ) -> ProcessId {
        self.create_process_lisp(
            LispString::from_utf8(&name),
            buffer,
            LispString::from_utf8(&command),
            args.into_iter()
                .map(|arg| LispString::from_utf8(&arg))
                .collect(),
            coding,
        )
    }

    pub(crate) fn create_process_lisp(
        &mut self,
        name: LispString,
        buffer: Value,
        command: LispString,
        args: Vec<LispString>,
        coding: ProcessCodingSystems,
    ) -> ProcessId {
        self.create_process_with_kind_lisp(
            name,
            buffer,
            command,
            args,
            ProcessKindWithoutDevice::Real,
            coding,
        )
    }

    pub(crate) fn create_process_lisp_resolved(
        &mut self,
        name: LispString,
        buffer: Value,
        command: LispString,
        args: Vec<LispString>,
        executable: Option<LispString>,
        coding: ProcessCodingSystems,
    ) -> ProcessId {
        self.create_process_with_kind_lisp_resolved(
            name,
            buffer,
            command,
            args,
            ProcessKindWithoutDevice::Real,
            executable,
            coding,
        )
    }

    /// Create a new process record with an explicit process kind.
    pub(crate) fn create_process_with_kind(
        &mut self,
        name: String,
        buffer: Value,
        command: String,
        args: Vec<String>,
        kind: ProcessKindWithoutDevice,
        coding: ProcessCodingSystems,
    ) -> ProcessId {
        self.create_process_with_kind_lisp(
            LispString::from_utf8(&name),
            buffer,
            LispString::from_utf8(&command),
            args.into_iter()
                .map(|arg| LispString::from_utf8(&arg))
                .collect(),
            kind,
            coding,
        )
    }

    pub(crate) fn create_process_with_kind_lisp(
        &mut self,
        name: LispString,
        buffer: Value,
        command: LispString,
        args: Vec<LispString>,
        kind: ProcessKindWithoutDevice,
        coding: ProcessCodingSystems,
    ) -> ProcessId {
        self.create_process_with_kind_lisp_resolved(name, buffer, command, args, kind, None, coding)
    }

    pub(crate) fn create_process_with_kind_lisp_resolved(
        &mut self,
        name: LispString,
        buffer: Value,
        command: LispString,
        args: Vec<LispString>,
        kind: ProcessKindWithoutDevice,
        executable: Option<LispString>,
        coding: ProcessCodingSystems,
    ) -> ProcessId {
        self.create_process_record(name, buffer, command, args, kind.into(), executable, coding)
    }

    /// GNU `Fmake_serial_process`'s record, which cannot exist without the
    /// device `serial_open` returned (src/process.c:3207-3217: `make_process`,
    /// then `serial_open`, then `p->infd = fd; p->outfd = fd`, with the whole thing
    /// under `record_unwind_protect (remove_process, proc)`).
    ///
    /// Taking the [`sys::SerialPort`] by value is the whole point: there is no
    /// other way to reach `ProcessKind::Serial`, and there is no other way to
    /// obtain a `SerialPort` than to have opened one.
    pub(crate) fn create_serial_process(
        &mut self,
        name: LispString,
        buffer: Value,
        port: sys::SerialPort,
        coding: ProcessCodingSystems,
    ) -> ProcessId {
        let id = self.create_process_record(
            name,
            buffer,
            LispString::from_utf8("serial"),
            Vec::new(),
            ProcessKind::Serial,
            None,
            coding,
        );
        if let Some(proc) = self.processes.get_mut(&id) {
            proc.live_io.serial_port = Some(port);
            // GNU's `Fmake_serial_process` stores `open`, not `run`
            // (`process-status` on a live serial port is `open`, measured).
            proc.status = ProcessStatusSymbol::Open.value();
        }
        id
    }

    fn create_process_record(
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
        let id = self.next_id;
        self.next_id += 1;
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
            #[cfg(windows)]
            stderr_pipe_owner_status_deferred_at: None,
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
            mark: super::marker::make_marker_value(None, None, false),
            default_directory: None,
        };
        register_process_print_name(id, &process_name_runtime(proc.name));
        self.processes.insert(id, proc);
        id
    }

    fn allocate_process_name(&self, requested: LispString) -> LispString {
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

    fn process_name_is_in_use(&self, candidate: &LispString) -> bool {
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

    /// Spawn an OS child process for a tracked process record.
    ///
    /// When `use_pty` is true (and on Unix), the child is spawned on a
    /// pseudo-terminal via `portable-pty`. Otherwise the traditional
    /// pipe-based `std::process::Command` path is used.
    pub fn spawn_child(&mut self, id: ProcessId, use_pty: bool) -> Result<(), String> {
        self.spawn_child_with_environment(id, use_pty, None)
            .map(|_| ())
    }

    pub(crate) fn spawn_child_with_environment(
        &mut self,
        id: ProcessId,
        use_pty: bool,
        child_environment: Option<super::environment::ChildEnvironment>,
    ) -> Result<ChildSpawnOutcome, String> {
        let proc = self
            .processes
            .get_mut(&id)
            .ok_or_else(|| "Process not found".to_string())?;

        if proc.live_io.child.has_child() {
            return Ok(ChildSpawnOutcome::Spawned); // Already spawned
        }

        // Don't spawn non-real processes
        if proc.kind != ProcessKind::Real {
            return Ok(ChildSpawnOutcome::Spawned);
        }

        let Some(argv) = process_spawn_lisp_argv(proc) else {
            return Ok(ChildSpawnOutcome::Spawned); // No program to run
        };
        if argv.is_empty()
            || argv[0].as_bytes().is_empty()
            || env_var_name_bytes_eq(argv[0].as_bytes(), b"nil")
        {
            return Ok(ChildSpawnOutcome::Spawned);
        }

        // Collect env overrides into a temporary Vec so we don't borrow
        // `self` across the mutable `proc` borrow below.
        let env_overrides: Vec<(LispString, Option<LispString>)> = self
            .env_overrides
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        // PTY path (Unix only).
        #[cfg(unix)]
        if use_pty {
            return self.spawn_child_pty(id, child_environment.as_ref(), &env_overrides);
        }

        // Pipe path (all platforms, or when use_pty is false).
        self.spawn_child_pipe(id, child_environment.as_ref(), &env_overrides)
    }

    /// Pipe-based child spawn (traditional stdin/stdout/stderr pipes).
    fn spawn_child_pipe(
        &mut self,
        id: ProcessId,
        child_environment: Option<&super::environment::ChildEnvironment>,
        env_overrides: &[(LispString, Option<LispString>)],
    ) -> Result<ChildSpawnOutcome, String> {
        let proc = self
            .processes
            .get(&id)
            .ok_or_else(|| "Process not found".to_string())?;

        let Some(argv) = process_spawn_lisp_argv(proc) else {
            return Ok(ChildSpawnOutcome::Spawned);
        };
        if argv.is_empty() {
            return Ok(ChildSpawnOutcome::Spawned);
        }

        // GNU's `create_process` sends stdout and stderr to one pipe unless
        // `:stderr` names a separate pipe-process.  Preserve that OS-level
        // topology: a shared pipe keeps stdout/stderr write ordering and, more
        // importantly, keeps a live reader so a child writing stderr cannot
        // die from SIGPIPE.
        let requested_stderr_pipe_id = process_value_to_id(&proc.stderrproc);
        let default_directory = proc.default_directory.clone();
        let _ = proc;
        let mut stderr_transfer = self.take_stderr_pipe_writer(id, requested_stderr_pipe_id);

        let argv_os = argv
            .iter()
            .map(lisp_string_to_os_string)
            .collect::<Vec<OsString>>();

        let mut cmd = crate::emacs_core::callproc::new_child_command(&argv_os[0]);
        cmd.args(&argv_os[1..]);
        cmd.stdin(Stdio::piped());
        let shared_output_reader = if let Some(writer) = stderr_transfer.writer() {
            let child_writer = writer
                .try_clone()
                .map_err(|error| format!("Failed to duplicate stderr pipe: {error}"))?;
            cmd.stdout(Stdio::piped());
            cmd.stderr(child_writer);
            None
        } else {
            let (reader, writer) = os_pipe::pipe()
                .map_err(|error| format!("Failed to create child output pipe: {error}"))?;
            let stderr_writer = writer
                .try_clone()
                .map_err(|error| format!("Failed to duplicate child output pipe: {error}"))?;
            cmd.stdout(writer);
            cmd.stderr(stderr_writer);
            Some(reader)
        };
        if let Some(dir) = &default_directory {
            cmd.current_dir(dir);
        }

        if let Some(environment) = child_environment {
            environment.apply_to_command(&mut cmd);
        }

        for (key, val) in env_overrides {
            let key_str = lisp_string_to_os_string(key);
            match val {
                Some(v) => {
                    let v_str = lisp_string_to_os_string(v);
                    cmd.env(&key_str, &v_str);
                }
                None => {
                    cmd.env_remove(&key_str);
                }
            }
        }

        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(e) => {
                if let Some(proc) = stderr_transfer.processes.get_mut(id) {
                    proc.status = process_status_exit_value(1);
                }
                return Err(format!("Failed to start process: {}", e));
            }
        };
        stderr_transfer.commit();

        // GNU records the child's real OS pid (create_process sets
        // p->pid = pid). `std::process::Child::id` exposes it as a `u32`.
        let os_pid = Some(child.id());
        let child_status_source = os_pid.and_then(ChildStatusSource::open);

        let stdout = match shared_output_reader {
            Some(reader) => Some(ChildOutputReader::Shared(reader)),
            None => child.stdout.take().map(ChildOutputReader::Stdout),
        };

        // Register stdout with the poller where the platform exposes child
        // pipe descriptors as pollable sources.
        if stderr_transfer
            .processes
            .get(id)
            .is_none_or(process_filter_accepts_output)
            && let (Some(poller), Some(stdout)) = (stderr_transfer.wait_backend.poller(), &stdout)
        {
            Self::register_child_stdout_with_poller(poller, stdout, id);
        }
        if let Some(status_source) = child_status_source.as_ref() {
            status_source.register_with_poller(stderr_transfer.wait_backend.poller(), id);
        }

        if let Some(proc) = stderr_transfer.processes.get_mut(id) {
            proc.live_io.child_stdout = stdout;
            proc.os_pid = os_pid;
            proc.live_io.child_status_source = child_status_source;
            proc.live_io.child = ChildOwnership::of_pipe_child(child);
            proc.status = process_status_run_value();
            // Pipe-mode processes don't have a real TTY.
            proc.tty_name = Value::NIL;
            proc.tty_stdin = false;
            proc.tty_stdout = false;
            proc.tty_stderr = false;
        }

        Ok(ChildSpawnOutcome::Spawned)
    }

    fn take_stderr_pipe_writer(
        &mut self,
        main_id: ProcessId,
        stderr_pipe_id: Option<ProcessId>,
    ) -> StderrPipeWriterTransfer<'_> {
        let transferable_id = stderr_pipe_id.filter(|stderr_id| {
            *stderr_id != main_id
                && self
                    .processes
                    .get(stderr_id)
                    .is_some_and(|proc| proc.kind == ProcessKind::Pipe)
        });
        let writer = transferable_id
            .and_then(|stderr_id| self.processes.get_mut(&stderr_id))
            .and_then(|stderr_proc| {
                #[cfg(windows)]
                {
                    stderr_proc.stderr_pipe_owner_status_deferred_at = None;
                }
                stderr_proc.live_io.module_pipe_writer.take()
            });
        let transferred_id = writer.as_ref().and(transferable_id);
        StderrPipeWriterTransfer {
            processes: self,
            owner_id: main_id,
            stderr_pipe_id: transferred_id,
            writer,
            committed: false,
        }
    }

    /// PTY-based child spawn via `portable-pty`.
    ///
    /// The child is attached to a pseudo-terminal. The master side provides
    /// a single combined I/O stream (PTY merges stdout and stderr) — UNLESS a
    /// separate stderr pipe-process is requested (`make-process :stderr`), in
    /// which case stdout stays on the PTY but stderr is routed to a dedicated
    /// pipe, exactly as GNU's `create_process` wires `forkin`/`forkout` to the
    /// pty and `forkerr` to the stderr pipe-process independently.
    #[cfg(unix)]
    fn spawn_child_pty(
        &mut self,
        id: ProcessId,
        child_environment: Option<&super::environment::ChildEnvironment>,
        env_overrides: &[(LispString, Option<LispString>)],
    ) -> Result<ChildSpawnOutcome, String> {
        let proc = self
            .processes
            .get_mut(&id)
            .ok_or_else(|| "Process not found".to_string())?;

        let rows = proc.window_rows.unwrap_or(24) as u16;
        let cols = proc.window_cols.unwrap_or(80) as u16;
        let requested_stderr_pipe_id = process_value_to_id(&proc.stderrproc);
        let default_directory = proc.default_directory.clone();
        let argv = process_spawn_lisp_argv(proc);
        // Release the `proc` borrow: the rest of this function reads other
        // process records (the stderr pipe-process) and re-borrows `id`.
        let _ = proc;

        // A separate stderr pipe-process (make-process :stderr) is wired here as
        // GNU does: stdout uses the PTY, stderr uses an independent pipe.  When
        // none is requested the PTY merges stdout and stderr as before.
        let mut stderr_transfer = self.take_stderr_pipe_writer(id, requested_stderr_pipe_id);

        let pty_system = portable_pty::native_pty_system();
        let pty_size = portable_pty::PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };
        let pty_pair = match pty_system.openpty(pty_size) {
            Ok(pair) => pair,
            Err(error) => return Err(format!("Failed to create PTY: {error}")),
        };

        let Some(argv) = argv else {
            return Ok(ChildSpawnOutcome::Spawned);
        };
        if argv.is_empty() {
            return Ok(ChildSpawnOutcome::Spawned);
        }

        let argv_os = argv
            .iter()
            .map(lisp_string_to_os_string)
            .collect::<Vec<OsString>>();

        // Obtain the TTY name from the master (which knows the slave path).
        let tty_name_path = pty_pair.master.tty_name();
        let tty_name = tty_name_path
            .as_ref()
            .map(|p| Value::heap_string(os_str_to_lisp_string(p.as_os_str())))
            .unwrap_or(Value::NIL);
        if let Some(tty_path) = tty_name_path.as_ref() {
            if let Err(error) = sys::configure_child_pty_tty(tty_path.as_os_str()) {
                return Err(format!("Failed to configure PTY child tty: {error}"));
            }
        }

        // GNU's `emacs_perror` names the program that could not be exec'd, so
        // keep it before `CommandBuilder::from_argv` consumes the argv.
        let program_os = argv_os[0].clone();
        let mut outcome = ChildSpawnOutcome::Spawned;

        // With a separate stderr pipe-process we cannot use portable_pty's
        // `spawn_command` (it hardwires the child's stdin/stdout/stderr all to
        // the PTY slave).  Instead spawn the child ourselves, dup'ing the PTY
        // slave onto stdin/stdout and leaving stderr on an OS pipe, mirroring
        // GNU's `emacs_spawn` where `std_err` is the separate `forkerr` fd and
        // only merges into `std_out` when no stderr pipe-process exists.
        if stderr_transfer.pipe_id().is_some() {
            let Some(tty_path) = tty_name_path.clone() else {
                return Err("PTY has no tty name for :stderr split spawn".to_string());
            };
            let mut cmd = crate::emacs_core::callproc::new_child_command(&argv_os[0]);
            cmd.args(&argv_os[1..]);
            let child_writer = stderr_transfer
                .writer()
                .expect("a transferred stderr pipe id always has its writer")
                .try_clone()
                .map_err(|error| format!("Failed to duplicate stderr pipe: {error}"))?;
            cmd.stderr(child_writer);
            if let Some(dir) = &default_directory {
                cmd.current_dir(dir);
            }
            if let Some(environment) = child_environment {
                environment.apply_to_command(&mut cmd);
            }
            for (key, val) in env_overrides {
                let key_str = lisp_string_to_os_string(key);
                match val {
                    Some(v) => {
                        cmd.env(&key_str, lisp_string_to_os_string(v));
                    }
                    None => {
                        cmd.env_remove(&key_str);
                    }
                }
            }
            // `new_child_command` already installs a `pre_exec` that calls
            // `setsid` (own session, no controlling tty).  Chain a second
            // `pre_exec` that opens the PTY slave by path and makes it the
            // controlling terminal on fds 0/1, leaving fd 2 (stderr) on the
            // pipe `Command` set up — exactly GNU's forkin/forkout=pty_tty,
            // forkerr=stderr-pipe arrangement.
            let tty_cstr = match std::ffi::CString::new(tty_path.as_os_str().as_bytes()) {
                Ok(path) => path,
                Err(_) => return Err("PTY tty name contains an interior NUL".to_string()),
            };
            // SAFETY: `pre_exec` runs in the forked child before exec; the closure
            // calls only `sys::establish_pty_controlling_terminal`, which is itself
            // restricted to async-signal-safe syscalls for exactly this context.
            unsafe {
                use std::os::unix::process::CommandExt;
                cmd.pre_exec(move || sys::establish_pty_controlling_terminal(&tty_cstr));
            }

            let child = match cmd.spawn() {
                Ok(child) => child,
                Err(error) => return Err(format!("Failed to spawn PTY child: {error}")),
            };
            stderr_transfer.commit();
            // GNU records the child's real OS pid (create_process sets p->pid).
            let os_pid = Some(child.id());
            let child_status_source = os_pid.and_then(ChildStatusSource::open);
            if let Some(status_source) = child_status_source.as_ref() {
                status_source.register_with_poller(stderr_transfer.wait_backend.poller(), id);
            }
            if let Some(proc) = stderr_transfer.processes.get_mut(id) {
                proc.os_pid = os_pid;
                proc.live_io.child_status_source = child_status_source;
                proc.live_io.child = ChildOwnership::of_pipe_child(child);
            }
        } else {
            let mut cmd = portable_pty::CommandBuilder::from_argv(argv_os);
            if let Some(dir) = &default_directory {
                cmd.cwd(dir);
            }
            if let Some(environment) = child_environment {
                environment.apply_to_pty_command(&mut cmd);
            }
            for (key, val) in env_overrides {
                let key_str = lisp_string_to_os_string(key);
                match val {
                    Some(v) => {
                        let v_str = lisp_string_to_os_string(v);
                        cmd.env(&key_str, &v_str);
                    }
                    None => {
                        cmd.env_remove(&key_str);
                    }
                }
            }

            match pty_pair.slave.spawn_command(cmd) {
                Ok(pty_child) => {
                    // GNU records the child's real OS pid; portable_pty exposes
                    // it via `Child::process_id`.
                    let os_pid = pty_child.process_id();
                    let child_status_source = os_pid.and_then(ChildStatusSource::open);
                    if let Some(status_source) = child_status_source.as_ref() {
                        status_source
                            .register_with_poller(stderr_transfer.wait_backend.poller(), id);
                    }
                    if let Some(proc) = stderr_transfer.processes.get_mut(id) {
                        proc.os_pid = os_pid;
                        proc.live_io.child_status_source = child_status_source;
                        proc.live_io.child = ChildOwnership::of_pty_child(pty_child);
                    }
                }
                Err(error) => {
                    // The exec failed.  GNU's forked child is still alive at
                    // this point and writes `emacs_perror`'s line to its own
                    // STDERR -- which here IS the pty -- before `_exit`ing
                    // (src/callproc.c:1206-1216).  There is no child to do it,
                    // so the parent writes the same bytes to the same tty, and
                    // the PTY master below is installed exactly as for a
                    // successful spawn: the reader finds the diagnostic and
                    // then EOF, and the caller supplies GNU's exit status.
                    let errno = error
                        .chain()
                        .find_map(|cause| cause.downcast_ref::<std::io::Error>())
                        .and_then(|io| io.raw_os_error())
                        .unwrap_or(libc::ENOENT);
                    if let Some(tty_path) = tty_name_path.as_ref() {
                        write_exec_failure_diagnostic_to_tty(tty_path, &program_os, errno);
                    }
                    outcome = ChildSpawnOutcome::ExecFailed(errno);
                }
            }
        }

        // Drop the slave end now that the child has it; otherwise the master
        // read never sees EOF after the child exits.
        drop(pty_pair.slave);

        let pty_read = pty_pair
            .master
            .try_clone_reader()
            .map_err(|e| format!("Failed to clone PTY reader: {}", e))?;
        let pty_write = pty_pair
            .master
            .take_writer()
            .map_err(|e| format!("Failed to take PTY writer: {}", e))?;

        // Register the PTY master fd with the poller for non-blocking I/O.
        if let Some(master_fd) = pty_pair.master.as_raw_fd() {
            // Set non-blocking on the master fd.
            let _ = sys::set_fd_nonblocking(master_fd);
            if stderr_transfer
                .processes
                .get(id)
                .is_none_or(process_filter_accepts_output)
                && let Some(poller) = stderr_transfer.wait_backend.poller()
            {
                let _ = Self::register_readable_raw_fd(poller, master_fd, id);
            }
        }

        let stderr_is_pty = stderr_transfer.pipe_id().is_none();
        if let Some(proc) = stderr_transfer.processes.get_mut(id) {
            proc.live_io.pty_master = Some(pty_pair.master);
            proc.live_io.pty_reader = Some(pty_read);
            proc.live_io.pty_writer = Some(Box::new(pty_write));
            proc.status = process_status_run_value();
            proc.tty_name = tty_name;
            proc.tty_stdin = true;
            proc.tty_stdout = true;
            // stderr is tty-backed only when it shares the PTY; with a separate
            // stderr pipe-process it is not (GNU's `Fprocess_tty_name` returns
            // nil for the stderr stream when `p->stderrproc` is set).
            proc.tty_stderr = stderr_is_pty;
        }

        Ok(outcome)
    }

    /// Poll one child-status transition and stage it for sentinel delivery.
    /// Returns true when the kernel reported stop, continue, exit, or signal.
    pub fn check_child_status_change(&mut self, id: ProcessId) -> bool {
        let Some(status) = self.poll_child_status_change(id) else {
            return false;
        };
        if process_status_is_terminal_for_notify(&status) {
            self.deactivate_child_status_source(id);
        }
        self.set_child_status_pending(id, status);
        true
    }

    pub(crate) fn defers_minimum_status_drain_after_output(&self, id: ProcessId) -> bool {
        self.get(id)
            .is_some_and(process_defers_pty_status_after_explicit_coding)
    }

    fn deactivate_child_status_source(&mut self, id: ProcessId) {
        if let Some(proc) = self.processes.get_mut(&id)
            && let Some(status_source) = proc.live_io.child_status_source.take()
            && let Some(poller) = self.wait_backend.poller()
        {
            status_source.unregister_from_poller(poller);
        }
    }

    fn poll_child_status_change(&mut self, id: ProcessId) -> Option<Value> {
        let proc = self.processes.get_mut(&id)?;

        // GNU keeps waiting on a stopped child so WCONTINUED (or a terminal
        // signal delivered while stopped) remains observable.  Only one
        // delivered transition may await sentinel publication at a time.
        let can_change_again = matches!(
            ProcessStatusSymbol::from_status_value(proc.status),
            Some(ProcessStatusSymbol::Run | ProcessStatusSymbol::Stop)
        );
        if proc.status_notify_pending || !can_change_again {
            return None;
        }

        // GNU's `child_status_changed (p->pid, &status, WUNTRACED |
        // WCONTINUED)` (src/process.c:7742).  The dispatch between the two
        // handle shapes, and the raw `waitpid` itself, live in
        // `process/reap.rs`, which is the only owner of either -- so a
        // terminal answer here is also the moment `p->alive` becomes 0 and
        // the pid stops being spellable.
        match proc.live_io.child.poll_status() {
            ChildStatusChange::NoChange | ChildStatusChange::Gone => None,
            ChildStatusChange::StillOurs(status) | ChildStatusChange::Reaped(status) => {
                Some(status)
            }
        }
    }

    fn set_child_status_pending(&mut self, id: ProcessId, status: Value) {
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

    fn stderr_pipe_owner(&self, stderr_id: ProcessId) -> Option<ProcessId> {
        self.processes
            .get(&stderr_id)
            .and_then(|stderr| stderr.live_io.stderr_pipe_owner)
    }

    fn clear_status_notify_pending(&mut self, id: ProcessId) {
        if let Some(proc) = self.processes.get_mut(&id) {
            proc.status_notify_pending = false;
            proc.pending_status = Value::NIL;
            #[cfg(windows)]
            {
                proc.stderr_pipe_owner_status_deferred_at = None;
            }
        }
    }

    /// Read available output from a child process's stdout.
    /// Returns the data read (may be empty if nothing available).
    fn read_child_stdout_result(
        &mut self,
        id: ProcessId,
        destination: ProcessOutputDestination,
        coding_systems: &crate::emacs_core::coding::CodingSystemManager,
    ) -> ProcessBytesRead {
        let Some(proc) = self.processes.get_mut(&id) else {
            return ProcessBytesRead::NoSource;
        };
        let read_len = process_read_buffer_len(proc);

        let Some(stdout) = proc.live_io.child_stdout.as_mut() else {
            return ProcessBytesRead::NoSource;
        };

        // Use non-blocking read via set_nonblocking on Unix
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let fd = stdout.as_raw_fd();
            // Set non-blocking
            let _ = sys::set_fd_nonblocking(fd);
        }

        let mut buf = vec![0u8; read_len];
        let full_read_len = buf.len();
        #[cfg(windows)]
        let result = {
            match peek_child_output_readiness(stdout) {
                Ok(Some(0)) => Err(std::io::Error::new(
                    std::io::ErrorKind::WouldBlock,
                    "child pipe has no data available",
                )),
                Ok(Some(available)) => stdout.read(&mut buf[..available.min(read_len)]),
                Ok(None) => Ok(0),
                Err(error) => Err(error),
            }
        };
        #[cfg(not(windows))]
        let result = stdout.read(&mut buf);
        let read = process_output_read_from_io_result(
            proc,
            coding_systems,
            destination,
            ProcessReadOutcome::from_stream_read(&result),
            &buf,
            full_read_len,
        );
        read
    }

    /// Read available output from a serial process's device.
    ///
    /// GNU has no separate path for this: `read_process_output` reads
    /// `p->infd`, which for a serial process is the descriptor `serial_open`
    /// returned (src/process.c:3212-3217).  The device was opened
    /// `O_NONBLOCK`, so an idle port is `WouldBlock` rather than a stall.
    fn read_serial_output_result(
        &mut self,
        id: ProcessId,
        destination: ProcessOutputDestination,
        coding_systems: &crate::emacs_core::coding::CodingSystemManager,
    ) -> ProcessBytesRead {
        let Some(proc) = self.processes.get_mut(&id) else {
            return ProcessBytesRead::NoSource;
        };
        let read_len = process_read_buffer_len(proc);
        let Some(port) = proc.live_io.serial_port.as_mut() else {
            return ProcessBytesRead::NoSource;
        };

        let mut buf = vec![0u8; read_len];
        let full_read_len = buf.len();
        let result = port.read(&mut buf);
        let read = process_output_read_from_io_result(
            proc,
            coding_systems,
            destination,
            ProcessReadOutcome::from_stream_read(&result),
            &buf,
            full_read_len,
        );
        read
    }

    /// Read available output from a PTY master reader.
    /// Returns the data read (may be empty if nothing available).
    /// PTY combines stdout and stderr into a single stream.
    fn read_pty_output_result(
        &mut self,
        id: ProcessId,
        destination: ProcessOutputDestination,
        coding_systems: &crate::emacs_core::coding::CodingSystemManager,
    ) -> ProcessBytesRead {
        let Some(proc) = self.processes.get_mut(&id) else {
            return ProcessBytesRead::NoSource;
        };
        let read_len = process_read_buffer_len(proc);
        let Some(reader) = proc.live_io.pty_reader.as_mut() else {
            return ProcessBytesRead::NoSource;
        };

        let mut buf = vec![0u8; read_len];
        let full_read_len = buf.len();
        let result = reader.read(&mut buf);
        let read = process_output_read_from_io_result(
            proc,
            coding_systems,
            destination,
            ProcessReadOutcome::from_pty_read(&result),
            &buf,
            full_read_len,
        );
        read
    }

    fn read_network_output_result(
        &mut self,
        id: ProcessId,
        destination: ProcessOutputDestination,
        coding_systems: &crate::emacs_core::coding::CodingSystemManager,
    ) -> ProcessBytesRead {
        let Some(proc) = self.processes.get_mut(&id) else {
            return ProcessBytesRead::NoSource;
        };

        let read_len = process_read_buffer_len(proc);
        if let Some(ref mut tls) = proc.live_io.tls_stream {
            let mut buf = vec![0u8; read_len];
            let full_read_len = buf.len();
            let result = tls.read_process_output(&mut buf);
            let read = process_output_read_from_io_result(
                proc,
                coding_systems,
                destination,
                ProcessReadOutcome::from_stream_read(&result),
                &buf,
                full_read_len,
            );
            return read;
        }

        if proc.live_io.network_socket.is_some() {
            enum RawNetworkRead {
                Stream(std::io::Result<usize>),
                Udp(std::io::Result<(usize, SocketAddr)>),
                #[cfg(unix)]
                UnixDatagram(std::io::Result<(usize, UnixSocketAddr)>),
                Unsupported,
            }

            let mut buf = vec![0u8; read_len];
            let full_read_len = buf.len();
            let raw_read = {
                let socket = proc.live_io.network_socket.as_mut().expect("checked above");
                match socket.read_stream_output(&mut buf) {
                    Some(result) => RawNetworkRead::Stream(result),
                    None => match socket {
                        NetworkSocket::UdpSocket(socket) => {
                            RawNetworkRead::Udp(socket.recv_from(&mut buf))
                        }
                        #[cfg(unix)]
                        NetworkSocket::UnixDatagram(socket) => {
                            RawNetworkRead::UnixDatagram(socket.recv_from(&mut buf))
                        }
                        _ => RawNetworkRead::Unsupported,
                    },
                }
            };
            let read = match raw_read {
                RawNetworkRead::Stream(result) => process_output_read_from_io_result(
                    proc,
                    coding_systems,
                    destination,
                    ProcessReadOutcome::from_stream_read(&result),
                    &buf,
                    full_read_len,
                ),
                RawNetworkRead::Udp(result) => match result {
                    Ok((n, addr)) => {
                        update_process_adaptive_read_buffering(proc, n, n == full_read_len);
                        proc.datagram_socket_addr = Some(addr);
                        proc.datagram_address = socket_addr_to_lisp_value(addr);
                        process_run_from_bytes(proc, coding_systems, destination, &buf[..n])
                    }
                    Err(ref err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        ProcessBytesRead::WouldBlock
                    }
                    Err(_) => ProcessBytesRead::Eof,
                },
                #[cfg(unix)]
                RawNetworkRead::UnixDatagram(result) => match result {
                    Ok((n, addr)) => {
                        update_process_adaptive_read_buffering(proc, n, n == full_read_len);
                        if let Some(path) = addr.as_pathname() {
                            let path = path.to_path_buf();
                            proc.datagram_unix_path = Some(path.clone());
                            proc.datagram_address = Value::heap_string(
                                crate::emacs_core::fileio::path_to_lisp_file_name(&path),
                            );
                        }
                        process_run_from_bytes(proc, coding_systems, destination, &buf[..n])
                    }
                    Err(ref err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        ProcessBytesRead::WouldBlock
                    }
                    Err(_) => ProcessBytesRead::Eof,
                },
                RawNetworkRead::Unsupported => ProcessBytesRead::WouldBlock,
            };
            return read;
        }

        ProcessBytesRead::NoSource
    }

    pub(crate) fn wait_for_process_events(
        &self,
        timeout: std::time::Duration,
    ) -> ProcessWaitEvents {
        if let Some(events) =
            self.wait_for_backend_events(timeout, ProcessWaitBackendInterest::ProcessesOnly)
        {
            return events;
        }

        // No poller available — sleep fallback
        std::thread::sleep(timeout.min(std::time::Duration::from_millis(10)));
        ProcessWaitEvents::ready_processes(self.live_process_ids())
    }

    pub(crate) fn has_wait_notification_backend(&self) -> bool {
        self.wait_backend.has_notifications()
    }

    /// Cross-platform handle for producers to wake a blocked wait after
    /// publishing work. `None` if no poller could be created.
    pub(crate) fn wait_notifier(&self) -> Option<WaitNotifier> {
        self.wait_backend.notify_handle()
    }

    /// Block on the unified wait poller (cross-thread notification and/or process
    /// fds, per `interest`) until something is ready or `timeout` elapses. This is
    /// the single GNU-`pselect`-style primitive the wait loop blocks on; see
    /// `Context::block_for_wait_request`.
    pub(crate) fn wait_for_backend_events(
        &self,
        timeout: std::time::Duration,
        interest: ProcessWaitBackendInterest,
    ) -> Option<ProcessWaitEvents> {
        self.wait_backend
            .wait_for_events(&self.processes, timeout, interest)
    }

    fn deactivate_network_process_io(&mut self, id: ProcessId) {
        if let Some(proc) = self.processes.get_mut(&id) {
            Self::unregister_process_poll_sources(self.wait_backend.poller(), proc);
            proc.live_io.tls_stream = None;
            proc.live_io.network_socket = None;
            proc.gnutls_initstage = GnutlsInitStage::Empty;
        }
    }

    /// Tear down a stderr pipe-process's readable I/O once its source EOFs.
    /// Removes the stderr fd from the poller and drops it so the descriptor is
    /// closed and the process stops being polled.
    fn deactivate_stderr_pipe_process_io(&mut self, id: ProcessId) {
        if let Some(proc) = self.processes.get_mut(&id) {
            Self::unregister_process_poll_sources(self.wait_backend.poller(), proc);
            proc.live_io.child_stdout = None;
        }
    }

    /// Retire a pipe process whose read end reached EOF: GNU's fd-loop half of
    /// the split at src/process.c:6072-6080.
    ///
    ///     else if (nread == 0 && PIPECONN_P (proc))
    ///       {
    ///         XPROCESS (proc)->tick = ++process_tick;
    ///         deactivate_process (proc);
    ///         if (EQ (XPROCESS (proc)->status, Qrun))
    ///           pset_status (XPROCESS (proc), list2 (Qexit, make_fixnum (0)));
    ///       }
    ///
    /// The status changes HERE, so `process-status` reports `closed` (a pipe
    /// maps `exit` to `closed`, src/process.c:1193) from this moment on; the
    /// SENTINEL is not run here. GNU runs it from `status_notify`, which the fd
    /// loop calls only once it has finished scanning, and which walks the alist
    /// newest-first so the owner of an implicit `:stderr` pipe -- created after
    /// it -- is always notified first. Marking the notification pending is what
    /// hands the sentinel to that later pass, and what keeps the process in the
    /// alist until then.
    ///
    /// Doing the two halves together here was the bug behind ledger entry 54:
    /// a sentinel that kills the stderr buffer saw the pipe either still `open`
    /// (EOF discarded) or already gone (sentinel run and reaped inline), never
    /// GNU's `closed`.
    fn retire_pipe_process_at_read_eof(&mut self, id: ProcessId) {
        self.deactivate_stderr_pipe_process_io(id);
        let mut changed = false;
        if let Some(proc) = self.processes.get_mut(&id)
            && !process_status_is_terminal_for_notify(&proc.status)
        {
            let terminal = process_status_exit_value(0);
            proc.status = terminal;
            proc.pending_status = terminal;
            proc.status_notify_pending = true;
            changed = true;
        }
        if changed {
            // GNU's `XPROCESS (proc)->tick = ++process_tick;` on the line
            // above its `deactivate_process` (src/process.c:6075).
            self.record_status_change(StatusChangeSite::PipeConnectionReadEof, id);
        }
    }

    /// GNU's non-EOF read failure path at `wait_reading_process_output`
    /// (src/process.c:6081-6090): close the failed read side and report exit
    /// status 256 when no already-observed child status supersedes it.
    fn retire_process_at_read_failure(&mut self, id: ProcessId) {
        let source = self.processes.get(&id).and_then(process_output_source);
        match source {
            Some(ProcessOutputSource::ChildStdout) => {
                if let Some(proc) = self.processes.get_mut(&id) {
                    if let (Some(poller), Some(stdout)) = (
                        self.wait_backend.poller(),
                        proc.live_io.child_stdout.as_ref(),
                    ) {
                        Self::unregister_child_stdout_from_poller(poller, stdout);
                    }
                    proc.live_io.child_stdout = None;
                }
            }
            Some(ProcessOutputSource::Pty) => self.deactivate_pty_process_read_io(id),
            Some(ProcessOutputSource::Serial) => {
                if let Some(proc) = self.processes.get_mut(&id) {
                    Self::unregister_process_poll_sources(self.wait_backend.poller(), proc);
                    proc.live_io.serial_port = None;
                }
            }
            Some(ProcessOutputSource::Network) | None => {}
        }

        let Some(proc) = self.processes.get_mut(&id) else {
            return;
        };
        if !proc.pending_status.is_nil() {
            proc.status = proc.pending_status;
        }
        if process_status_is_run(&proc.status)
            || ProcessStatusSymbol::from_status_value(proc.status)
                == Some(ProcessStatusSymbol::Open)
        {
            let failed = process_status_exit_value(256);
            proc.status = failed;
            proc.pending_status = failed;
        } else {
            proc.pending_status = proc.status;
        }
        proc.status_notify_pending = true;
        // GNU's `XPROCESS (proc)->tick = ++process_tick;` at src/process.c:6084,
        // on the line above the same `deactivate_process`.
        self.record_status_change(StatusChangeSite::SubprocessReadFailure, id);
    }

    /// GNU `read_process_output`: EOF/EIO on a real subprocess PTY removes the
    /// read fd, but does not make the process terminal; SIGCHLD/status
    /// notification observes child death later.
    fn deactivate_pty_process_read_io(&mut self, id: ProcessId) {
        if let Some(proc) = self.processes.get_mut(&id) {
            #[cfg(unix)]
            if let (Some(poller), Some(master)) = (
                self.wait_backend.poller(),
                proc.live_io
                    .pty_master
                    .as_ref()
                    .and_then(|master| master.as_raw_fd()),
            ) {
                let borrowed = unsafe { std::os::unix::io::BorrowedFd::borrow_raw(master) };
                let _ = poller.delete(borrowed);
            }
            proc.live_io.pty_reader = None;
        }
    }

    /// Kill (remove) a process by id.  Returns true if found.
    pub fn kill_process(&mut self, id: ProcessId) -> bool {
        if let Some(proc) = self.processes.get_mut(&id) {
            Self::unregister_process_poll_sources(self.wait_backend.poller(), proc);
            kill_real_process_child(proc, signal_kill_number());
            proc.live_io.tls_stream.take();
            proc.gnutls_initstage = GnutlsInitStage::Empty;
            proc.gnutls_boot_parameters = Value::NIL;
            proc.live_io.network_socket.take();
            proc.status = process_status_signal_value(signal_kill_number());
            proc.live_io.child_status_source = None;
            true
        } else {
            false
        }
    }

    /// GNU `Fdelete_process`'s stamping half (src/process.c:1123-1150): kill the
    /// child if it is still alive, settle the terminal status, and close the
    /// channels -- everything EXCEPT taking the process out of the process
    /// list.  GNU leaves that to `status_notify`'s `delete-exited-processes`
    /// decision (:7926-7929) and to its own trailing `remove_process` (:1153),
    /// which is why a `delete-process` sentinel still sees its process listed
    /// when the flag is nil.
    ///
    /// Returns whether the id named a live process.
    fn stamp_process_for_delete(&mut self, id: ProcessId) -> bool {
        let Some(proc) = self.processes.get_mut(&id) else {
            return false;
        };
        // GNU's FIRST statement here is `p->raw_status_new = 0;`
        // (src/process.c:1123): the record is thrown away BEFORE anything
        // reads it, unconditionally, so a status the SIGCHLD handler recorded
        // before `delete-process` was called is NOT what `delete-process`
        // reports.  `if (p->raw_status_new) update_status (p);` at :1141 can
        // therefore only be true for a record that arrived AFTER :1123 -- one
        // that `record_kill_process`'s own SIGKILL produced.
        //
        // This port used to ADOPT the pending status here, which read as
        // GNU's :1141 and was not: the arm was unreachable with a pre-existing
        // record until ledger 193's trigger made one, and it then answered
        // `exit`/7 where GNU answers `signal`/9.  `UpdateStatusSite::
        // DeleteProcess`'s docstring predicted exactly that.
        proc.status_notify_pending = false;
        proc.pending_status = Value::NIL;
        let site = if matches!(
            proc.kind,
            ProcessKind::Network | ProcessKind::Pipe | ProcessKind::Serial
        ) {
            if !process_status_is_terminal_for_notify(&proc.status) {
                proc.status = process_status_exit_value(0);
            }
            StatusChangeSite::DeleteProcessConnection
        } else if !process_status_is_exit_or_signal(&proc.status) {
            // GNU: `if (p->alive) record_kill_process (p, Qnil);` (:1134-1135),
            // then `if (! (EQ (symbol, Qsignal) || EQ (symbol, Qexit)))
            // pset_status (p, list2 (Qsignal, make_fixnum (SIGKILL)));`
            // (:1146-1147).  The kill is a no-op on a reaped child since
            // ledger 187, because `pid_if_unreaped` has no pid to give.
            kill_real_process_child(proc, signal_kill_number());
            proc.status = process_status_signal_value(signal_kill_number());
            StatusChangeSite::DeleteProcessChild
        } else {
            StatusChangeSite::DeleteProcessChild
        };
        wait_for_real_process_child_termination(proc);
        proc.status_notify_pending = false;
        proc.pending_status = Value::NIL;
        Self::deactivate_process_io(self.wait_backend.poller(), proc);
        // GNU's two `p->tick = ++process_tick;` here (:1128 for the connection
        // arm, :1148 for the subprocess arm) are each followed IMMEDIATELY by
        // `status_notify (p, NULL)` (:1129, :1149), so the tick they move is
        // consumed in the same call and no later walk ever sees it.  This port
        // runs that notification from `delete_process_running_its_sentinel`,
        // whose `settle_status_and_retire` marks the tick notified -- see
        // `StatusChangeNotifier::SynchronouslyAtTheSite`.
        self.record_status_change(site, id);
        true
    }

    /// Delete a process entirely: GNU `Fdelete_process` for a caller that runs
    /// no sentinel of its own -- the stamping half above plus the
    /// unconditional `remove_process` at src/process.c:1153.
    pub fn delete_process(&mut self, id: ProcessId) -> bool {
        if self.stamp_process_for_delete(id) {
            self.reap_exited_process(id);
            true
        } else {
            self.deleted_processes.contains_key(&id)
        }
    }

    pub(crate) fn process_ids_for_buffer(&self, buffer_id: BufferId) -> Vec<ProcessId> {
        let buffer = Value::make_buffer(buffer_id);
        self.processes
            .iter()
            .filter_map(|(id, proc)| (proc.buffer == buffer).then_some(*id))
            .collect()
    }

    pub(crate) fn hangup_real_process_for_buffer_kill(&mut self, id: ProcessId) -> bool {
        let Some(proc) = self.processes.get_mut(&id) else {
            return false;
        };
        if proc.kind != ProcessKind::Real || process_status_is_exit_or_signal(&proc.status) {
            return false;
        }
        // GNU `kill_buffer_processes` only SENDS the hangup
        // (`process_send_signal (proc, SIGHUP, …)`); the status becomes
        // `(signal . SIGHUP)` when the child's death is actually observed
        // (SIGCHLD/waitpid), and the sentinel runs inside the next wait's
        // `status_notify`. Synthesizing a pending terminal status here made
        // `process-status` report `signal` before the child had even died —
        // action must never write status (unlike `delete-process`, whose
        // synchronous `(signal . SIGKILL)` stamp IS GNU behavior,
        // process.c:1145).
        kill_real_process_child(proc, signal_hup_number());
        true
    }

    /// GNU `remove_process` for an already-terminated process (called from
    /// `status_notify` when `delete-exited-processes' is non-nil): drop the
    /// process from the live process table (so `get-process'/`process-list' no
    /// longer return it) while keeping the object reachable for bindings that
    /// still hold its value.  Unlike `delete_process`, this does NOT kill or
    /// re-stamp the child — it has already exited and its recorded terminal
    /// status (exit/signal) must be preserved for `process-status' on the value.
    pub fn reap_exited_process(&mut self, id: ProcessId) {
        if let Some(mut proc) = self.processes.remove(&id) {
            Self::deactivate_process_io(self.wait_backend.poller(), &mut proc);
            self.deleted_processes.insert(id, proc);
        }
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

    /// Get a mutable process by id from either live or stale process tables.
    pub fn get_any_mut(&mut self, id: ProcessId) -> Option<&mut Process> {
        if self.processes.contains_key(&id) {
            self.processes.get_mut(&id)
        } else {
            self.deleted_processes.get_mut(&id)
        }
    }

    pub(crate) fn open_channel_for_module(&self, process: Value) -> Result<std::ffi::c_int, Flow> {
        let id = resolve_process_object_or_wrong_type_any_in_manager(self, &process)?;
        let proc = self
            .get_any(id)
            .ok_or_else(|| signal_wrong_type_processp(process))?;
        if proc.kind != ProcessKind::Pipe {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("pipe-process-p"), process],
            ));
        }
        let writer = proc.live_io.module_pipe_writer.as_ref().ok_or_else(|| {
            signal(
                "error",
                vec![Value::string("Pipe process has no writable channel")],
            )
        })?;
        duplicate_module_pipe_writer(writer).ok_or_else(|| {
            signal(
                LispCondition::FileError,
                vec![Value::string("Cannot duplicate file descriptor")],
            )
        })
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
        id > 0 && id < self.next_id
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
    pub fn find_by_buffer_id(&self, buffer_id: crate::buffer::BufferId) -> Option<ProcessId> {
        self.processes
            .values()
            .find(|p| p.buffer.as_buffer_id() == Some(buffer_id))
            .map(|p| p.id)
    }

    /// Queue input for a process and try to flush it once.
    pub fn send_input(&mut self, id: ProcessId, input: &LispString) -> Result<bool, Flow> {
        if !self.queue_input(id, input)? {
            return Ok(false);
        }
        let _ = self.flush_process_write_queue(id)?;
        Ok(true)
    }

    fn queue_input(&mut self, id: ProcessId, input: &LispString) -> Result<bool, Flow> {
        let Some(proc) = self.processes.get_mut(&id) else {
            return Ok(false);
        };
        proc.write_queue =
            write_queue_push(proc.write_queue, Value::heap_string(input.clone()), false);
        Ok(true)
    }

    fn write_queue_is_empty(&self, id: ProcessId) -> bool {
        self.processes
            .get(&id)
            .is_none_or(|proc| proc.write_queue.is_nil())
    }

    fn flush_process_write_queue(&mut self, id: ProcessId) -> Result<ProcessWriteFlush, Flow> {
        if self.write_queue_is_empty(id) {
            self.update_process_write_interest(id, ProcessWriteInterest::Readable);
            return Ok(ProcessWriteFlush::Drained);
        }

        loop {
            let Some(entry) = self.pop_process_write_queue(id) else {
                self.update_process_write_interest(id, ProcessWriteInterest::Readable);
                return Ok(ProcessWriteFlush::Drained);
            };
            if entry.len == 0 {
                continue;
            }
            let Some(bytes) = entry.bytes() else {
                continue;
            };
            if bytes.is_empty() {
                continue;
            }

            match self.write_process_input_once(id, &bytes)? {
                ProcessWriteAttempt::Written(0) | ProcessWriteAttempt::WouldBlock => {
                    self.push_process_write_queue_entry(id, entry, true);
                    self.update_process_write_interest(
                        id,
                        ProcessWriteInterest::ReadableAndWritable,
                    );
                    return Ok(ProcessWriteFlush::Blocked);
                }
                ProcessWriteAttempt::Written(n) if n < bytes.len() => {
                    self.push_process_write_queue_entry(id, entry.advance(n), true);
                    continue;
                }
                ProcessWriteAttempt::Written(_) => continue,
                ProcessWriteAttempt::NoSource => {
                    // `Disconnected` is GNU's permanent `outfd == -1`, not a
                    // transient would-block condition. Retaining this entry
                    // (and every later send) can only grow an unreachable
                    // queue before the Lisp layer reports the closed output
                    // descriptor. Keep the legacy no-endpoint harness behavior
                    // for `Connected`, whose source may be attached later.
                    if self.processes.get(&id).is_some_and(|proc| {
                        proc.input_disposition == ProcessInputDisposition::Disconnected
                    }) {
                        if let Some(proc) = self.processes.get_mut(&id) {
                            proc.write_queue = Value::NIL;
                        }
                    } else {
                        self.push_process_write_queue_entry(id, entry, true);
                    }
                    return Ok(ProcessWriteFlush::NoSource);
                }
            }
        }
    }

    fn pop_process_write_queue(&mut self, id: ProcessId) -> Option<ProcessWriteQueueEntry> {
        let proc = self.processes.get_mut(&id)?;
        let (queue, entry) = write_queue_pop(proc.write_queue);
        proc.write_queue = queue;
        entry
    }

    fn push_process_write_queue_entry(
        &mut self,
        id: ProcessId,
        entry: ProcessWriteQueueEntry,
        front: bool,
    ) {
        if let Some(proc) = self.processes.get_mut(&id) {
            proc.write_queue = write_queue_push_entry(proc.write_queue, entry, front);
        }
    }

    fn write_process_input_once(
        &mut self,
        id: ProcessId,
        bytes: &[u8],
    ) -> Result<ProcessWriteAttempt, Flow> {
        let Some(proc) = self.processes.get_mut(&id) else {
            return Ok(ProcessWriteAttempt::NoSource);
        };

        match proc.input_disposition {
            ProcessInputDisposition::Discard => {
                return Ok(ProcessWriteAttempt::Written(bytes.len()));
            }
            ProcessInputDisposition::Disconnected => return Ok(ProcessWriteAttempt::NoSource),
            ProcessInputDisposition::Connected => {}
        };
        let Some(endpoint) = ProcessInputEndpoint::select(proc) else {
            return Ok(ProcessWriteAttempt::NoSource);
        };
        let Some(result) = endpoint.write_once(proc, bytes) else {
            return Ok(ProcessWriteAttempt::NoSource);
        };

        match result {
            Ok(n) => {
                if n > 0 {
                    reset_adaptive_read_delay_after_process_write(proc);
                }
                Ok(ProcessWriteAttempt::Written(n))
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                Ok(ProcessWriteAttempt::WouldBlock)
            }
            Err(err) if err.kind() == std::io::ErrorKind::BrokenPipe => {
                // GNU `send_process`'s EPIPE arm (src/process.c:6941-6947):
                //
                //   close_process_fd (&p->open_fd[WRITE_TO_SUBPROCESS]);
                //   p->outfd = -1;
                //   error ("Process %s no longer connected to pipe; closed it", ...);
                //
                // and NOTHING else -- the status is not touched, so the child
                // stays sweepable and the next wait's `status_notify` hands
                // the sentinel the child's REAL exit status.  Writing
                // `(exit . 256)` here (introduced by GNU commit
                // 4d7e6e51dd4, 2012, and removed by e381cf1fc97, 2025) made
                // the status terminal before the reap, which no later sweep
                // may overwrite
                // (`SweepableChild::of` admits only `run`/`stop`) and which
                // set no `status_notify_pending` -- so the sentinel never
                // ran.  Measured with lsp-mode against a crashing nixd:
                // `lsp--handle-process-exit` never fired, the dead
                // workspace stayed in the session, and every later send
                // reported "not running: exited abnormally with code 256".
                let name = process_name_runtime(proc.name);
                endpoint.disconnect(self.wait_backend.poller(), proc);
                let error = signal(
                    "error",
                    vec![Value::string(format!(
                        "Process {name} no longer connected to pipe; closed it"
                    ))],
                );
                self.update_process_write_interest(id, ProcessWriteInterest::Readable);
                Err(error)
            }
            Err(err) => Err(signal_process_io("Writing to process", None, err)),
        }
    }

    /// Register a network socket with the I/O poller so that
    /// `wait_for_output` wakes up when data arrives.
    pub fn register_socket_fd(&self, id: ProcessId) -> Result<(), String> {
        let proc = self.processes.get(&id).ok_or("Process not found")?;
        if !process_filter_accepts_output(proc) {
            return Ok(());
        }
        if let Some(poller) = self.wait_backend.poller() {
            if let Some(tls) = proc.live_io.tls_stream.as_ref() {
                Self::register_readable_source(poller, tls.tcp_stream(), id)?;
                return Ok(());
            }

            let socket = proc.live_io.network_socket.as_ref().ok_or("No socket")?;
            socket.register_readable(poller, id)?;
        }
        Ok(())
    }

    /// GNU `Fmake_serial_process`'s `add_process_read_fd (fd)`,
    /// src/process.c:3241-3243, which is guarded exactly as this is:
    /// `if (!EQ (p->command, Qt) && !EQ (p->filter, Qt))` -- a `:stop t` port
    /// and a port whose filter is `t` are opened and configured but not polled.
    pub(crate) fn register_serial_read_fd(&self, id: ProcessId) {
        let Some(poller) = self.wait_backend.poller() else {
            return;
        };
        let Some(proc) = self.processes.get(&id) else {
            return;
        };
        if process_stopped_for_io(proc) || !process_filter_accepts_output(proc) {
            return;
        }
        if let Some(port) = proc.live_io.serial_port.as_ref() {
            let _ = port.register_readable(poller, id);
        }
    }

    pub fn register_socket_writable_fd(&self, id: ProcessId) -> Result<(), String> {
        let proc = self.processes.get(&id).ok_or("Process not found")?;
        if let Some(poller) = self.wait_backend.poller() {
            let socket = proc.live_io.network_socket.as_ref().ok_or("No socket")?;
            socket.register_writable(poller, id)?;
        }
        Ok(())
    }

    fn update_process_write_interest(&self, id: ProcessId, interest: ProcessWriteInterest) {
        let Some(poller) = self.wait_backend.poller() else {
            return;
        };
        let Some(proc) = self.processes.get(&id) else {
            return;
        };

        if let Some(stdin) = proc.live_io.child.stdin() {
            match interest {
                ProcessWriteInterest::Readable => {
                    Self::unregister_child_stdin_writable_from_poller(poller, stdin);
                }
                ProcessWriteInterest::ReadableAndWritable => {
                    Self::register_child_stdin_writable_with_poller(poller, stdin, id);
                }
            }
            return;
        }

        let accepts_output = process_filter_accepts_output(proc);
        let _ = proc;
        self.set_process_output_read_interest(id, accepts_output);
    }

    fn update_tcp_client_contact(
        proc: &mut Process,
        remote_addr: SocketAddr,
        local_addr: Option<SocketAddr>,
    ) -> Result<(), Flow> {
        proc.childp = process_contact_plist_put(
            proc.childp,
            ProcessKeyword::Remote.value(),
            socket_addr_to_lisp_value(remote_addr),
        )?;
        if let Some(local_addr) = local_addr {
            proc.childp = process_contact_plist_put(
                proc.childp,
                ProcessKeyword::Local.value(),
                socket_addr_to_lisp_value(local_addr),
            )?;
        }
        Ok(())
    }

    fn start_next_pending_network_connect(
        &mut self,
        id: ProcessId,
        addrs: Vec<SocketAddr>,
        options: &[NetworkSocketOptionSpec],
    ) -> Result<Option<i32>, Flow> {
        let start = start_pending_tcp_stream_connect(addrs, options)?;
        let started = match start {
            PendingNetworkConnectStart::Started(started) => started,
            PendingNetworkConnectStart::Failed(code) => return Ok(Some(code)),
        };
        let local_addr = started.stream.local_addr().ok();
        if let Some(proc) = self.processes.get_mut(&id) {
            proc.live_io.network_socket = Some(NetworkSocket::TcpStream(started.stream));
            proc.live_io.pending_network_connect = Some(PendingNetworkConnect::Tcp {
                remaining_addrs: started.remaining_addrs,
                socket_options: options.to_vec(),
            });
            proc.status = process_status_connect_value();
            Self::update_tcp_client_contact(proc, started.remote_addr, local_addr)?;
        }
        self.register_socket_writable_fd(id).ok();
        Ok(None)
    }

    fn complete_pending_network_connect(
        &mut self,
        id: ProcessId,
    ) -> Result<PendingNetworkConnectCompletion, Flow> {
        let Some(proc) = self.processes.get(&id) else {
            return Ok(PendingNetworkConnectCompletion::None);
        };
        if proc.live_io.pending_network_connect.is_none() {
            return Ok(PendingNetworkConnectCompletion::None);
        }
        if proc
            .live_io
            .pending_network_connect
            .as_ref()
            .is_some_and(|pending| matches!(pending, PendingNetworkConnect::Dns(_)))
        {
            return self.complete_pending_dns_network_connect(id);
        }
        let connect_error = proc
            .live_io
            .network_socket
            .as_ref()
            .and_then(NetworkSocket::take_pending_connect_error)
            .transpose()
            .map_err(|err| signal_process_io("Checking network connection", None, err))?
            .flatten();

        if let Some(err) = connect_error {
            let pending = self
                .processes
                .get_mut(&id)
                .and_then(|proc| proc.live_io.pending_network_connect.take());
            let Some(pending) = pending else {
                return Ok(PendingNetworkConnectCompletion::None);
            };
            if let Some(proc) = self.processes.get(&id)
                && let Some(socket) = proc.live_io.network_socket.as_ref()
                && let Some(poller) = self.wait_backend.poller()
            {
                socket.unregister_readable(poller);
            }
            let code = io_error_status_code(&err);
            match pending {
                PendingNetworkConnect::Tcp {
                    remaining_addrs,
                    socket_options,
                } if !remaining_addrs.is_empty() => {
                    return match self.start_next_pending_network_connect(
                        id,
                        remaining_addrs,
                        &socket_options,
                    )? {
                        None => Ok(PendingNetworkConnectCompletion::Retrying),
                        Some(code) => {
                            let sentinel = self
                                .processes
                                .get(&id)
                                .map(|proc| proc.sentinel)
                                .unwrap_or(Value::NIL);
                            if let Some(proc) = self.processes.get_mut(&id) {
                                proc.status = process_status_failed_value(code);
                                proc.live_io.network_socket = None;
                                proc.live_io.pending_network_connect = None;
                            }
                            // GNU src/process.c:6141.
                            self.record_status_change(
                                StatusChangeSite::NonBlockingConnectFailed,
                                id,
                            );
                            Ok(PendingNetworkConnectCompletion::Failed { sentinel, code })
                        }
                    };
                }
                _ => {}
            }

            let sentinel = self
                .processes
                .get(&id)
                .map(|proc| proc.sentinel)
                .unwrap_or(Value::NIL);
            if let Some(proc) = self.processes.get_mut(&id) {
                proc.status = process_status_failed_value(code);
                proc.live_io.network_socket = None;
                proc.live_io.pending_network_connect = None;
            }
            // GNU src/process.c:6141, the arm with no addrinfos left.
            self.record_status_change(StatusChangeSite::NonBlockingConnectFailed, id);
            return Ok(PendingNetworkConnectCompletion::Failed { sentinel, code });
        }

        let sentinel = self
            .processes
            .get(&id)
            .map(|proc| proc.sentinel)
            .unwrap_or(Value::NIL);
        if let Some(proc) = self.processes.get(&id)
            && let Some(socket) = proc.live_io.network_socket.as_ref()
            && let Some(poller) = self.wait_backend.poller()
        {
            socket.unregister_readable(poller);
        }
        if let Some(proc) = self.processes.get_mut(&id) {
            proc.live_io.pending_network_connect = None;
            proc.status = process_status_run_value();
        }
        let tls_parameters = self
            .processes
            .get(&id)
            .map(|proc| proc.gnutls_boot_parameters)
            .filter(|parameters| !parameters.is_nil())
            .map(parse_make_network_tls_parameters)
            .transpose()?
            .flatten();
        if let Some(parameters) = tls_parameters {
            upgrade_process_to_tls::<RustlsBackend>(
                self,
                id,
                &parameters.client,
                "make-network-process",
                signal_gnutls_boot_error,
            )?;
        }
        self.register_socket_fd(id).ok();
        Ok(PendingNetworkConnectCompletion::Connected { sentinel })
    }

    fn complete_pending_dns_network_connect(
        &mut self,
        id: ProcessId,
    ) -> Result<PendingNetworkConnectCompletion, Flow> {
        let Some(proc) = self.processes.get(&id) else {
            return Ok(PendingNetworkConnectCompletion::None);
        };
        let Some(PendingNetworkConnect::Dns(request)) =
            proc.live_io.pending_network_connect.as_ref()
        else {
            return Ok(PendingNetworkConnectCompletion::None);
        };
        if !request.is_ready() {
            return Ok(PendingNetworkConnectCompletion::None);
        }

        let pending = self
            .processes
            .get_mut(&id)
            .and_then(|proc| proc.live_io.pending_network_connect.take());
        let Some(PendingNetworkConnect::Dns(request)) = pending else {
            return Ok(PendingNetworkConnectCompletion::None);
        };

        let resolved = match request.receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => {
                if let Some(proc) = self.processes.get_mut(&id) {
                    proc.live_io.pending_network_connect =
                        Some(PendingNetworkConnect::Dns(request));
                }
                return Ok(PendingNetworkConnectCompletion::None);
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                Err("resolver thread disconnected".to_string())
            }
        };

        match resolved {
            Ok(addrs) if !addrs.is_empty() => {
                match self.start_next_pending_network_connect(id, addrs, &request.socket_options)? {
                    None => Ok(PendingNetworkConnectCompletion::Retrying),
                    Some(code) => {
                        let sentinel = self
                            .processes
                            .get(&id)
                            .map(|proc| proc.sentinel)
                            .unwrap_or(Value::NIL);
                        if let Some(proc) = self.processes.get_mut(&id) {
                            proc.status = process_status_failed_value(code);
                            proc.live_io.network_socket = None;
                            proc.live_io.pending_network_connect = None;
                        }
                        // GNU src/process.c:6141.
                        self.record_status_change(StatusChangeSite::NonBlockingConnectFailed, id);
                        Ok(PendingNetworkConnectCompletion::Failed { sentinel, code })
                    }
                }
            }
            Ok(_) | Err(_) => {
                let host = request.host.split('\0').next().unwrap_or(&request.host);
                if let Some(proc) = self.processes.get_mut(&id) {
                    proc.status = process_status_failed_message_value(format!(
                        "Name lookup of {host} failed"
                    ));
                    proc.live_io.network_socket = None;
                    proc.live_io.pending_network_connect = None;
                }
                Ok(PendingNetworkConnectCompletion::DnsFailed)
            }
        }
    }

    fn create_accepted_network_client(
        &mut self,
        buffers: &mut BufferManager,
        spec: AcceptedNetworkClientSpec,
    ) -> ProcessId {
        let AcceptedNetworkClientSpec {
            name,
            buffer,
            contact,
            socket,
            inheritance,
        } = spec;
        let buffer = buffer.materialize(buffers);
        let inherit_coding_system_flag = !buffer.is_nil() && inheritance.inherit_coding_system_flag;
        let client_id = self.create_process_with_kind_lisp(
            LispString::from_utf8(&name),
            buffer,
            LispString::from_utf8("network"),
            Vec::new(),
            ProcessKindWithoutDevice::Network,
            inheritance.coding,
        );
        if let Some(client) = self.get_mut(client_id) {
            client.live_io.network_socket = Some(socket);
            client.status = process_status_run_value();
            client.childp = contact;
            client.filter = inheritance.filter;
            client.sentinel = inheritance.sentinel;
            client.plist = inheritance.plist;
            client.inherit_coding_system_flag = inherit_coding_system_flag;
            client.thread = inheritance.thread;
            // `create_process_with_kind_lisp` supplies GNU `make_process`'s
            // `ExitQueryPolicy::Query`.  There is intentionally no policy in
            // `AcceptedNetworkClientInheritance`, so a `:noquery t` listener
            // cannot overwrite that accepted-client default.
            client.adaptive_read_buffering = 0;
            client.read_output_delay = Duration::ZERO;
            client.read_output_skip = false;
        }
        self.register_socket_fd(client_id).ok();
        client_id
    }

    fn accept_network_server_connections(
        &mut self,
        buffers: &mut BufferManager,
        id: ProcessId,
    ) -> Result<Vec<AcceptedNetworkConnection>, Flow> {
        // Accepted transports stay by value through this short-lived local
        // dispatch, avoiding an allocation for every accepted connection.
        #[allow(clippy::large_enum_variant)]
        enum AcceptedSocket {
            Tcp {
                stream: TcpStream,
                remote_addr: SocketAddr,
                local_addr: Option<SocketAddr>,
            },
            #[cfg(unix)]
            Seqpacket {
                socket: Socket,
                remote_addr: SockAddr,
                local_addr: Option<SockAddr>,
            },
            #[cfg(unix)]
            Unix {
                stream: UnixStream,
                remote_name: String,
                local_name: String,
            },
        }

        let mut accepted = Vec::new();

        loop {
            let accepted_socket = {
                let Some(server) = self.processes.get(&id) else {
                    return Ok(accepted);
                };
                match server.live_io.network_socket.as_ref() {
                    Some(NetworkSocket::TcpListener(listener)) => match listener.accept() {
                        Ok((stream, remote_addr)) => Ok(Some(AcceptedSocket::Tcp {
                            local_addr: stream.local_addr().ok(),
                            stream,
                            remote_addr,
                        })),
                        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
                        Err(_) => Ok(None),
                    },
                    #[cfg(unix)]
                    Some(NetworkSocket::SeqpacketListener(listener)) => match listener.accept() {
                        Ok((socket, remote_addr)) => Ok(Some(AcceptedSocket::Seqpacket {
                            local_addr: socket.local_addr().ok(),
                            socket,
                            remote_addr,
                        })),
                        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
                        Err(_) => Ok(None),
                    },
                    #[cfg(unix)]
                    Some(NetworkSocket::UnixListener(listener)) => match listener.accept() {
                        Ok((stream, _)) => Ok(Some(AcceptedSocket::Unix {
                            remote_name: unix_socket_addr_to_runtime_string(
                                stream.peer_addr().ok(),
                            ),
                            local_name: unix_socket_addr_to_runtime_string(
                                stream.local_addr().ok(),
                            ),
                            stream,
                        })),
                        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
                        Err(_) => Ok(None),
                    },
                    _ => Err(()),
                }
            };
            let accepted_socket = match accepted_socket {
                Ok(Some(socket)) => socket,
                Ok(None) => break,
                Err(()) => return Ok(accepted),
            };

            let (
                server_name,
                server_contact,
                server_buffer,
                server_filter,
                server_sentinel,
                server_log,
                server_plist,
                coding_decode,
                coding_encode,
                inherit_coding_system_flag,
                server_thread,
            ) = {
                let Some(server) = self.processes.get(&id) else {
                    return Ok(accepted);
                };
                (
                    process_name_runtime(server.name),
                    server.childp,
                    server.buffer,
                    server.filter,
                    server.sentinel,
                    server.log,
                    server.plist,
                    server.coding_decode,
                    server.coding_encode,
                    server.inherit_coding_system_flag,
                    server.thread,
                )
            };
            let inheritance = AcceptedNetworkClientInheritance {
                filter: server_filter,
                sentinel: server_sentinel,
                plist: copy_process_plist(server_plist)?,
                coding: ProcessCodingSystems::inherited_from_server(coding_decode, coding_encode),
                inherit_coding_system_flag,
                thread: server_thread,
            };

            let mut contact = super::builtins::builtin_copy_sequence(vec![server_contact])
                .unwrap_or(server_contact);
            contact =
                process_contact_plist_put(contact, ProcessKeyword::Server.value(), Value::NIL)
                    .unwrap_or(contact);

            let (client_name, socket, host_for_message) = match accepted_socket {
                AcceptedSocket::Tcp {
                    stream,
                    remote_addr,
                    local_addr,
                } => {
                    let _ = stream.set_nonblocking(true);
                    contact = process_contact_plist_put(
                        contact,
                        ProcessKeyword::Host.value(),
                        Value::string(remote_addr.ip().to_string()),
                    )
                    .unwrap_or(contact);
                    contact = process_contact_plist_put(
                        contact,
                        ProcessKeyword::Service.value(),
                        Value::fixnum(remote_addr.port() as i64),
                    )
                    .unwrap_or(contact);
                    contact = process_contact_plist_put(
                        contact,
                        ProcessKeyword::Remote.value(),
                        socket_addr_to_lisp_value(remote_addr),
                    )
                    .unwrap_or(contact);
                    if let Some(local_addr) = local_addr {
                        contact = process_contact_plist_put(
                            contact,
                            ProcessKeyword::Local.value(),
                            socket_addr_to_lisp_value(local_addr),
                        )
                        .unwrap_or(contact);
                    }
                    (
                        accepted_network_process_name(&server_name, remote_addr),
                        NetworkSocket::TcpStream(stream),
                        remote_addr.ip().to_string(),
                    )
                }
                #[cfg(unix)]
                AcceptedSocket::Seqpacket {
                    socket,
                    remote_addr,
                    local_addr,
                } => {
                    let _ = socket.set_nonblocking(true);
                    if let Some(remote_addr) = remote_addr.as_socket() {
                        contact = process_contact_plist_put(
                            contact,
                            ProcessKeyword::Host.value(),
                            Value::string(remote_addr.ip().to_string()),
                        )
                        .unwrap_or(contact);
                        contact = process_contact_plist_put(
                            contact,
                            ProcessKeyword::Service.value(),
                            Value::fixnum(remote_addr.port() as i64),
                        )
                        .unwrap_or(contact);
                        contact = process_contact_plist_put(
                            contact,
                            ProcessKeyword::Remote.value(),
                            socket_addr_to_lisp_value(remote_addr),
                        )
                        .unwrap_or(contact);
                        if let Some(local_addr) = local_addr.and_then(|addr| addr.as_socket()) {
                            contact = process_contact_plist_put(
                                contact,
                                ProcessKeyword::Local.value(),
                                socket_addr_to_lisp_value(local_addr),
                            )
                            .unwrap_or(contact);
                        }
                        (
                            accepted_network_process_name(&server_name, remote_addr),
                            NetworkSocket::SeqpacketStream(socket),
                            remote_addr.ip().to_string(),
                        )
                    } else {
                        let remote_name =
                            socket2_unix_sockaddr_to_runtime_string(Some(&remote_addr));
                        let local_name =
                            socket2_unix_sockaddr_to_runtime_string(local_addr.as_ref());
                        contact = process_contact_plist_put(
                            contact,
                            ProcessKeyword::Host.value(),
                            Value::NIL,
                        )
                        .unwrap_or(contact);
                        contact = process_contact_plist_put(
                            contact,
                            ProcessKeyword::Remote.value(),
                            Value::string(&remote_name),
                        )
                        .unwrap_or(contact);
                        contact = process_contact_plist_put(
                            contact,
                            ProcessKeyword::Local.value(),
                            Value::string(&local_name),
                        )
                        .unwrap_or(contact);

                        let prefix = format!("{} <", server_name);
                        let sequence = self
                            .processes
                            .values()
                            .filter(|process| {
                                process_name_runtime(process.name).starts_with(&prefix)
                            })
                            .count()
                            + 1;
                        let host_for_message = if remote_name.is_empty() {
                            "-".to_string()
                        } else {
                            remote_name
                        };
                        (
                            format!("{} <{}>", server_name, sequence),
                            NetworkSocket::SeqpacketStream(socket),
                            host_for_message,
                        )
                    }
                }
                #[cfg(unix)]
                AcceptedSocket::Unix {
                    stream,
                    remote_name,
                    local_name,
                } => {
                    let _ = stream.set_nonblocking(true);
                    contact = process_contact_plist_put(
                        contact,
                        ProcessKeyword::Host.value(),
                        Value::NIL,
                    )
                    .unwrap_or(contact);
                    contact = process_contact_plist_put(
                        contact,
                        ProcessKeyword::Remote.value(),
                        Value::string(&remote_name),
                    )
                    .unwrap_or(contact);
                    contact = process_contact_plist_put(
                        contact,
                        ProcessKeyword::Local.value(),
                        Value::string(&local_name),
                    )
                    .unwrap_or(contact);

                    let prefix = format!("{} <", server_name);
                    let sequence = self
                        .processes
                        .values()
                        .filter(|process| process_name_runtime(process.name).starts_with(&prefix))
                        .count()
                        + 1;
                    let host_for_message = if remote_name.is_empty() {
                        "-".to_string()
                    } else {
                        remote_name
                    };
                    (
                        format!("{} <{}>", server_name, sequence),
                        NetworkSocket::UnixStream(stream),
                        host_for_message,
                    )
                }
            };

            let buffer = AcceptedClientBuffer::from_server(
                buffers,
                server_buffer,
                server_filter,
                &server_name,
                &client_name,
            );
            let client_id = self.create_accepted_network_client(
                buffers,
                AcceptedNetworkClientSpec {
                    name: client_name,
                    buffer,
                    contact,
                    socket,
                    inheritance,
                },
            );

            accepted.push(AcceptedNetworkConnection {
                server_id: id,
                client_id,
                log: server_log,
                sentinel: server_sentinel,
                log_message: format!("accept from {}\n", host_for_message),
                sentinel_message: format!("open from {}\n", host_for_message),
            });
        }

        Ok(accepted)
    }

    /// Read available output from a process — child stdout or network socket.
    /// Returns `Some(data)` with available data (possibly empty on WouldBlock),
    /// or `None` on EOF / connection closed.
    fn read_process_output_result(
        &mut self,
        id: ProcessId,
        destination: ProcessOutputDestination,
        coding_systems: &crate::emacs_core::coding::CodingSystemManager,
    ) -> ProcessBytesRead {
        if self
            .processes
            .get(&id)
            .is_some_and(|process| !process_filter_accepts_output(process))
        {
            return ProcessBytesRead::WouldBlock;
        }
        let source = self.processes.get(&id).and_then(process_output_source);

        match source {
            Some(ProcessOutputSource::Pty) => {
                self.read_pty_output_result(id, destination, coding_systems)
            }
            Some(ProcessOutputSource::ChildStdout) => {
                self.read_child_stdout_result(id, destination, coding_systems)
            }
            Some(ProcessOutputSource::Network) => {
                self.read_network_output_result(id, destination, coding_systems)
            }
            Some(ProcessOutputSource::Serial) => {
                self.read_serial_output_result(id, destination, coding_systems)
            }
            None => ProcessBytesRead::NoSource,
        }
    }

    /// Read available output from a process WITHOUT decoding it.
    ///
    /// The name says what it skips, and the return type enforces it.  Decoding
    /// is `decode_coding_object`'s (src/coding.h:750-755), which evaluates the
    /// coding system's `:post-read-conversion` (src/coding.c:8180-8194) and
    /// therefore needs the `Context` that owns this `ProcessManager`; and GNU
    /// then runs `read_process_output_set_last_coding_system`
    /// (src/process.c:6417-6425) after every decoded run, which needs the same
    /// `Context` to write `last-coding-system-used` into.  This entry point has
    /// neither, so it hands back the undecoded [`PendingProcessRun`] and lets
    /// the caller say out loud that it is not decoding.  Every path that serves
    /// real Lisp goes through [`Context::read_process_output_recording_coding`]
    /// instead; this one exists for unit fixtures that drive a `ProcessManager`
    /// on its own.
    ///
    /// The `CodingSystemManager` is still a REQUIRED argument, because
    /// `detect_coding` reads it and a fixture that skipped it would resolve the
    /// coding system by a different rule than the editor does.  A fixture with
    /// no `Context` in reach can pass `&CodingSystemManager::new()`, which is
    /// the honest statement that no coding system is defined and therefore
    /// nothing detects -- not a silent inheritance of one.
    pub(crate) fn read_process_output_without_decoding(
        &mut self,
        id: ProcessId,
        destination: ProcessOutputDestination,
        coding_systems: &crate::emacs_core::coding::CodingSystemManager,
    ) -> Option<PendingProcessRun> {
        match self.read_process_output_result(id, destination, coding_systems) {
            ProcessBytesRead::Data { run, .. } | ProcessBytesRead::EofAfterLastBlock { run } => {
                Some(run)
            }
            // A discarded read has no run by construction, and this entry
            // point's only caller passes `to_filter()`, which never discards.
            ProcessBytesRead::Discarded { .. }
            | ProcessBytesRead::WouldBlock
            | ProcessBytesRead::Failed
            | ProcessBytesRead::Eof
            | ProcessBytesRead::NoSource => None,
        }
    }

    /// The half of GNU's `read_process_output_set_last_coding_system`
    /// (src/process.c:6417-6459) that belongs to the process record rather than
    /// to the evaluator: the carryover the decoder could not consume, and the
    /// decoder state it ended in.
    ///
    /// Both happen AFTER the decode, which is GNU's order and not an
    /// arrangement of convenience -- see [`PendingProcessRun::carryover`].
    fn finish_process_run(
        &mut self,
        id: ProcessId,
        carryover: Vec<u8>,
        decoder: crate::encoding::CodingDecoderState,
    ) {
        let Some(proc) = self.get_mut(id) else {
            return;
        };
        proc.coding_state.store_carryover(carryover);
        proc.coding_state.store_decoder(decoder);
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
}

const DEFAULT_PROCESS_SENTINEL_SYMBOL: &str = "internal-default-process-sentinel";

fn dedupe_process_ids(process_ids: impl IntoIterator<Item = ProcessId>) -> Vec<ProcessId> {
    let mut unique = Vec::new();
    for id in process_ids {
        if !unique.contains(&id) {
            unique.push(id);
        }
    }
    unique
}

/// Put the processes whose STATUS is about to be reported into GNU's
/// notification order, leaving every other position in the pass untouched.
///
/// GNU services one wake with two walks in two different orders, and this port
/// has a single loop for both:
///
/// * the output/filter walk in `wait_reading_process_output` visits ready file
///   descriptors, in whatever order the fd scan produced them;
/// * the status walk in `status_notify` visits `FOR_EACH_PROCESS`
///   (src/process.c:7885), which is `FOR_EACH_ALIST_VALUE (Vprocess_alist,
///   ...)` (:343) -- and `make_process` conses each new process onto the FRONT
///   of that alist (:953).  So a status pass is NEWEST-FIRST, and two
///   processes whose status changed together run their sentinels in reverse
///   creation order.
///
/// `ProcessId` is a monotonic counter, so descending id IS newest-first; that
/// is the same identity `list_processes` uses to reproduce `process-list`.
/// Only the notification-pending entries are permuted, and only among
/// themselves, so the read order of every other process in the pass is
/// exactly what the poller reported.
fn order_pending_status_notifications_newest_first(
    processes: &ProcessManager,
    proc_ids: &mut [ProcessId],
) {
    let pending = |id: &ProcessId| {
        processes
            .get(*id)
            .is_some_and(|process| process.status_notify_pending)
    };
    let mut newest_first: Vec<ProcessId> =
        proc_ids.iter().filter(|id| pending(id)).copied().collect();
    if newest_first.len() < 2 {
        return;
    }
    newest_first.sort_unstable_by(|a, b| b.cmp(a));
    let mut next = newest_first.into_iter();
    for slot in proc_ids.iter_mut() {
        if pending(slot) {
            // One pending id is produced per pending slot, so the iterator
            // cannot run dry.
            *slot = next.next().expect("one notification per pending slot");
        }
    }
}

/// Which asynchronous callback an escaped error came from.
///
/// GNU treats the classes DIFFERENTLY, so the kind decides the reporting, not
/// a log string: a filter or sentinel error goes through `cmd_error_internal`
/// (process.c:6208, :7791) and is therefore FATAL in batch, while a timer error
/// is caught by timer.el's own `condition-case-unless-debug` and merely
/// messaged (timer.el:332-338), so batch survives it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AsyncCallbackKind {
    /// GNU `read_process_output_error_handler` (process.c:6208).
    ProcessFilter,
    /// GNU `exec_sentinel_error_handler` (process.c:7791).
    ProcessSentinel,
    /// GNU runs these through timer.el `timer-event-handler`, never through
    /// the command-error reporter.
    Timer,
    /// A network server's log function. GNU installs NO handler around it
    /// (a bare `calln`, process.c:5176), so an error there propagates to the
    /// command loop instead of being reported here. Keeping the catch is a
    /// known, deliberate divergence rather than part of this fix.
    ServerLog,
}

impl AsyncCallbackKind {
    /// Diagnostic name for the trace log.
    pub(crate) fn label(self) -> &'static str {
        match self {
            AsyncCallbackKind::ProcessFilter => "process filter",
            AsyncCallbackKind::ProcessSentinel => "process sentinel",
            AsyncCallbackKind::Timer => "GNU Lisp timer",
            AsyncCallbackKind::ServerLog => "server log",
        }
    }

    /// GNU's `cmd_error_internal` context string for this class, or `None`
    /// when GNU does not route the class through the command-error reporter.
    pub(crate) fn command_error_context(self) -> Option<&'static str> {
        match self {
            AsyncCallbackKind::ProcessFilter => Some("error in process filter: "),
            AsyncCallbackKind::ProcessSentinel => Some("error in process sentinel: "),
            AsyncCallbackKind::Timer | AsyncCallbackKind::ServerLog => None,
        }
    }
}

impl super::eval::Context {
    fn visible_process_read_config(&self) -> ProcessReadConfig {
        let readmax = self
            .visible_variable_value_or_nil("read-process-output-max")
            .as_fixnum()
            .unwrap_or(DEFAULT_READ_PROCESS_OUTPUT_MAX as i64)
            .clamp(1, READ_PROCESS_OUTPUT_MAX_CEILING as i64) as usize;
        let adaptive_value = self.visible_variable_value_or_nil("process-adaptive-read-buffering");
        let adaptive_read_buffering = if adaptive_value.is_nil() {
            0
        } else if adaptive_value == Value::T {
            1
        } else {
            2
        };

        ProcessReadConfig {
            readmax,
            adaptive_read_buffering,
        }
    }

    fn sync_process_read_config_from_visible_variables(&mut self) {
        let config = self.visible_process_read_config();
        self.processes.set_default_read_config(config);
    }

    /// GNU `status_notify`'s branch on `delete_exited_processes`
    /// (src/process.c:7926-7929); the variable is `DEFVAR_BOOL`'d at :8916 with
    /// default 1.  Read once per notification, as GNU reads the C variable, so
    /// a sentinel that rebinds it cannot change the decision already taken for
    /// its own process.
    fn exited_process_disposition(&self) -> ExitedProcessDisposition {
        ExitedProcessDisposition::from_delete_exited_processes(
            self.visible_variable_value_or_nil("delete-exited-processes")
                .is_truthy(),
        )
    }

    pub(crate) fn kill_buffer_processes(&mut self, buffer_id: BufferId) -> Result<(), Flow> {
        for id in self.processes.process_ids_for_buffer(buffer_id) {
            let Some(kind) = self.processes.get(id).map(|proc| proc.kind) else {
                continue;
            };
            if kind == ProcessKind::Real {
                self.processes.hangup_real_process_for_buffer_kill(id);
                continue;
            }

            let was_terminal = self
                .processes
                .get(id)
                .is_some_and(|proc| process_status_is_terminal_for_notify(&proc.status));
            let was_pending_notification = self
                .processes
                .get(id)
                .is_some_and(|proc| proc.status_notify_pending);
            // GNU `kill_buffer_processes` calls `Fdelete_process` for a
            // network/serial/pipe process (src/process.c:8463-8464), so it
            // inherits the stamp/`status_notify`/remove ordering above.
            self.delete_process_running_its_sentinel(
                id,
                !was_terminal || was_pending_notification,
            )?;
        }
        Ok(())
    }

    fn run_async_process_callback_preserving_state(
        &mut self,
        callback: Value,
        args: Vec<Value>,
        kind: AsyncCallbackKind,
    ) -> Result<(), Flow> {
        let saved_match_data = self.match_data.clone();
        let saved_current_buffer = self.buffers.current_buffer_id();
        let saved_waiting_for_input = self.waiting_for_user_input();
        let saved_deactivate_mark = self.eval_symbol("deactivate-mark").unwrap_or(Value::NIL);
        let specpdl_count = self.specpdl.len();

        let gc_roots = self.save_specpdl_roots();
        self.push_specpdl_root(callback);
        for arg in &args {
            self.push_specpdl_root(*arg);
        }
        // The saved state below lives only in Rust locals across the
        // callback (arbitrary Lisp): the saved match-data's searched string
        // and the saved deactivate-mark value are heap objects whose prior
        // roots the callback can replace (a new string-match, a plain setq).
        // Root them for the callback span; unbind_to pops these with the
        // specbinds. GNU parks the same state on its specpdl
        // (record_unwind_protect restore_match_data, keyboard.c/process.c).
        if let Some(crate::emacs_core::regex::SearchedString::Heap(searched)) = saved_match_data
            .as_ref()
            .and_then(crate::emacs_core::regex::MatchData::searched_string)
        {
            self.push_specpdl_root(*searched);
        }
        self.push_specpdl_root(saved_deactivate_mark);

        let result = (|| {
            self.try_specbind_or_unwind_to(specpdl_count, intern("inhibit-quit"), Value::T)?;
            self.try_specbind_or_unwind_to(specpdl_count, intern("last-nonmenu-event"), Value::T)?;
            self.apply(callback, args)
        })();
        self.match_data = saved_match_data;
        if let Some(buffer_id) = saved_current_buffer {
            self.restore_current_buffer_if_live(buffer_id);
        }
        self.set_waiting_for_user_input(saved_waiting_for_input);
        // Restore deactivate-mark BEFORE unbinding: its saved value loses
        // its root when unbind_to pops the GcRoot above, and GNU's specpdl
        // ordering restores it under the still-bound inhibit-quit anyway.
        self.assign("deactivate-mark", saved_deactivate_mark);
        let result = self.unbind_to_with_result(specpdl_count, result);
        self.restore_specpdl_roots(gc_roots);

        self.finish_callback_flow(result, kind)
    }

    /// Resolve the control flow that escaped a timer/process callback after the
    /// callback's own state (buffer/deactivate-mark/specpdl/gc-roots) has been
    /// restored.
    ///
    /// GNU runs timer callbacks through `lisp/emacs-lisp/timer.el`
    /// `timer-event-handler`, which wraps the call in
    /// `condition-case-unless-debug err … (error …)`; process filters/sentinels
    /// in `src/process.c` (`read_process_output`/`exec_sentinel`) run with no
    /// surrounding handler at all.  In both cases an `error`-class *signal* is
    /// caught (and logged), but a non-local `throw` is NOT an error, so it
    /// propagates past the callback boundary to the matching outer `catch`.
    ///
    /// Mirroring that, a `Flow::Signal` is caught and logged here, while
    /// non-local control flow is propagated to the caller so it can reach the
    /// matching wait/catch boundary.  A throw to a tag with no live catch still
    /// becomes a `no-catch` error at the eval/thread boundary, as in GNU.
    ///
    /// `Flow::Shutdown` propagates for the same reason: GNU's `Fkill_emacs`
    /// never returns, so a callback that kills cannot be resumed and its exit
    /// code must not be swallowed here.
    pub(crate) fn finish_callback_flow(
        &mut self,
        result: EvalResult,
        kind: AsyncCallbackKind,
    ) -> Result<(), Flow> {
        match result {
            Ok(_) => Ok(()),
            Err(err @ (Flow::Throw(_) | Flow::ThreadBlocked(_) | Flow::Shutdown(_))) => Err(err),
            Err(err @ Flow::Signal(_)) => {
                let rendered = super::error::format_flow_with_eval(self, &err);
                tracing::warn!("{} callback error: {}", kind.label(), rendered);
                let Flow::Signal(sig) = &err else {
                    unreachable!("matched Flow::Signal above")
                };
                match kind.command_error_context() {
                    // GNU reports these through cmd_error_internal, whose
                    // default reporter writes to stderr and kills a batch
                    // session -- so the shutdown propagates and the work the
                    // error escaped from is not resumed.
                    Some(context) => {
                        let data = self.signal_error_data_value(sig);
                        self.report_command_error(data, context)
                    }
                    // Reported by the callback's own Lisp handler in GNU; the
                    // trace above is all this boundary owes.
                    None => Ok(()),
                }
            }
        }
    }

    fn run_process_filter_callback(
        &mut self,
        pid: ProcessId,
        filter: Value,
        data: &LispString,
    ) -> Result<(), Flow> {
        let proc_val = Value::make_process(pid);
        let output_val = Value::heap_string(data.clone());
        match ProcessFilterDispatch::from_lisp(filter) {
            ProcessFilterDispatch::Default => {
                let callback = Value::symbol(DEFAULT_PROCESS_FILTER_SYMBOL);
                self.run_async_process_callback_preserving_state(
                    callback,
                    vec![proc_val, output_val],
                    AsyncCallbackKind::ProcessFilter,
                )
            }
            ProcessFilterDispatch::Suspended => Ok(()),
            ProcessFilterDispatch::Callback(callback) => self
                .run_async_process_callback_preserving_state(
                    callback,
                    vec![proc_val, output_val],
                    AsyncCallbackKind::ProcessFilter,
                ),
        }
    }

    /// If `pid` is an implicit `:stderr` pipe whose owner also has a terminal
    /// status to publish, publish the OWNER's first.
    ///
    /// This is the ordering GNU gets for free from `status_notify`'s
    /// newest-first walk of the process alist (src/process.c:7886): the owner
    /// is created after its stderr pipe, so it is always the newer entry. Here
    /// the owner's exit may simply not have been polled yet when the pipe's EOF
    /// arrives, so the check has to be explicit -- including polling the
    /// owner's child status, which is what `status_notify` would already have
    /// seen via SIGCHLD by the time it runs.
    ///
    /// The return value says whether the pipe may notify in this pass.  A
    /// running owner with no pending status work does not block its pipe's
    /// sentinel.
    fn notify_stderr_pipe_owner_first(
        &mut self,
        pid: ProcessId,
        target_process: Option<ProcessId>,
        outcome: &mut ProcessOutputServiceOutcome,
    ) -> Result<bool, Flow> {
        let Some(owner_id) = self.processes.stderr_pipe_owner(pid) else {
            return Ok(true);
        };
        let owner_already_notified = self.processes.get(owner_id).is_some_and(|owner| {
            process_status_is_terminal_for_notify(&owner.status) && !owner.status_notify_pending
        });
        if owner_already_notified {
            return Ok(true);
        }
        let mut owner_pending = self
            .processes
            .get(owner_id)
            .is_some_and(|owner| owner.status_notify_pending);
        #[cfg(not(windows))]
        {
            owner_pending = owner_pending
                || (self.processes.check_child_status_change(owner_id)
                    && self
                        .processes
                        .get(owner_id)
                        .is_some_and(|owner| owner.status_notify_pending));
        }
        if owner_pending {
            outcome.absorb(self.run_process_status_notification(owner_id, target_process)?);
            return Ok(false);
        }
        #[cfg(windows)]
        {
            owner_pending = self.processes.check_child_status_change(owner_id)
                && self
                    .processes
                    .get(owner_id)
                    .is_some_and(|owner| owner.status_notify_pending);
            if owner_pending {
                outcome.absorb(self.run_process_status_notification(owner_id, target_process)?);
                return Ok(false);
            }

            let deferred_at = self
                .processes
                .get(pid)
                .and_then(|pipe| pipe.stderr_pipe_owner_status_deferred_at);
            if deferred_at.is_none_or(|at| at.elapsed() < Duration::from_millis(100)) {
                if let Some(pipe) = self.processes.get_mut(pid) {
                    pipe.stderr_pipe_owner_status_deferred_at
                        .get_or_insert_with(Instant::now);
                }
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Drain an implicit `:stderr` pipe's available bytes through its filter,
    /// and close it if the drain reaches EOF, WITHOUT running its sentinel.
    ///
    /// That is GNU's division of labour, not a compromise: the fd-scan loop
    /// gives a pipe connection its terminal status as soon as a read returns 0
    /// (src/process.c:6072-6080), and `status_notify` runs the sentinel later.
    /// Deferring the sentinel is what the split-stderr wait wants -- a targeted
    /// wait for the owner must not run the pipe's sentinel early -- but
    /// deferring the STATUS with it left the pipe reading as `open` to the
    /// owner's sentinel, which is ledger entry 54.
    /// GNU's `setup_process_coding_systems` (src/process.c:8380-8409) re-reads
    /// the process's buffer and filter every time either changes, so this is
    /// evaluated per read rather than cached on the process.
    fn process_output_sink(&self, id: ProcessId) -> ProcessOutputDestination {
        let fast_read = self.fast_read_process_output_enabled();
        let Some(proc) = self.processes.get(id) else {
            return ProcessOutputDestination::to_filter();
        };
        ProcessOutputDestination::of(proc, &self.buffers, fast_read)
    }

    /// GNU's `fast_read_process_output` (src/process.c:8980), the Lisp variable
    /// `fast-read-process-output'.
    fn fast_read_process_output_enabled(&self) -> bool {
        !self
            .visible_variable_value_or_nil("fast-read-process-output")
            .is_nil()
    }

    /// Read one run of a process's output and record the coding system it was
    /// decoded with -- GNU's `read_process_output_set_last_coding_system`
    /// (src/process.c:6417-6446), which runs after EVERY decoded run, buffer
    /// destination and Lisp filter alike (:6506 and :6565).
    ///
    /// It does three things with one answer, and all three are observable:
    ///
    /// * `Vlast_coding_system_used` becomes `CODING_ID_NAME (coding->id)` --
    ///   the id BOTH of GNU's rewrites have had their turn at, so it carries
    ///   the character code `detect_coding` chose (src/coding.c:6751) as well
    ///   as the end-of-line type `adjust_coding_eol_type` chose (:6805).
    /// * When that differs from the process's own decode coding system, the
    ///   process's slot is REPLACED by it (`pset_decode_coding_system`, :6425).
    ///   That is how both detections become sticky: GNU decodes a subprocess
    ///   through ONE `struct coding_system`, so the second chunk of output is
    ///   decoded by whatever the first chunk resolved to.  Measured under GNU
    ///   31.0.90, a child writing `a CRLF b CRLF` and then, after a pause,
    ///   `x CR y CR` reads as `(97 10 98 10 120 13 121 13)` -- the second
    ///   chunk's bare CRs survive because the process is dos by then, where
    ///   re-detecting per chunk would call them `mac` and eat them.  The two
    ///   axes go sticky INDEPENDENTLY, because `undecided-dos` is still type
    ///   `Qundecided`: a first chunk of `a CRLF b CRLF` under `undecided`
    ///   reports `undecided-dos`, and a later chunk of `caf <c3> <a9> CR LF`
    ///   moves it on to `utf-8-dos`.
    /// * When the process's ENCODE coding system is still nil, it is completed
    ///   from the decode answer with `coding_inherit_eol_type` (:6442-6444).
    ///
    /// This is the only way to turn a [`ProcessBytesRead`] into a
    /// [`ProcessOutputRead`], so no driver on this side can consume process
    /// output without the write-back having happened.
    fn read_process_output_recording_coding(
        &mut self,
        id: ProcessId,
        destination: ProcessOutputDestination,
    ) -> Result<ProcessOutputRead, Flow> {
        // Stage one: the read.  It borrows the `ProcessManager` mutably out of
        // `self` and gives it back with a [`PendingProcessRun`] -- bytes and a
        // settled coding system, no text.  It no longer needs
        // `inhibit-eol-conversion': the trailing-CR lookahead that made it
        // matter is `eol_dos`, and `eol_dos` is the DECODER's, as it is in GNU
        // (src/coding.c:1250-1251).
        let (run, bytes_read, last_block) =
            match self
                .processes
                .read_process_output_result(id, destination, &self.coding_systems)
            {
                ProcessBytesRead::Data { run, bytes_read } => (run, bytes_read, false),
                ProcessBytesRead::EofAfterLastBlock { run } => (run, 0, true),
                // `read_and_insert_process_output` returned at :6464 without
                // decoding, so stages two and three below have nothing to do:
                // no text for a filter, no `last-coding-system-used', no
                // sticky rewrite of the process's coding system.  The count
                // still travels, because GNU's caller counts the read as
                // activity whether or not anything was made of it.
                ProcessBytesRead::Discarded { bytes_read } => {
                    return Ok(ProcessOutputRead::Data {
                        data: LispString::from_utf8(""),
                        bytes_read,
                    });
                }
                ProcessBytesRead::WouldBlock => return Ok(ProcessOutputRead::WouldBlock),
                ProcessBytesRead::Failed => return Ok(ProcessOutputRead::Failed),
                ProcessBytesRead::Eof => return Ok(ProcessOutputRead::Eof),
                ProcessBytesRead::NoSource => return Ok(ProcessOutputRead::NoSource),
            };
        // Stage two: the decode, with the whole `Context` in hand because GNU's
        // decoder needs the whole editor -- ISO-2022's designations, CCL
        // programs and charset lists live in the evaluator, and
        // `:post-read-conversion` IS the evaluator.
        let decoded = self.decode_pending_process_run(run)?;
        // Stage three: GNU's `read_process_output_set_last_coding_system`
        // (src/process.c:6417-6459), which runs after EVERY decoded run.
        self.record_process_run_coding(id, decoded.run.coding);
        self.processes
            .finish_process_run(id, decoded.run.carryover, decoded.decoder);
        if last_block {
            // GNU's zero-byte last block is delivered from INSIDE the read --
            // `read_and_dispose_of_process_output` calls the filter itself
            // (src/process.c:6567-6572) -- and `read_process_output` then
            // returns 0, which is the end of file its caller acts on (:6345).
            // Doing both here is what keeps the two from being separable: the
            // only variant that can carry a last block is consumed here, so no
            // drain loop can see an EOF whose last block was never decoded.
            // The text is empty unless a `:post-read-conversion` inserted some
            // of its own, which GNU counts into `coding->produced_char`
            // (src/coding.c:8194) and hands to the filter like any other run.
            if !decoded.run.text.is_empty() {
                let filter = self
                    .processes
                    .get(id)
                    .map(|p| p.filter)
                    .unwrap_or(Value::NIL);
                self.run_process_filter_callback(id, filter, &decoded.run.text)?;
            }
            return Ok(ProcessOutputRead::Eof);
        }
        Ok(ProcessOutputRead::Data {
            data: decoded.run.text,
            bytes_read,
        })
    }

    /// Run one [`PendingProcessRun`] through the shared decoder.
    ///
    /// GNU protects the decode, not only the filter: `specbind (Qinhibit_quit,
    /// Qt)` and `specbind (Qlast_nonmenu_event, Qt)` are at
    /// src/process.c:6537-6538, BEFORE either branch's
    /// `decode_coding_c_string` (:6502 through `read_and_insert_process_output`,
    /// :6562 for the filter), and the nonrecursive match-data save is at
    /// :6541-6556.  Once the decode evaluates Lisp those bindings stop being a
    /// filter-only concern, so they are taken here rather than only in
    /// [`Self::run_process_filter_callback`].
    ///
    /// What is NOT taken here is `inhibit-modification-hooks`.  GNU binds it in
    /// `read_and_insert_process_output` alone (:6501), around a decode that
    /// writes straight into the process buffer, "because that might modify the
    /// buffer, while we rely on process_coding.produced".  This port's decode
    /// produces a string in a work buffer -- which is GNU's own shape for the
    /// filter branch, `dst_object` `Qt` (src/coding.c:8133-8137) -- and the
    /// process buffer is written afterwards by the insertion path, with the
    /// change hooks GNU's `signal_after_change` (:6510) runs.
    fn decode_pending_process_run(
        &mut self,
        run: PendingProcessRun,
    ) -> Result<DecodedPendingProcessRun, Flow> {
        let PendingProcessRun {
            coding,
            bytes,
            mut decoder,
            block,
        } = run;
        let saved_match_data = self.match_data.clone();
        let specpdl_count = self.specpdl.len();
        let decoded = (|| {
            self.try_specbind_or_unwind_to(specpdl_count, intern("inhibit-quit"), Value::T)?;
            self.try_specbind_or_unwind_to(specpdl_count, intern("last-nonmenu-event"), Value::T)?;
            coding.decode_in_context(self, &bytes, &mut decoder, block)
        })();
        // The `?` is deliberately AFTER the unwind, not on the call: a
        // `:post-read-conversion` that signals must still leave the bindings
        // popped and the match data restored, which is what GNU's specpdl does
        // for it.  The bytes of a run whose decode signalled are lost in both
        // editors -- GNU's error escapes `read_process_output` before
        // `read_process_output_set_last_coding_system` can store the carryover.
        let decoded = match decoded {
            Ok(decoded) => self
                .unbind_to_with_result(specpdl_count, Ok(Value::NIL))
                .map(|_| decoded),
            Err(flow) => match self.unbind_to_with_result(specpdl_count, Err(flow)) {
                Err(flow) => Err(flow),
                Ok(_) => unreachable!("unwinding an error cannot produce a value"),
            },
        };
        self.match_data = saved_match_data;
        Ok(DecodedPendingProcessRun {
            run: decoded?,
            decoder,
        })
    }

    /// The write-back itself.
    fn record_process_run_coding(&mut self, id: ProcessId, coding: ProcessRunCoding) {
        let used = coding.used;
        let used_symbol = Value::symbol(used);
        self.set_variable("last-coding-system-used", used_symbol);

        // `if (!EQ (p->decode_coding_system, Vlast_coding_system_used))`, :6423.
        let Some(proc) = self.processes.get_mut(id) else {
            return;
        };
        if crate::emacs_core::value::eq_value(&proc.coding_decode, &used_symbol) {
            return;
        }
        proc.coding_decode = used_symbol;
        if !proc.coding_encode.is_nil() {
            return;
        }
        // "If a coding system for encoding is not yet decided, we set it as the
        // same as coding-system for decoding" (:6431-6433).
        let inherited = crate::encoding::coding_inherit_unix_eol_type(&self.coding_systems, used);
        if let Some(proc) = self.processes.get_mut(id) {
            proc.coding_encode = Value::symbol(&inherited);
        }
    }

    fn drain_associated_stderr_output_without_notifying(
        &mut self,
        stderr_id: ProcessId,
        target_process: Option<ProcessId>,
    ) -> Result<ProcessOutputServiceOutcome, Flow> {
        let mut outcome = ProcessOutputServiceOutcome::default();
        let is_target = target_process.is_none_or(|target| target == stderr_id);

        loop {
            let sink = self.process_output_sink(stderr_id);
            match self.read_process_output_recording_coding(stderr_id, sink)? {
                ProcessOutputRead::Data { data, bytes_read } => {
                    if bytes_read > 0 {
                        outcome.record_activity(is_target);
                    }
                    if !data.is_empty() {
                        let filter = self
                            .processes
                            .get(stderr_id)
                            .map(|p| p.filter)
                            .unwrap_or(Value::NIL);
                        self.run_process_filter_callback(stderr_id, filter, &data)?;
                    }
                }
                ProcessOutputRead::Eof | ProcessOutputRead::NoSource => {
                    self.processes.retire_pipe_process_at_read_eof(stderr_id);
                    return Ok(outcome);
                }
                ProcessOutputRead::Failed => {
                    self.processes.retire_process_at_read_failure(stderr_id);
                    return Ok(outcome);
                }
                ProcessOutputRead::WouldBlock => return Ok(outcome),
            }
        }
    }

    fn poll_process_stdout_output_without_status_detailed(
        &mut self,
        pid: ProcessId,
        target_process: Option<ProcessId>,
    ) -> Result<(ProcessOutputServiceOutcome, ProcessOutputDrainDisposition), Flow> {
        let mut outcome = ProcessOutputServiceOutcome::default();
        let mut saw_output = false;
        let is_target = target_process == Some(pid);

        loop {
            let sink = self.process_output_sink(pid);
            match self.read_process_output_recording_coding(pid, sink)? {
                ProcessOutputRead::Data { data, bytes_read } => {
                    if bytes_read > 0 {
                        saw_output = true;
                        outcome.record_activity(is_target);
                    }
                    if !data.is_empty() {
                        let filter = self
                            .processes
                            .get(pid)
                            .map(|p| p.filter)
                            .unwrap_or(Value::NIL);
                        self.run_process_filter_callback(pid, filter, &data)?;
                    }
                }
                ProcessOutputRead::WouldBlock => {
                    let disposition = if saw_output {
                        ProcessOutputDrainDisposition::Output
                    } else {
                        ProcessOutputDrainDisposition::Blocked
                    };
                    return Ok((outcome, disposition));
                }
                ProcessOutputRead::Eof | ProcessOutputRead::NoSource => {
                    let disposition = if saw_output {
                        ProcessOutputDrainDisposition::Output
                    } else {
                        ProcessOutputDrainDisposition::Terminal
                    };
                    return Ok((outcome, disposition));
                }
                ProcessOutputRead::Failed => {
                    self.processes.retire_process_at_read_failure(pid);
                    return Ok((outcome, ProcessOutputDrainDisposition::Terminal));
                }
            }
        }
    }

    fn poll_process_stdout_output_without_status(
        &mut self,
        pid: ProcessId,
        target_process: Option<ProcessId>,
    ) -> Result<(ProcessOutputServiceOutcome, bool), Flow> {
        let (outcome, disposition) =
            self.poll_process_stdout_output_without_status_detailed(pid, target_process)?;
        Ok((
            outcome,
            disposition == ProcessOutputDrainDisposition::Output,
        ))
    }

    fn run_process_sentinel_callback(
        &mut self,
        pid: ProcessId,
        sentinel: Value,
        message: &str,
    ) -> Result<(), Flow> {
        if sentinel.is_nil() {
            return Ok(());
        }

        let callback = if sentinel.is_symbol_named(DEFAULT_PROCESS_SENTINEL_SYMBOL) {
            Value::symbol(DEFAULT_PROCESS_SENTINEL_SYMBOL)
        } else {
            sentinel
        };

        self.run_async_process_callback_preserving_state(
            callback,
            vec![Value::make_process(pid), Value::string(message)],
            AsyncCallbackKind::ProcessSentinel,
        )
    }

    /// GNU `exec_sentinel` (src/process.c:7800), driven from a
    /// [`ProcessStatusNotification`] -- i.e. from arguments captured before the
    /// process was retired, never from a lookup that the retirement has
    /// invalidated.
    fn run_status_notification_sentinel(
        &mut self,
        notification: ProcessStatusNotification,
    ) -> Result<(), Flow> {
        self.run_process_sentinel_callback(
            notification.id(),
            notification.sentinel(),
            notification.message(),
        )
    }

    /// The sentinel for a process this port has already taken out of the live
    /// table.  Only `continue-process` reaches it now, and its `"continued"`
    /// notification retires nothing.
    fn notify_process_status_sentinel(&mut self, pid: ProcessId) -> Result<(), Flow> {
        let Some(notification) =
            ProcessStatusNotification::for_retired_process(&self.processes, pid)
        else {
            return Ok(());
        };
        // GNU `status_notify`'s `p->update_tick = p->tick;` (:7894, :7935).
        // This is the notification `process_send_signal` runs at :7181 for the
        // SIGCONT tick it moved at :7178; without the mark, that tick would
        // still be standing and the wait's own walk would run the sentinel a
        // second time.
        self.processes.mark_status_change_notified(pid);
        self.run_status_notification_sentinel(notification)
    }

    /// GNU `Fdelete_process` (src/process.c:1083-1156), which is not "remove,
    /// then notify" but "stamp, `status_notify`, remove":
    ///
    /// * :1123-1148 stamps the terminal status and kills the child;
    /// * :1129 / :1149 call `status_notify`, which takes the
    ///   `delete-exited-processes` decision (:7926-7929) and only then runs the
    ///   sentinel (:7937) -- so with the flag nil the sentinel still sees its
    ///   own process in `process-list`;
    /// * :1153 removes it unconditionally, after the sentinel has returned.
    ///
    /// Measured, `emacs -Q --batch`, GNU Emacs 31.0.90, with
    /// `delete-exited-processes` nil:
    ///
    /// ```text
    /// PW169-DELETE-KEEP-SENTINEL: (:event "killed" :get-buffer-process t
    ///                              :get-process t :in-process-list t)
    /// PW169-DELETE-KEEP-AFTER:    (:get-process nil :in-process-list nil)
    /// ```
    ///
    /// This port used to remove unconditionally before the sentinel, which was
    /// right for the default flag and wrong for the other setting -- the same
    /// class as the exit-path inversion this entry fixes, in the opposite
    /// direction.
    fn delete_process_running_its_sentinel(
        &mut self,
        id: ProcessId,
        run_sentinel: bool,
    ) -> Result<(), Flow> {
        if !self.processes.stamp_process_for_delete(id) {
            // Already gone: `delete-process` is idempotent and runs no sentinel.
            self.processes.delete_process(id);
            return Ok(());
        }
        let result = if run_sentinel {
            let disposition = self.exited_process_disposition();
            match ProcessStatusNotification::settle_status_and_retire(
                &mut self.processes,
                id,
                ProcessIoTeardown::Terminal,
                disposition,
            ) {
                Some(notification) => self.run_status_notification_sentinel(notification),
                None => Ok(()),
            }
        } else {
            Ok(())
        };
        // GNU's trailing `remove_process` (:1153) is unconditional, and it runs
        // even when the sentinel signalled -- `exec_sentinel` swallows the
        // error in its own `internal_condition_case_1` (:7845-7848), so the
        // removal is never skipped.  Reap before propagating, for the same
        // reason.
        self.processes.reap_exited_process(id);
        result
    }

    fn run_process_log_callback(
        &mut self,
        log: Value,
        server_id: ProcessId,
        client_id: ProcessId,
        message: &str,
    ) -> Result<(), Flow> {
        if log.is_nil() {
            return Ok(());
        }

        self.run_async_process_callback_preserving_state(
            log,
            vec![
                Value::make_process(server_id),
                Value::make_process(client_id),
                Value::string(message),
            ],
            AsyncCallbackKind::ServerLog,
        )
    }

    pub(crate) fn poll_process_output_for_service_request(
        &mut self,
        request: &ProcessOutputServiceRequest,
    ) -> Result<ProcessOutputServiceOutcome, Flow> {
        let target_process = request.target_process();
        let proc_ids = request.live_processes(self.processes.live_process_ids());
        self.poll_process_output_for_ids(proc_ids, target_process, true)
    }

    /// GNU `status_notify (NULL, WAIT_PROC)` (src/process.c:5554, :5854), with
    /// GNU's own visit set.
    ///
    /// `status_notify` is a `FOR_EACH_PROCESS` over the whole alist whose body
    /// is guarded per process by `p->tick != p->update_tick` (:7892), so the
    /// set is *"every process whose status changed since it was last
    /// notified"* -- and NOT *"every process the child-status sweep just
    /// stamped"*, which is what this function used to take.  The difference is
    /// the seven of GNU's eight `p->tick = ++process_tick;` sites that are not
    /// `handle_child_signal`'s (see [`StatusChangeSite`]): a status this port
    /// publishes from the pipe-EOF branch or a read failure left nothing
    /// behind that a later walk could pick up.  (A write `EPIPE` is NOT such
    /// a site: GNU's arm closes the write fd and touches no status, so the
    /// child's death reaches the walk as the SIGCHLD record.)
    ///
    /// `p->update_tick = p->tick` is set for the whole visit set FIRST, which
    /// is GNU's :7885-7894 and its stated reason -- *"Set this now, so that if
    /// new processes are created by sentinels that we run, we get called again
    /// to handle their status changes"*.  It is also what bounds the walk: a
    /// process this port declines to notify on this pass (a deferral GNU does
    /// not have) keeps its `status_notify_pending` and is serviced by the ready
    /// set, rather than being revisited by every later walk forever.
    pub(crate) fn notify_processes_with_unnotified_status_change(
        &mut self,
        target_process: Option<ProcessId>,
    ) -> Result<ProcessOutputServiceOutcome, Flow> {
        let visit = self.processes.processes_with_unnotified_status_change();
        if visit.is_empty() {
            return Ok(ProcessOutputServiceOutcome::default());
        }
        child_status::record_status_notify_visits(visit.len());
        // GNU src/process.c:7894, `p->update_tick = p->tick;`, inside the
        // membership test and before anything else the visit does.
        for id in &visit {
            self.processes.mark_status_change_notified(*id);
        }
        // `publish_status_before_readable_output` is GNU's order in
        // `status_notify` itself: it drains the process's remaining output
        // (:7896-7909) and then runs the sentinel, so the status is what the
        // visit is FOR.
        self.poll_process_output_for_ids(visit, target_process, true)
    }

    pub(crate) fn poll_ready_process_output_for_service_request(
        &mut self,
        events: ProcessWaitEvents,
        request: &ProcessOutputServiceRequest,
    ) -> Result<ProcessOutputServiceOutcome, Flow> {
        let target_process = request.target_process();
        let mut outcome = ProcessOutputServiceOutcome::default();

        let writable_processes = request.ready_processes(events.writable_processes_ref().to_vec());
        for pid in writable_processes {
            match self.processes.complete_pending_network_connect(pid)? {
                PendingNetworkConnectCompletion::None => {}
                PendingNetworkConnectCompletion::Retrying => {
                    outcome.record_serviced();
                }
                PendingNetworkConnectCompletion::Connected { sentinel } => {
                    // GNU services :nowait completion inside the wait and
                    // keeps waiting (only read bytes complete the wait).
                    outcome.record_serviced();
                    self.run_process_sentinel_callback(pid, sentinel, "open\n")?;
                }
                PendingNetworkConnectCompletion::Failed { sentinel, code } => {
                    outcome.record_serviced();
                    // GNU's :6141 bump is consumed by the `status_notify` of
                    // the wait it happened inside; this port runs the sentinel
                    // in the same pass, so the tick is spent here.
                    self.processes.mark_status_change_notified(pid);
                    self.run_process_sentinel_callback(
                        pid,
                        sentinel,
                        &format!("failed with code {code}\n"),
                    )?;
                }
                PendingNetworkConnectCompletion::DnsFailed => {
                    outcome.record_serviced();
                }
            }
            match self.processes.flush_process_write_queue(pid)? {
                ProcessWriteFlush::Drained | ProcessWriteFlush::Blocked => {
                    outcome.record_serviced();
                }
                ProcessWriteFlush::NoSource => {}
            }
        }

        let proc_ids = request.ready_processes(events.ready_processes_ref().to_vec());
        outcome.absorb(self.poll_process_output_for_ids(proc_ids, target_process, false)?);

        Ok(outcome)
    }

    fn poll_process_output_for_ids(
        &mut self,
        proc_ids: Vec<ProcessId>,
        target_process: Option<ProcessId>,
        publish_status_before_readable_output: bool,
    ) -> Result<ProcessOutputServiceOutcome, Flow> {
        let mut proc_ids = dedupe_process_ids(proc_ids);

        if proc_ids.is_empty() {
            return Ok(ProcessOutputServiceOutcome::default());
        }
        order_pending_status_notifications_newest_first(&self.processes, &mut proc_ids);
        if let Some(target) = target_process
            && let Some(index) = proc_ids.iter().position(|pid| *pid == target)
        {
            proc_ids.remove(index);
            proc_ids.insert(0, target);
        }

        let mut outcome = ProcessOutputServiceOutcome::default();
        let mut split_stderr_owners_with_output = Vec::new();

        for pid in proc_ids {
            let is_target = target_process.is_none_or(|target| target == pid);
            if self
                .processes
                .get(pid)
                .is_some_and(process_has_ready_async_dns)
            {
                match self.processes.complete_pending_network_connect(pid)? {
                    PendingNetworkConnectCompletion::None => {}
                    PendingNetworkConnectCompletion::Retrying => {
                        outcome.record_serviced();
                    }
                    PendingNetworkConnectCompletion::Connected { sentinel } => {
                        outcome.record_serviced();
                        self.run_process_sentinel_callback(pid, sentinel, "open\n")?;
                    }
                    PendingNetworkConnectCompletion::Failed { sentinel, code } => {
                        outcome.record_serviced();
                        // Same as above: the sentinel runs in this pass, so
                        // GNU's :6141 tick is consumed here.
                        self.processes.mark_status_change_notified(pid);
                        self.run_process_sentinel_callback(
                            pid,
                            sentinel,
                            &format!("failed with code {code}\n"),
                        )?;
                    }
                    PendingNetworkConnectCompletion::DnsFailed => {
                        outcome.record_serviced();
                    }
                }
                continue;
            }
            if self
                .processes
                .get(pid)
                .is_some_and(|process| process.status_notify_pending)
            {
                // GNU's `status_notify` walks the process alist newest-first,
                // and an implicit `:stderr` pipe is created BEFORE the process
                // that owns it, so within one notification pass the owner's
                // sentinel always runs before the pipe's. This loop instead
                // services whichever process is ready, which flipped that order
                // whenever the pipe's EOF was observed before the child's exit
                // was polled -- and the flip is Lisp-visible, because the pipe
                // is removed from the alist by its own notification, so the
                // owner's sentinel then found `get-buffer-process' nil where
                // GNU still has the pipe attached and `closed` (ledger 54).
                let is_implicit_stderr = self.processes.get(pid).is_some_and(|process| {
                    process.kind == ProcessKind::Pipe
                        && !process.live_io.child.has_child()
                        && self.processes.stderr_pipe_owner(pid).is_some()
                });
                let allow_pipe_notification =
                    self.notify_stderr_pipe_owner_first(pid, target_process, &mut outcome)?;
                if is_implicit_stderr && !allow_pipe_notification {
                    continue;
                }

                if self
                    .processes
                    .get(pid)
                    .is_some_and(process_defers_pty_status_after_explicit_coding)
                {
                    let (pending_outcome, disposition) = self
                        .poll_process_stdout_output_without_status_detailed(pid, target_process)?;
                    match disposition {
                        ProcessOutputDrainDisposition::Output => {
                            outcome.absorb(pending_outcome);
                            continue;
                        }
                        ProcessOutputDrainDisposition::Blocked => continue,
                        ProcessOutputDrainDisposition::Terminal => {}
                    }
                }
                {
                    outcome.absorb(self.run_process_status_notification(pid, target_process)?);
                }
                continue;
            }
            // A child status transition (exit, signal, stop, continued) is
            // independent of pipe readability.  Initial poll passes check it
            // before reading output, matching GNU's already-delivered
            // `raw_status_new`; readiness-wake passes let output win first.
            let has_readable_process_io = self
                .processes
                .get(pid)
                .is_some_and(process_has_readable_process_io);
            let defer_status_poll_for_readable_pty = self
                .processes
                .get(pid)
                .is_some_and(process_defers_status_poll_while_readable_pty);
            if !defer_status_poll_for_readable_pty
                && (publish_status_before_readable_output || !has_readable_process_io)
                && self.processes.check_child_status_change(pid)
            {
                if self
                    .processes
                    .get(pid)
                    .is_some_and(process_defers_pty_status_after_explicit_coding)
                {
                    let (pending_outcome, disposition) = self
                        .poll_process_stdout_output_without_status_detailed(pid, target_process)?;
                    match disposition {
                        ProcessOutputDrainDisposition::Output => {
                            outcome.absorb(pending_outcome);
                            continue;
                        }
                        ProcessOutputDrainDisposition::Blocked => continue,
                        ProcessOutputDrainDisposition::Terminal => {}
                    }
                }
                {
                    outcome.absorb(self.run_process_status_notification(pid, target_process)?);
                }
                continue;
            }
            if self
                .processes
                .get(pid)
                .is_some_and(|process| !process_has_readable_process_io(process))
            {
                continue;
            }
            if self.processes.get(pid).is_some_and(|process| {
                ProcessStatusSymbol::from_status_value(process.status)
                    == Some(ProcessStatusSymbol::Connect)
            }) {
                continue;
            }
            if target_process.is_none() && self.processes.clear_adaptive_read_skip_if_needed(pid) {
                continue;
            }

            self.sync_process_read_config_from_visible_variables();
            let accepted = self
                .processes
                .accept_network_server_connections(&mut self.buffers, pid)?;
            // The events' log/sentinel closures live only in this Rust Vec
            // while earlier callbacks run arbitrary Lisp; a log function
            // that set-process-sentinel's or delete-process's a connection
            // unlinks a later event's closure from the process table (its
            // only root), and a GC frees it before its dispatch. Thread the
            // Values onto one rooted heap list for the loop's span.
            let mut accepted_holder = Value::NIL;
            for event in accepted.iter().rev() {
                accepted_holder =
                    Value::cons(event.log, Value::cons(event.sentinel, accepted_holder));
            }
            let accepted_root_scope = self.save_specpdl_roots();
            self.push_specpdl_root(accepted_holder);
            let accepted_result = (|| -> Result<(), Flow> {
                for event in accepted {
                    // GNU `server_accept_connection` runs inside the wait; the
                    // accept (and its "open from" sentinel) never terminates it.
                    outcome.record_serviced();
                    self.run_process_log_callback(
                        event.log,
                        event.server_id,
                        event.client_id,
                        &event.log_message,
                    )?;
                    let sentinel = self
                        .processes
                        .get(event.client_id)
                        .map(|process| process.sentinel)
                        .unwrap_or(event.sentinel);
                    self.run_process_sentinel_callback(
                        event.client_id,
                        sentinel,
                        &event.sentinel_message,
                    )?;
                }
                Ok(())
            })();
            self.restore_specpdl_roots(accepted_root_scope);
            accepted_result?;

            let is_network = self
                .processes
                .get(pid)
                .map(|p| p.kind == ProcessKind::Network)
                .unwrap_or(false);
            // A standalone pipe process (including a `:stderr` pipe) has no
            // child of its own and reads through `child_stdout`.  On EOF it
            // must reach a terminal state and run its sentinel, or
            // `accept-process-output` would block forever waiting on it.  This
            // must NOT match the main (Real) process, which owns the child.
            let is_stderr_pipe = self
                .processes
                .get(pid)
                .is_some_and(is_standalone_pipe_process);

            let mut read_result = {
                let sink = self.process_output_sink(pid);
                self.read_process_output_recording_coding(pid, sink)?
            };
            let mut saw_output = false;
            let mut saw_eof_after_output = false;
            let mut handled_terminal_eof = false;
            loop {
                match read_result {
                    ProcessOutputRead::Data { data, bytes_read } => {
                        if bytes_read > 0 {
                            saw_output = true;
                            outcome.record_activity(is_target);
                        }
                        if !data.is_empty() {
                            let filter = self
                                .processes
                                .get(pid)
                                .map(|p| p.filter)
                                .unwrap_or(Value::NIL);
                            self.run_process_filter_callback(pid, filter, &data)?;
                        }
                        if bytes_read == 0 && data.is_empty() {
                            break;
                        }
                    }
                    ProcessOutputRead::Eof if is_stderr_pipe => {
                        if let Some(owner_id) = self.processes.stderr_pipe_owner(pid)
                            && target_process == Some(owner_id)
                        {
                            if !split_stderr_owners_with_output.contains(&owner_id) {
                                // The poller can report the split-stderr fd
                                // before the owner's stdout fd.  GNU's
                                // targeted wait still reports stdout/stderr
                                // bytes first and leaves the stderr EOF
                                // sentinel for a later notification pass.
                                let (owner_outcome, owner_saw_output) = self
                                    .poll_process_stdout_output_without_status(
                                        owner_id,
                                        target_process,
                                    )?;
                                outcome.absorb(owner_outcome);
                                if owner_saw_output {
                                    split_stderr_owners_with_output.push(owner_id);
                                }
                            }
                            if split_stderr_owners_with_output.contains(&owner_id) {
                                break;
                            }
                        }
                        outcome.record_serviced();

                        // The pipe finishes HERE (exit 0, so `process-status`
                        // reports `closed`) and is notified LATER, which is
                        // where GNU draws the line: the fd loop retires it
                        // (src/process.c:6072-6080) and `status_notify`
                        // (src/process.c:7873) runs the sentinel that inserts
                        // "Process NAME stderr finished" and removes it from
                        // the alist. Running both halves here published the
                        // pipe's death to Lisp -- sentinel first, then gone
                        // from `get-buffer-process' -- ahead of the OWNER's
                        // sentinel, where GNU's newest-first alist walk always
                        // puts the owner first (ledger entry 54).
                        self.processes.retire_pipe_process_at_read_eof(pid);
                        handled_terminal_eof = true;
                        break;
                    }
                    ProcessOutputRead::Eof | ProcessOutputRead::Failed if is_network => {
                        // GNU: EOF is not output; the wait continues (or the
                        // terminated-target break ends it at the loop top).
                        outcome.record_serviced();

                        // GNU: EOF on a running network connection sets
                        // `(exit . 256)` (process.c:6090, "Preserve status of
                        // processes already terminated" branch) -- exit code 0
                        // means "deleted\n", non-zero "connection broken by
                        // remote peer\n" (`status_message`). Derive the
                        // sentinel text from the status so the two can never
                        // disagree.
                        if let Some(proc) = self.processes.get_mut(pid)
                            && (process_status_is_run(&proc.status)
                                || ProcessStatusSymbol::from_status_value(proc.status)
                                    == Some(ProcessStatusSymbol::Open))
                        {
                            proc.status = process_status_exit_value(256);
                        }
                        // Same `status_notify` ordering as the child exit path
                        // below: the removal decision (src/process.c:7926-7929)
                        // precedes `exec_sentinel' (:7937), and the type is what
                        // makes that the only expressible order.
                        let disposition = self.exited_process_disposition();
                        let notification = ProcessStatusNotification::settle_status_and_retire(
                            &mut self.processes,
                            pid,
                            ProcessIoTeardown::Network,
                            disposition,
                        );
                        if let Some(notification) = notification {
                            self.run_status_notification_sentinel(notification)?;
                        }
                        handled_terminal_eof = true;
                        break;
                    }
                    ProcessOutputRead::Failed => {
                        outcome.record_serviced();
                        self.processes.retire_process_at_read_failure(pid);
                        handled_terminal_eof = true;
                        break;
                    }
                    ProcessOutputRead::Eof => {
                        if saw_output {
                            saw_eof_after_output = true;
                        }
                        break;
                    }
                    ProcessOutputRead::WouldBlock | ProcessOutputRead::NoSource => break,
                }

                // GNU's wait loop does a no-wait follow-up pass after reading
                // target output (`wait = MINIMUM`), which vacuums up immediately
                // available bytes and EOF/status transitions before returning.
                // Keep this non-blocking: stop as soon as the source would block.
                read_result = {
                    let sink = self.process_output_sink(pid);
                    self.read_process_output_recording_coding(pid, sink)?
                };
            }

            if handled_terminal_eof {
                continue;
            }

            if saw_eof_after_output
                && self.processes.get(pid).is_some_and(|process| {
                    process_output_source(process) == Some(ProcessOutputSource::Pty)
                })
            {
                self.processes.deactivate_pty_process_read_io(pid);
            }

            if saw_output {
                let associated_stderr_id = self
                    .processes
                    .get(pid)
                    .and_then(|proc| process_value_to_id(&proc.stderrproc));
                if let Some(stderr_id) =
                    associated_stderr_id.filter(|id| self.processes.get(*id).is_some())
                {
                    if !split_stderr_owners_with_output.contains(&pid) {
                        split_stderr_owners_with_output.push(pid);
                    }
                    // GNU's first targeted wait for a split-stderr subprocess
                    // can read both stdout and stderr bytes without also
                    // running the main or implicit stderr process sentinels.
                    // Drain currently available stderr bytes here; an EOF
                    // closes the pipe as GNU's fd loop does and leaves only the
                    // NOTIFICATION to a later status pass.
                    let stderr_outcome = self.drain_associated_stderr_output_without_notifying(
                        stderr_id,
                        target_process,
                    )?;
                    outcome.absorb(stderr_outcome);
                }

                // The pass got here because output was available; GNU then
                // does a no-wait follow-up (`wait = MINIMUM`) that can observe
                // and publish a just-exited child before
                // `accept-process-output` returns.  Model that as one
                // zero-duration backend wait and one nonblocking status poll;
                // this is a readiness drain, not a retry delay.
                //
                // This runs for an INITIAL-POLL pass as well, not only a
                // readiness wake, and ledger 180 is why.  The pre-read status
                // check at the top of the loop happens BEFORE the child has
                // written its output, so on an initial poll it always misses a
                // child that exits right after writing; without a follow-up the
                // pass returns having read the output and left the exit
                // unnoticed.  GNU cannot end up there: its record is made by
                // SIGCHLD at any instant and `status_notify` runs from the same
                // wait (src/process.c:5554), so `wait_reading_process_output`
                // does not return having seen a child's EOF and not its status.
                // Measured on the `(while (process-live-p p)
                // (accept-process-output p 1))` idiom, 60 runs: the sentinel is
                // lost 0/60 in GNU and was lost 4/60 here until this follow-up
                // covered the initial poll too.
                let publish_same_pass_after_output = self
                    .processes
                    .get(pid)
                    .is_some_and(process_publishes_status_after_ready_output);
                let mut status_changed =
                    publish_same_pass_after_output && self.processes.check_child_status_change(pid);
                if !status_changed && publish_same_pass_after_output {
                    let _ = self.processes.wait_for_backend_events(
                        Duration::ZERO,
                        ProcessWaitBackendInterest::ProcessesOnly,
                    );
                    status_changed = self.processes.check_child_status_change(pid);
                }
                if status_changed {
                    let defer_status_after_output = self
                        .processes
                        .get(pid)
                        .is_some_and(process_defers_pty_status_after_explicit_coding);
                    if defer_status_after_output {
                        continue;
                    }
                    outcome.absorb(self.run_process_status_notification(pid, target_process)?);
                }

                continue;
            }

            // No process bytes were read in this pass.  If the status was not
            // already published before a poll-phase read attempt, check it now.
            // GNU's SIGCHLD/status wake is independent of process output: the
            // shell-through-PTY deferral above preserves output-before-status
            // ordering only when bytes are actually pending, not for no-output
            // exits such as `sh -c "exit 7"`.
            if (!publish_status_before_readable_output || has_readable_process_io)
                && self.processes.check_child_status_change(pid)
            {
                if self
                    .processes
                    .get(pid)
                    .is_some_and(process_defers_pty_status_after_explicit_coding)
                {
                    let (pending_outcome, disposition) = self
                        .poll_process_stdout_output_without_status_detailed(pid, target_process)?;
                    match disposition {
                        ProcessOutputDrainDisposition::Output => {
                            outcome.absorb(pending_outcome);
                            continue;
                        }
                        ProcessOutputDrainDisposition::Blocked => continue,
                        ProcessOutputDrainDisposition::Terminal => {}
                    }
                }
                outcome.absorb(self.run_process_status_notification(pid, target_process)?);
            }
        }

        Ok(outcome)
    }

    /// GNU `status_notify` drains the terminated process's complete output
    /// topology before publishing its status and running its sentinel.  Return
    /// the resulting wait activity so output completes only a wait whose
    /// target admits that stream; running the sentinel merely services it.
    fn run_process_status_notification(
        &mut self,
        pid: ProcessId,
        target_process: Option<ProcessId>,
    ) -> Result<ProcessOutputServiceOutcome, Flow> {
        let mut outcome = ProcessOutputServiceOutcome::default();
        let owner_is_target = target_process.is_none_or(|target| target == pid);

        // GNU's `status_notify` calls `bset_update_mode_line` on the process's
        // buffer when its status changed (process.c:7940), because
        // `mode-line-process` renders that status. This is the one trigger
        // whose staleness is invisible to the editing user until the process
        // exits, so it must not depend on some later edit to repaint.
        self.mark_chrome_dirty_all();

        // Drain the owner's primary stream before exposing its terminal
        // status.  In GNU this happens while `status_notify` walks every
        // process whose tick changed.
        let mut saw_owner_output = false;
        while let ProcessOutputRead::Data { data, bytes_read } = {
            let sink = self.process_output_sink(pid);
            self.read_process_output_recording_coding(pid, sink)?
        } {
            if bytes_read > 0 {
                saw_owner_output = true;
                outcome.record_activity(owner_is_target);
            }
            if !data.is_empty() {
                let filter = self
                    .processes
                    .get(pid)
                    .map(|p| p.filter)
                    .unwrap_or(Value::NIL);
                self.run_process_filter_callback(pid, filter, &data)?;
            }
        }
        if let Some(proc) = self.processes.get_mut(pid)
            && process_should_defer_explicit_coding_status_after_output(proc, saw_owner_output)
        {
            proc.explicit_coding_status_deferred_once = true;
            return Ok(outcome);
        }

        // A `:stderr` destination is represented by an implicit pipe process,
        // but it is still part of this child's output topology.  The child has
        // exited, so every byte it wrote is now readable before EOF.  Drain
        // those bytes through the stderr process's own filter before the main
        // sentinel runs; asynchronous clients commonly inspect that buffer in
        // the sentinel.  Keep wait accounting attached to the stderr process,
        // as GNU does when WAIT_PROC names only the owner.
        let stderr_id = self
            .processes
            .get(pid)
            .and_then(|proc| process_value_to_id(&proc.stderrproc));
        if let Some(stderr_id) = stderr_id.filter(|id| self.processes.get(*id).is_some()) {
            outcome.absorb(
                self.drain_associated_stderr_output_without_notifying(stderr_id, target_process)?,
            );
        }

        // GNU `status_notify` settles the status, builds the message and takes
        // the `delete-exited-processes' removal decision (src/process.c:
        // 7914-7929) BEFORE `exec_sentinel' (:7937), so an exit sentinel sees
        // its own process already gone from `process-list'/`get-process'/
        // `get-buffer-process'.  This port had the two halves the other way
        // round (ledger 165, found and not fixed; ledger 169, fixed).  The
        // ordering is now the type's, not this function's: the notification
        // cannot be built without the retirement having happened, and it
        // carries the sentinel arguments that a post-retirement `get` could no
        // longer supply.
        let disposition = self.exited_process_disposition();
        let notification = ProcessStatusNotification::settle_status_and_retire(
            &mut self.processes,
            pid,
            ProcessIoTeardown::Terminal,
            disposition,
        );
        if let Some(notification) = notification {
            self.run_status_notification_sentinel(notification)?;
        }
        outcome.record_serviced();
        Ok(outcome)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn check_keyword_arg_pairs(args: &[Value]) -> Result<(), Flow> {
    if args.len().is_multiple_of(2) {
        Ok(())
    } else {
        Err(signal(LispCondition::MalformedKeywordArgList, vec![]))
    }
}

fn process_owned_runtime_string(value: Value) -> String {
    value
        .as_lisp_string()
        .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
        .expect("ValueKind::String must carry LispString payload")
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn expect_list(value: &Value) -> Result<(), Flow> {
    if value.is_list() {
        Ok(())
    } else {
        Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), *value],
        ))
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn signal_wrong_type_sequence(value: Value) -> Flow {
    signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol("sequencep"), value],
    )
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn signal_wrong_type_character(value: Value) -> Flow {
    signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol("characterp"), value],
    )
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn char_code_from_value(value: &Value) -> Result<u32, Flow> {
    match value.kind() {
        ValueKind::Fixnum(_) => Ok(super::builtins::expect_character_code(value)? as u32),
        _ => Err(signal_wrong_type_character(*value)),
    }
}

/// Append the Emacs-internal byte encoding of a single character code.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn push_char_code_bytes(code: u32, bytes: &mut Vec<u8>) {
    let mut buf = [0u8; crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH];
    let len = crate::emacs_core::emacs_char::char_string(code, &mut buf);
    bytes.extend_from_slice(&buf[..len]);
}

/// Convert a string / character-code vector / character-code list into a
/// faithful multibyte `LispString`, encoding each character code directly to
/// Emacs bytes via `char_string`.
///
/// Issue #131: this replaces a storage-String round-trip that corrupted real
/// character codes in the PUA sentinel ranges — e.g. the nerd-font glyph
/// U+E0B0 was rewritten to the eight-bit code 0x3FFFB0. Building the bytes
/// directly keeps every code intact.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn char_sequence_to_lisp_string(value: &Value) -> Result<LispString, Flow> {
    if let Some(string) = value.as_lisp_string() {
        return Ok(string.clone());
    }
    let mut bytes = Vec::new();
    match value.kind() {
        ValueKind::Veclike(VecLikeType::Vector) => {
            let vec = value.as_vector_data().unwrap().clone();
            for elt in vec.iter() {
                push_char_code_bytes(char_code_from_value(elt)?, &mut bytes);
            }
        }
        ValueKind::Cons | ValueKind::Nil => {
            let mut cursor = *value;
            loop {
                match cursor.kind() {
                    ValueKind::Nil => break,
                    ValueKind::Cons => {
                        let car = cursor.cons_car();
                        let cdr = cursor.cons_cdr();
                        push_char_code_bytes(char_code_from_value(&car)?, &mut bytes);
                        cursor = cdr;
                    }
                    _ => {
                        return Err(signal(
                            LispCondition::WrongTypeArgument,
                            vec![Value::symbol("listp"), cursor],
                        ));
                    }
                }
            }
        }
        _ => return Err(signal_wrong_type_sequence(*value)),
    }
    Ok(crate::heap_types::LispString::from_emacs_bytes(bytes))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn expect_int_or_marker(value: &Value) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        ValueKind::Veclike(VecLikeType::Marker) => super::marker::marker_position_as_int(value),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integer-or-marker-p"), *value],
        )),
    }
}

pub(crate) fn checked_region_bytes(
    buf: &crate::buffer::Buffer,
    region: super::position::LispRegionArgs,
) -> Result<EmacsByteRange, Flow> {
    region.accessible_byte_range(buf)
}

fn file_error_symbol(kind: std::io::ErrorKind) -> &'static str {
    match kind {
        std::io::ErrorKind::NotFound => "file-missing",
        std::io::ErrorKind::AlreadyExists => "file-already-exists",
        std::io::ErrorKind::PermissionDenied => "permission-denied",
        _ => "file-error",
    }
}

pub(crate) fn signal_process_io(action: &str, target: Option<&str>, err: std::io::Error) -> Flow {
    let mut data = vec![Value::string(action), Value::string(err.to_string())];
    if let Some(target) = target {
        data.push(Value::string(target));
    }
    signal(file_error_symbol(err.kind()), data)
}

/// GNU `report_file_error (STRING, FILENAME)` (callproc.c/fileio.c) for a
/// subprocess file-open/IO failure: signal a file-error-family condition whose
/// DATA is `(STRING STRERROR FILENAME)`, deriving the error SYMBOL and the bare
/// `strerror` string (no Rust "(os error N)" suffix) from the underlying
/// `errno`.  Use this instead of `signal_process_io` whenever the failing
/// operation has a Lisp filename to report — GNU always includes it.
#[cfg(unix)]
pub(crate) fn signal_process_file_error(
    action: &str,
    filename: Value,
    err: std::io::Error,
) -> Flow {
    let errno = err.raw_os_error().unwrap_or(libc::EIO);
    signal_file_errno(action, filename, errno)
}

#[cfg(not(unix))]
pub(crate) fn signal_process_file_error(
    action: &str,
    filename: Value,
    err: std::io::Error,
) -> Flow {
    let mut data = vec![
        Value::string(action),
        Value::string(err.to_string()),
        filename,
    ];
    signal(file_error_symbol(err.kind()), data)
}

/// The bare strerror string for an errno, matching GNU's `emacs_strerror`
/// (e.g. ENOENT -> "No such file or directory").  Rust's
/// `io::Error::to_string()` appends "(os error N)", which GNU never emits, so
/// go through libc directly.
#[cfg(unix)]
fn errno_message(errno: libc::c_int) -> String {
    // SAFETY: strerror returns a pointer to a static (per-thread) C string.
    unsafe {
        let ptr = libc::strerror(errno);
        if ptr.is_null() {
            String::new()
        } else {
            std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned()
        }
    }
}

#[cfg(not(unix))]
fn errno_message(errno: libc::c_int) -> String {
    std::io::Error::from_raw_os_error(errno).to_string()
}

/// GNU `report_file_errno` (fileio.c): signal a file-error-family condition
/// whose DATA is `(STRING ERRNO-STRING . NAME-LIST)` and whose error SYMBOL is
/// derived from ERRNO (ENOENT -> `file-missing`, EEXIST -> `file-already-exists`,
/// EACCES -> `permission-denied`, else `file-error`).  NAME is wrapped in a
/// one-element list unless it is itself a list (or nil), exactly like
/// `get_file_errno_data`.
pub(crate) fn signal_file_errno(string: &str, name: Value, errno: libc::c_int) -> Flow {
    let symbol = match errno {
        libc::ENOENT => "file-missing",
        libc::EEXIST => "file-already-exists",
        libc::EACCES => "permission-denied",
        _ => "file-error",
    };
    let mut data = vec![Value::string(string), Value::string(errno_message(errno))];
    if name.is_cons() || name.is_nil() {
        if let Some(items) = super::value::list_to_vec(&name) {
            data.extend(items);
        }
    } else {
        data.push(name);
    }
    signal(symbol, data)
}

fn signal_wrong_type_string(value: Value) -> Flow {
    signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol("stringp"), value],
    )
}

pub(crate) fn expect_string_strict(value: &Value) -> Result<String, Flow> {
    match value.kind() {
        ValueKind::String => Ok(process_owned_runtime_string(*value)),
        _ => Err(signal_wrong_type_string(*value)),
    }
}

fn expect_network_lookup_hostname(value: &Value) -> Result<String, Flow> {
    let string = match value.kind() {
        ValueKind::String => value
            .as_lisp_string()
            .expect("ValueKind::String must carry LispString payload"),
        _ => return Err(signal_wrong_type_string(*value)),
    };

    if string.is_multibyte() && string.sbytes() != string.schars() {
        let hostname = crate::emacs_core::emacs_char::to_utf8_lossy(string.as_bytes());
        return Err(signal(
            "error",
            vec![Value::string(format!(
                "Non-ASCII hostname {hostname} detected, please use \u{2018}puny-encode-domain\u{2019}"
            ))],
        ));
    }

    Ok(crate::emacs_core::emacs_char::to_utf8_lossy(
        string.as_bytes(),
    ))
}

fn expect_process_name_lisp_string(value: &Value) -> Result<LispString, Flow> {
    match value.kind() {
        ValueKind::String => Ok(value
            .as_lisp_string()
            .expect("ValueKind::String must carry LispString payload")
            .clone()),
        _ => Err(signal(
            "error",
            vec![Value::string(":name value not a string")],
        )),
    }
}

fn keyword_name(value: &Value) -> Option<&str> {
    match value.kind() {
        ValueKind::Symbol(k) => Some(resolve_sym(k)),
        _ => None,
    }
}
pub(crate) fn parse_lisp_string_args_strict(args: &[Value]) -> Result<Vec<LispString>, Flow> {
    args.iter()
        .map(|arg| {
            super::builtins::expect_lisp_string(arg)
                .cloned()
                .map_err(|_| signal_wrong_type_string(*arg))
        })
        .collect()
}

fn signal_wrong_type_processp(value: Value) -> Flow {
    signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol("processp"), value],
    )
}

fn signal_process_does_not_exist(name: &str) -> Flow {
    signal(
        "error",
        vec![Value::string(format!("Process {name} does not exist"))],
    )
}

fn signal_buffer_has_no_process(buffers: &BufferManager, buffer_id: BufferId) -> Flow {
    signal(
        "error",
        vec![Value::string(format!(
            "Buffer {} has no process",
            buffers
                .get(buffer_id)
                .map(|buffer| buffer.name_runtime_string_owned())
                .unwrap_or_else(|| "<deleted buffer>".to_string())
        ))],
    )
}

fn signal_process_not_active_in_manager(processes: &ProcessManager, id: ProcessId) -> Flow {
    let name = processes
        .get_any(id)
        .map(|proc| process_name_runtime(proc.name))
        .unwrap_or_else(|| id.to_string());
    signal(
        "error",
        vec![Value::string(format!("Process {name} is not active"))],
    )
}

/// GNU `process_send_signal`'s first guard, `!EQ (p->type, Qreal)`
/// (src/process.c:7084-7086).
///
/// It is asked of the process OBJECT, which `get_process` (:7081) resolves
/// without consulting liveness, so it must be asked through `get_any` here.
/// Asking it through the live table instead let a retired network, serial or
/// pipe process fall past it and be treated as signalable -- invisible until
/// ledger 169 started retiring processes before their sentinels run.
fn check_process_is_real_subprocess(processes: &ProcessManager, id: ProcessId) -> Result<(), Flow> {
    match processes.get_any(id) {
        Some(proc) if proc.kind != ProcessKind::Real => Err(signal_process_not_subprocess(proc)),
        _ => Ok(()),
    }
}

fn signal_process_not_subprocess(proc: &Process) -> Flow {
    signal(
        "error",
        vec![Value::string(format!(
            "Process {} is not a subprocess",
            process_name_runtime(proc.name)
        ))],
    )
}

fn signal_cannot_signal_process(proc: &Process) -> Flow {
    signal(
        "error",
        vec![Value::string(format!(
            "Cannot signal process {}",
            process_name_runtime(proc.name)
        ))],
    )
}

fn process_not_running_reason(proc: &Process) -> String {
    if process_is_listening(proc) {
        "listen".to_string()
    } else {
        // GNU's `error ("Process %s not running: %s", ..., status_message (p))`
        // (:6728, :7455) runs one line after `update_status`, so the reason
        // names the status the gate just rejected.
        gnu_process_status_message_for_status(proc, process_effective_status(proc))
    }
}

fn signal_process_not_running_in_manager(processes: &ProcessManager, id: ProcessId) -> Flow {
    let (name, reason) = processes
        .get_any(id)
        .map(|proc| {
            (
                process_name_runtime(proc.name),
                process_not_running_reason(proc),
            )
        })
        .unwrap_or_else(|| (id.to_string(), "inactive".to_string()));
    signal(
        "error",
        vec![Value::string(format!(
            "Process {name} not running: {reason}"
        ))],
    )
}

/// Decode a process designator into a raw `ProcessId` candidate.
///
/// This is the single root that maps a Lisp value to a process key.  Like GNU's
/// `get_process` / `CHECK_PROCESS`, only a genuine process object designates a
/// process by identity — a bare integer is NOT a process (GNU signals
/// `wrong-type-argument processp`).  It does NOT validate that the id still
/// names a live/known process; callers layer their own `get`/`get_any` checks
/// on top.  Name-string and nil (current-buffer) designators are handled by the
/// individual resolvers since they need manager/buffer state.
pub(crate) fn process_value_to_id(value: &Value) -> Option<ProcessId> {
    value.as_process_id()
}

fn resolve_process_or_wrong_type_any_in_manager(
    processes: &ProcessManager,
    value: &Value,
) -> Result<ProcessId, Flow> {
    if let Some(id) = process_value_to_id(value) {
        return if processes.get_any(id).is_some() {
            Ok(id)
        } else {
            Err(signal_wrong_type_processp(*value))
        };
    }
    match value.kind() {
        ValueKind::String => {
            let name = process_owned_runtime_string(*value);
            processes
                .find_by_name(&name)
                .ok_or_else(|| signal_wrong_type_processp(*value))
        }
        _ => Err(signal_wrong_type_processp(*value)),
    }
}

fn resolve_process_object_or_wrong_type_any_in_manager(
    processes: &ProcessManager,
    value: &Value,
) -> Result<ProcessId, Flow> {
    process_value_to_id(value)
        .filter(|id| processes.get_any(*id).is_some())
        .ok_or_else(|| signal_wrong_type_processp(*value))
}

fn resolve_process_for_status_in_state(
    processes: &ProcessManager,
    buffers: &BufferManager,
    value: &Value,
) -> Result<Option<ProcessId>, Flow> {
    if let Some(id) = process_value_to_id(value) {
        return if processes.get_any(id).is_some() {
            Ok(Some(id))
        } else {
            Err(signal_wrong_type_processp(*value))
        };
    }
    match value.kind() {
        ValueKind::String => {
            let name = process_owned_runtime_string(*value);
            Ok(processes.find_by_name(&name))
        }
        ValueKind::Nil => {
            let current_buffer = buffers
                .current_buffer_id()
                .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
            processes
                .find_by_buffer_id(current_buffer)
                .map(Some)
                .ok_or_else(|| signal_buffer_has_no_process(buffers, current_buffer))
        }
        ValueKind::Veclike(VecLikeType::Buffer) => {
            let buffer_id = value.as_buffer_id().unwrap();
            if buffers.get(buffer_id).is_none() {
                return Err(signal(
                    "error",
                    vec![Value::string("Attempt to get process for a dead buffer")],
                ));
            }
            processes
                .find_by_buffer_id(buffer_id)
                .map(Some)
                .ok_or_else(|| signal_buffer_has_no_process(buffers, buffer_id))
        }
        _ => Err(signal_wrong_type_processp(*value)),
    }
}

fn resolve_get_process_designator_in_state(
    processes: &ProcessManager,
    buffers: &BufferManager,
    value: &Value,
) -> Result<ProcessId, Flow> {
    if let Some(id) = process_value_to_id(value) {
        return if processes.get_any(id).is_some() {
            Ok(id)
        } else {
            Err(signal_wrong_type_processp(*value))
        };
    }

    match value.kind() {
        ValueKind::String => {
            let name = process_owned_runtime_string(*value);
            if let Some(id) = processes.find_by_name(&name) {
                return Ok(id);
            }
            if let Some(buffer_id) = buffers.find_buffer_by_name(&name) {
                return processes
                    .find_by_buffer_id(buffer_id)
                    .ok_or_else(|| signal_buffer_has_no_process(buffers, buffer_id));
            }
            Err(signal_process_does_not_exist(&name))
        }
        ValueKind::Nil => {
            let current_buffer = buffers
                .current_buffer_id()
                .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
            processes
                .find_by_buffer_id(current_buffer)
                .ok_or_else(|| signal_buffer_has_no_process(buffers, current_buffer))
        }
        ValueKind::Veclike(VecLikeType::Buffer) => {
            let buffer_id = value.as_buffer_id().unwrap();
            if buffers.get(buffer_id).is_none() {
                return Err(signal(
                    "error",
                    vec![Value::string("Attempt to get process for a dead buffer")],
                ));
            }
            processes
                .find_by_buffer_id(buffer_id)
                .ok_or_else(|| signal_buffer_has_no_process(buffers, buffer_id))
        }
        _ => Err(signal_wrong_type_processp(*value)),
    }
}

/// GNU's `Fget_buffer` (src/buffer.c:479-491), which is how both of this
/// port's buffer-keyed process lookups name a buffer:
///
/// ```c
///   if (BUFFERP (buffer_or_name))
///     return buffer_or_name;
///   CHECK_STRING (buffer_or_name);
///   return Fcdr (assoc_ignore_text_properties (buffer_or_name, Vbuffer_alist));
/// ```
///
/// Three things it does NOT do, each of which this function used to.  A buffer
/// OBJECT comes back as given, dead or alive -- the docstring's "If
/// BUFFER-OR-NAME is a buffer, return it as given" (:483) -- so a process
/// whose buffer was killed is still findable by that buffer, which is the
/// state `Fget_buffer_process`'s own docstring describes ("Return nil if all
/// processes associated with BUFFER have been deleted or killed",
/// src/process.c:8414-8415: the BUFFER may outlive nothing at all).  A name is
/// matched against `Vbuffer_alist`, which holds only live buffers.  And `nil`
/// is not a designator for anything: `Fget_buffer_process` answers it with
/// `if (NILP (buffer)) return Qnil;` (:8421) rather than reaching for the
/// selected window, and `Fmake_network_process` reads `buffer_defaults` for it
/// (:4132-4135).
///
/// This is deliberately NOT `get_process`'s rule (src/process.c:1045-1048),
/// which errors with "Attempt to get process for a dead buffer": that is a
/// PROCESS designator and this is a buffer one.  The two neighbours above it
/// implement `get_process`, and they are right to differ.
fn resolve_buffer_for_process_lookup_in_state(
    buffers: &BufferManager,
    value: &Value,
) -> Result<Option<crate::buffer::BufferId>, Flow> {
    match value.kind() {
        ValueKind::Nil => Ok(None),
        ValueKind::String => {
            let name_str = process_owned_runtime_string(*value);
            Ok(buffers.find_buffer_by_name(&name_str))
        }
        ValueKind::Veclike(VecLikeType::Buffer) => Ok(value.as_buffer_id()),
        _ => Err(signal_wrong_type_string(*value)),
    }
}

fn resolve_live_process_designator_in_manager(
    processes: &ProcessManager,
    value: &Value,
) -> Option<ProcessId> {
    let id = process_value_to_id(value)?;
    processes.get(id).map(|_| id)
}

fn resolve_live_process_or_wrong_type_in_manager(
    processes: &ProcessManager,
    value: &Value,
) -> Result<ProcessId, Flow> {
    resolve_live_process_designator_in_manager(processes, value).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), *value],
        )
    })
}

fn current_thread_handle(threads: &ThreadManager) -> Value {
    threads
        .thread_handle(threads.current_thread_id())
        .unwrap_or(Value::NIL)
}

fn is_stale_process_id_designator_in_manager(processes: &ProcessManager, value: &Value) -> bool {
    match process_value_to_id(value) {
        Some(id) if id > 0 => {
            processes.get(id).is_none()
                && (processes.get_any(id).is_some() || processes.was_issued_id(id))
        }
        _ => false,
    }
}

/// The same staleness, restricted to the kind GNU's TYPE check lets through.
///
/// GNU's "Process NAME is not active" is `p->infd < 0`, and in every subr that
/// raises it the type check comes FIRST: `process_send_signal` tests
/// `!EQ (p->type, Qreal)` at src/process.c:7084-7086 before `p->infd < 0` at
/// :7087-7089, and `Fprocess_running_child_p` does the same at :7042-7047.  A
/// network, serial or pipe process is never `Qreal`, so for those the type
/// check always wins -- "is not a subprocess", never "is not active" -- and
/// `stop-process`/`continue-process` do not reach either test, because they
/// handle those three kinds first and return the process (:7267-7278,
/// :7294-7315).
///
/// This port's analogue of `p->infd < 0` is "no longer in the live table", and
/// ledger 169 made that true at the retirement, which is where GNU puts it.
/// Answering it ahead of the type check made a retired `:stderr` pipe report
/// "is not active" inside its own sentinel where GNU reports "is not a
/// subprocess" -- six rows of the neighbour audit, measured.  So the guard is
/// asked only about the kind GNU would have let past.
///
/// An id in neither table cannot be asked its kind; it keeps the old answer.
fn is_stale_real_process_designator_in_manager(processes: &ProcessManager, value: &Value) -> bool {
    is_stale_process_id_designator_in_manager(processes, value)
        && process_value_to_id(value)
            .and_then(|id| processes.get_any(id))
            .is_none_or(|proc| proc.kind == ProcessKind::Real)
}

fn resolve_optional_process_or_current_buffer_in_state(
    processes: &ProcessManager,
    buffers: &BufferManager,
    value: Option<&Value>,
) -> Result<ProcessId, Flow> {
    if let Some(v) = value
        && !v.is_nil()
    {
        return resolve_get_process_designator_in_state(processes, buffers, v);
    }

    let current_buffer = buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    processes
        .find_by_buffer_id(current_buffer)
        .ok_or_else(|| signal_buffer_has_no_process(buffers, current_buffer))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn process_live_status_value(process: &Process) -> Value {
    if process_stopped_for_io(process) {
        return Value::list(vec![Value::symbol("stop")]);
    }
    // GNU decodes a pending child status at observation (`update_status`), so
    // a process whose exit has been reaped-but-not-yet-notified is already
    // dead to `process-live-p`.
    let status = process_effective_status(process);
    let kind = process.kind;
    match ProcessStatusSymbol::from_status_value(status) {
        Some(ProcessStatusSymbol::Run) => process_live_running_status_value(kind),
        Some(ProcessStatusSymbol::Stop) => Value::list(vec![Value::symbol("stop")]),
        Some(ProcessStatusSymbol::Open) => Value::list(vec![
            Value::symbol("open"),
            Value::symbol("listen"),
            Value::symbol("connect"),
            Value::symbol("stop"),
        ]),
        Some(ProcessStatusSymbol::Listen) => Value::list(vec![
            Value::symbol("listen"),
            Value::symbol("connect"),
            Value::symbol("stop"),
        ]),
        Some(ProcessStatusSymbol::Connect) => Value::list(vec![Value::symbol("connect")]),
        _ => Value::NIL,
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn process_live_running_status_value(kind: ProcessKind) -> Value {
    match kind {
        ProcessKind::Network => Value::list(vec![
            Value::symbol("listen"),
            Value::symbol("connect"),
            Value::symbol("stop"),
        ]),
        ProcessKind::Pipe => Value::list(vec![
            Value::symbol("open"),
            Value::symbol("listen"),
            Value::symbol("connect"),
            Value::symbol("stop"),
        ]),
        _ => Value::list(vec![
            Value::symbol("run"),
            Value::symbol("open"),
            Value::symbol("listen"),
            Value::symbol("connect"),
            Value::symbol("stop"),
        ]),
    }
}

/// GNU `update_status` view of a process: a pending-but-unnotified child
/// status (`raw_status_new` in GNU, `status_notify_pending` +
/// `pending_status` here) is DECODED at observation points --
/// `Fprocess_status` and `Fprocess_exit_status` both run `update_status`
/// before reading -- while the sentinel notification stays pending for the
/// wait loop's `status_notify` pass.
fn process_effective_status(process: &Process) -> Value {
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
fn process_public_status_symbol(process: &Process) -> Value {
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
            _ => Value::symbol("closed"),
        },
        Some(ProcessStatusSymbol::Open) => ProcessStatusSymbol::Open.value(),
        Some(ProcessStatusSymbol::Listen) => ProcessStatusSymbol::Listen.value(),
        Some(ProcessStatusSymbol::Closed) => ProcessStatusSymbol::Closed.value(),
        Some(ProcessStatusSymbol::Connect) => ProcessStatusSymbol::Connect.value(),
        Some(ProcessStatusSymbol::Failed) => ProcessStatusSymbol::Failed.value(),
        _ => Value::NIL,
    }
}

fn default_process_tty_name() -> String {
    // Fallback TTY name when the actual PTY slave path is not available.
    "/dev/pts/0".to_string()
}

// ---------------------------------------------------------------------------
// Bootstrap variables
// ---------------------------------------------------------------------------

pub fn register_bootstrap_vars(obarray: &mut super::symbol::Obarray) {
    obarray.set_symbol_value("process-connection-type", Value::T);
    obarray.make_special("process-connection-type");
    // GNU `process.c` `syms_of_process` DEFVAR_LISPs
    // `process-adaptive-read-buffering` (default nil); it controls the
    // short-read delay heuristic in `read_process_output` and is set per
    // process at `start-process'/`make-process' time
    // (`p->adaptive_read_buffering`).  It must be *bound* (to nil) so that
    // `(boundp 'process-adaptive-read-buffering)` is t and reading the
    // variable does not signal `void-variable`; e.g. `tramp-sh.el` binds it
    // with `(let ((process-adaptive-read-buffering nil)) ...)`.  Without this
    // DEFVAR, code that reads the variable before calling a (non-existent)
    // helper sees `void-variable` instead of reaching the real error.
    obarray.set_symbol_value("process-adaptive-read-buffering", Value::NIL);
    obarray.make_special("process-adaptive-read-buffering");
    obarray.set_symbol_value(
        "interrupt-process-functions",
        Value::list(vec![Value::symbol("internal-default-interrupt-process")]),
    );
    obarray.make_special("interrupt-process-functions");
    obarray.set_symbol_value(
        "signal-process-functions",
        Value::list(vec![Value::symbol("internal-default-signal-process")]),
    );
    obarray.make_special("signal-process-functions");
    obarray.set_symbol_value("internal--daemon-sockname", Value::NIL);
    obarray.make_special("internal--daemon-sockname");
    obarray.define_int_variable("read-process-output-max", 65536);
    obarray.define_int_variable("process-error-pause-time", 1);
    // GNU `gnutls.c` provides this via `DEFVAR_INT ("gnutls-log-level",
    // global_gnutls_log_level)` (default 0).  `gnutls.el` only forward-declares
    // it (`(defvar gnutls-log-level)  ; gnutls.c`), so without the C-side
    // definition it is void and `gnutls-negotiate` errors on
    // `:loglevel ,gnutls-log-level` before it ever reaches the (working,
    // TLS-capable) `gnutls-boot` -- breaking every package download and
    // thus `use-package`.  See https://github.com/eval-exec/neomacs/issues/121.
    obarray.define_int_variable("gnutls-log-level", 0);
    // GNU `gnutls.c` always DEFVAR_LISPs `libgnutls-version`; when Emacs is
    // built without libgnutls, the documented value is -1.  Neomacs exposes a
    // `gnutls-boot` compatibility API over Rust TLS rather than linking
    // libgnutls, so keep the variable bound without pretending to have a
    // libgnutls version.  `nsm.el` reads this during HTTPS package refresh.
    obarray.set_symbol_value("libgnutls-version", Value::fixnum(-1));
    obarray.make_special("libgnutls-version");
    for (symbol, code) in [
        ("gnutls-e-interrupted", -52),
        ("gnutls-e-again", -28),
        ("gnutls-e-invalid-session", -10),
        ("gnutls-e-not-ready-for-handshake", -65500),
    ] {
        obarray
            .put_property(symbol, "gnutls-code", Value::fixnum(code))
            .expect("bootstrap gnutls-code plist should be well formed");
    }
}

/// Check whether `process-connection-type` is truthy (non-nil).
///
/// GNU Emacs defaults this to `t`, meaning processes should use PTYs.
/// When nil, pipe-based I/O is used instead.
fn process_connection_type_is_pty(obarray: &super::symbol::Obarray) -> bool {
    match obarray.symbol_value("process-connection-type") {
        Some(v) if v.is_nil() => false,
        Some(_) => true,
        // Default is t (PTY) when the variable has not been set.
        None => true,
    }
}

fn signal_wrong_type_bufferp(value: Value) -> Flow {
    signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol("bufferp"), value],
    )
}

fn signal_wrong_type_threadp(value: Value) -> Flow {
    signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol("threadp"), value],
    )
}

fn signal_wrong_type_integerp(value: Value) -> Flow {
    signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol("integerp"), value],
    )
}

fn signal_wrong_type_numberp(value: Value) -> Flow {
    signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol("numberp"), value],
    )
}

fn signal_process_attributes_pid_range_error() -> Flow {
    signal(
        "error",
        vec![Value::string(
            "Not an in-range integer, integral float, or cons of integers",
        )],
    )
}

fn signal_undefined_signal_name(name: &str) -> Flow {
    signal(
        "error",
        vec![Value::string(format!("Undefined signal name {name}"))],
    )
}

fn resolve_optional_process_with_explicit_return_in_state(
    processes: &ProcessManager,
    buffers: &BufferManager,
    value: Option<&Value>,
) -> Result<(ProcessId, Value), Flow> {
    // `kill-process`, `stop-process`, `continue-process`, `quit-process` and
    // `interrupt-process' all reach GNU's `process_send_signal', whose TYPE
    // check precedes its `p->infd < 0' check -- and the two connection-shaped
    // subrs never reach it at all.  See
    // `is_stale_real_process_designator_in_manager'.
    if let Some(v) = value
        && !v.is_nil()
        && is_stale_real_process_designator_in_manager(processes, v)
        && let Some(id) = process_value_to_id(v)
    {
        return Err(signal_process_not_active_in_manager(processes, id));
    }
    if let Some(v) = value
        && !v.is_nil()
    {
        let id = resolve_get_process_designator_in_state(processes, buffers, v)?;
        return Ok((id, *v));
    }
    let id = resolve_optional_process_or_current_buffer_in_state(processes, buffers, value)?;
    Ok((id, Value::NIL))
}

enum SignalProcessTarget {
    Process(ProcessId),
    MissingNamedProcess,
    Pid(i64),
}

fn resolve_signal_process_target_in_state(
    processes: &ProcessManager,
    buffers: &BufferManager,
    value: Option<&Value>,
) -> Result<SignalProcessTarget, Flow> {
    if let Some(v) = value
        && !v.is_nil()
    {
        // A first-class process object designates that PROCESS, live or not.
        // GNU's `internal-default-signal-process` takes `XPROCESS
        // (process)->pid` (src/process.c:7380) after a plain `CHECK_PROCESS`
        // (:7379) -- no liveness test -- and raises "Cannot signal process %s"
        // when that pid is <= 0 (:7381-7382), which is what a pipe or network
        // process has.  Falling through to the pid branch here handed
        // `sys::send_signal` this port's `ProcessId` (1, 2, 3 ...) as though it
        // were an OS pid; before ledger 169 that was unreachable for a process
        // still in the alist, and retiring earlier made it reachable.
        if let Some(id) = v.as_process_id()
            && processes.get_any(id).is_some()
        {
            return Ok(SignalProcessTarget::Process(id));
        }
        return match v.kind() {
            ValueKind::String => {
                let name_str = process_owned_runtime_string(*v);
                Ok(match processes.find_by_name(&name_str) {
                    Some(id) => SignalProcessTarget::Process(id),
                    None => SignalProcessTarget::MissingNamedProcess,
                })
            }
            // GNU `Fsignal_process` treats a bare integer as a literal OS PID
            // and never looks it up: `internal-default-signal-process` calls
            // `get_process` only for a NON-number (src/process.c:7369-7370),
            // and a number goes straight to
            // `CONS_TO_INTEGER (process, pid_t, pid)` (:7375-7376).  The
            // docstring says the same (:7405-7407).
            //
            // Consulting the live process table here made the answer depend on
            // whether an unrelated process happened to hold that `ProcessId`.
            // Measured, `-Q --batch`, one live child, before this change:
            //
            //   (signal-process 1 0)   GNU -1 (EPERM from `kill (1, 0)`)
            //                          Neomacs 0 -- this port's process #1
            //
            // The comment above this arm already stated GNU's rule; the code
            // under it did not follow it.
            //
            // The domain is `NUMBERP`, not "non-negative fixnum": GNU calls
            // `get_process` only when `!NUMBERP (process)` (:7369-7370), so a
            // bignum, an integral float and a NEGATIVE integer all reach
            // `CONS_TO_INTEGER` and then `kill`.  A negative pid is a POSIX
            // process GROUP -- ledger 175, and ledger 169's fifth residual.
            ValueKind::Fixnum(_) | ValueKind::Float | ValueKind::Veclike(VecLikeType::Bignum) => {
                Ok(SignalProcessTarget::Pid(cons_to_os_pid(*v)?))
            }
            _ => Ok(SignalProcessTarget::Process(
                resolve_get_process_designator_in_state(processes, buffers, v)?,
            )),
        };
    }

    let id = resolve_optional_process_or_current_buffer_in_state(processes, buffers, value)?;
    Ok(SignalProcessTarget::Process(id))
}

fn parse_signal_number(value: &Value) -> Result<i32, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(n as i32),
        ValueKind::String => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), *value],
        )),
        _ => {
            // Borrow the symbol name before consuming it
            let sym_name = value.as_symbol_name().map(|s| s.to_owned());
            if let Some(name) = sym_name {
                sys::signal_name_number(&name).ok_or_else(|| signal_undefined_signal_name(&name))
            } else {
                Err(signal_wrong_type_integerp(*value))
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProcessSignalRecipient {
    ImmediateProcess,
    ProcessGroup,
}

/// GNU's `kill (p->pid, ...)` / `kill (- p->pid, ...)` for one of this port's
/// own children.
///
/// The pid comes from [`ChildOwnership::pid_if_unreaped`] and not from
/// `Process::os_pid`, and that IS GNU's `p->alive` gate.  `process_send_signal`
/// -- which every signal subr reaches -- is
///
/// ```c
///   /* Do not kill an already-reaped process, as that could kill an
///      innocent bystander that happens to have the same process ID.  */
///   block_child_signal (&oldset);
///   if (p->alive)
///     kill (pid, signo);                       /* src/process.c:7199-7205 */
///   unblock_child_signal (&oldset);
/// ```
///
/// and `Fdelete_process`'s is `if (p->alive) record_kill_process (p, Qnil);`
/// (:1134-1135, and src/callproc.c:202-207).  `os_pid` stays as GNU's `p->pid`,
/// the number `Fprocess_id` reports and GNU keeps after the reap; what goes
/// away with the child is the number that authorises a syscall.
fn deliver_process_signal(
    proc: &Process,
    signal_num: i32,
    recipient: ProcessSignalRecipient,
) -> i32 {
    let Some(pid) = proc.live_io.child.pid_if_unreaped() else {
        return -1;
    };
    match recipient {
        ProcessSignalRecipient::ImmediateProcess => sys::send_signal(pid as i64, signal_num),
        ProcessSignalRecipient::ProcessGroup => sys::send_signal_to_group(pid as i64, signal_num),
    }
}

fn process_has_subprocess_backing(proc: &Process) -> bool {
    proc.os_pid.is_some() || proc.live_io.child.has_child()
}

fn record_unbacked_real_process_signal(proc: &mut Process, signal_num: i32) -> bool {
    if proc.kind != ProcessKind::Real || process_has_subprocess_backing(proc) {
        return false;
    }
    proc.status = process_status_signal_value(signal_num);
    proc.status_notify_pending = false;
    proc.pending_status = Value::NIL;
    true
}

fn signal_process_or_unbacked_success(
    proc: &mut Process,
    signal_num: i32,
    recipient: ProcessSignalRecipient,
) -> i32 {
    let result = deliver_process_signal(proc, signal_num, recipient);
    if result == 0 {
        return 0;
    }
    if record_unbacked_real_process_signal(proc, signal_num) {
        return 0;
    }
    result
}

fn kill_real_process_child(proc: &mut Process, signal_num: i32) {
    if deliver_process_signal(proc, signal_num, ProcessSignalRecipient::ProcessGroup) == 0 {
        return;
    }
    if record_unbacked_real_process_signal(proc, signal_num) {
        return;
    }
    proc.live_io.child.kill_handle();
}

/// GNU's `wait_for_termination (child, NULL, ...)` (src/sysdep.c:500, called
/// that way at src/callproc.c:257) on a child that is already terminal or has
/// just been killed explicitly.
///
/// Dropping either Rust child handle closes the handle but does not perform
/// Unix `waitpid`, which leaves a zombie.  Call this only on the synchronous
/// delete path; normal status polling has already reaped naturally exited
/// children -- and, since ledger 187, has also given up the pid, so a child
/// that was reaped there is not waited a second time here.
fn wait_for_real_process_child_termination(proc: &mut Process) {
    proc.live_io.child.wait_for_termination();
}

fn signal_hup_number() -> i32 {
    cfg_select! {
        unix => { libc::SIGHUP }
        _ => { 1 }
    }
}

fn signal_kill_number() -> i32 {
    cfg_select! {
        unix => { libc::SIGKILL }
        _ => { 9 }
    }
}

fn ticks_to_secs_usecs(ticks: i64, hz: i64) -> (i64, i64) {
    if hz <= 0 {
        return (0, 0);
    }
    let secs = ticks.div_euclid(hz);
    let rem = ticks.rem_euclid(hz);
    let usecs = ((rem as i128) * 1_000_000i128 / (hz as i128)) as i64;
    (secs, usecs)
}

fn time_list_from_secs_usecs(secs: i64, usecs: i64) -> Value {
    let high = (secs >> 16) & 0xFFFF_FFFF;
    let low = secs & 0xFFFF;
    Value::list(vec![
        Value::fixnum(high),
        Value::fixnum(low),
        Value::fixnum(usecs.clamp(0, 999_999)),
        Value::fixnum(0),
    ])
}

fn time_list_from_ticks(ticks: i64, hz: i64) -> Value {
    let (secs, usecs) = ticks_to_secs_usecs(ticks, hz);
    time_list_from_secs_usecs(secs, usecs)
}

fn now_epoch_secs_usecs() -> Option<(i64, i64)> {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(dur) => Some((dur.as_secs() as i64, dur.subsec_micros() as i64)),
        Err(_) => None,
    }
}

fn nonnegative_time_diff(now: (i64, i64), then: (i64, i64)) -> (i64, i64) {
    let (now_secs, now_usecs) = now;
    let (then_secs, then_usecs) = then;
    if (now_secs, now_usecs) < (then_secs, then_usecs) {
        return (0, 0);
    }
    let mut secs = now_secs - then_secs;
    let mut usecs = now_usecs - then_usecs;
    if usecs < 0 {
        secs -= 1;
        usecs += 1_000_000;
    }
    (secs, usecs)
}

fn parse_make_process_command(value: &Value) -> Result<Vec<LispString>, Flow> {
    let as_vec: Option<Vec<Value>> = match value.kind() {
        ValueKind::Veclike(VecLikeType::Vector) => Some(value.as_vector_data().unwrap().clone()),
        ValueKind::Cons | ValueKind::Nil => list_to_vec(value),
        _ => None,
    };

    let Some(items) = as_vec else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("sequencep"), *value],
        ));
    };

    items
        .into_iter()
        .map(|item| {
            super::builtins::expect_lisp_string(&item)
                .cloned()
                .map_err(|_| signal_wrong_type_string(item))
        })
        .collect()
}

fn parse_make_process_buffer(
    eval: &mut super::eval::Context,
    value: &Value,
) -> Result<Value, Flow> {
    parse_make_process_buffer_in_state(&mut eval.buffers, value)
}

/// Every process constructor's `:buffer`, which in GNU is one call to
/// `Fget_buffer_create` -- `Fmake_process` at src/process.c:1849-1851,
/// `Fmake_pipe_process` at :3091-3094, `Fmake_serial_process` at :3223-3226
/// and `Fmake_network_process` at :4017.
///
/// A buffer OBJECT comes back as it was handed in, and GNU's docstring says so
/// in as many words: "If BUFFER-OR-NAME is a buffer instead of a string,
/// return it as given, even if it is dead.  The return value is never nil."
/// (src/buffer.c:581-582.)  That is not an oversight of GNU's -- it is the
/// state three later `BUFFER_LIVE_P` tests exist to handle:
/// `read_and_insert_process_output` (:6464),
/// `internal-default-process-sentinel`, whose own comment is "Avoid error if
/// buffer is deleted (probably that's why the process is dead, too)"
/// (:7969-7971), and `setup_process_coding_systems` (:8395).  Refusing the
/// dead buffer here made all three unreachable and turned `make-process` into
/// a signal where GNU returns a process.
fn parse_make_process_buffer_in_state(
    buffers: &mut BufferManager,
    value: &Value,
) -> Result<Value, Flow> {
    match value.kind() {
        ValueKind::Nil => Ok(Value::NIL),
        ValueKind::String => {
            let name_str = process_owned_runtime_string(*value);
            let id = buffers
                .find_buffer_by_name(&name_str)
                .unwrap_or_else(|| buffers.create_buffer(&name_str));
            Ok(Value::make_buffer(id))
        }
        ValueKind::Veclike(VecLikeType::Buffer) => Ok(*value),
        _ => Err(signal_wrong_type_string(*value)),
    }
}

fn expect_integer(value: &Value) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        _ => Err(signal_wrong_type_integerp(*value)),
    }
}

fn expect_ushort_dimension(value: &Value) -> Result<u16, Flow> {
    let n = expect_integer(value)?;
    u16::try_from(n).map_err(|_| {
        signal(
            LispCondition::ArgsOutOfRange,
            vec![*value, Value::fixnum(0), Value::fixnum(i64::from(u16::MAX))],
        )
    })
}

fn value_as_nonnegative_integer(value: &Value) -> Option<i64> {
    match value.kind() {
        ValueKind::Fixnum(n) if n >= 0 => Some(n),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
enum NetworkAddressFamily {
    #[strum(serialize = "ipv4")]
    Ipv4,
    #[strum(serialize = "ipv6")]
    Ipv6,
}

impl NetworkAddressFamily {
    fn from_symbol_value(value: &Value) -> Option<Self> {
        value.as_symbol_name()?.parse().ok()
    }

    #[cfg(test)]
    fn name(self) -> &'static str {
        self.into()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NetworkProcessFamily {
    Unspecified,
    Local,
    Ipv4,
    Ipv6,
    Raw(i32),
}

impl NetworkProcessFamily {
    fn is_local(self) -> bool {
        self == Self::Local
    }

    fn loopback_host(self) -> &'static str {
        match self {
            Self::Ipv6 => "::1",
            _ => "127.0.0.1",
        }
    }

    fn addrinfo_family(self) -> i32 {
        match self {
            Self::Unspecified => 0,
            Self::Local => sys::net::af_local(),
            Self::Ipv4 => sys::net::af_inet(),
            Self::Ipv6 => sys::net::af_inet6(),
            Self::Raw(raw) => raw,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
enum NetworkProcessFamilySymbol {
    Local,
    Ipv4,
    Ipv6,
}

impl NetworkProcessFamilySymbol {
    fn from_symbol_value(value: &Value) -> Option<Self> {
        value.as_symbol_name()?.parse().ok()
    }

    #[cfg(test)]
    fn name(self) -> &'static str {
        self.into()
    }
}

fn parse_network_host(value: &Value, family: NetworkProcessFamily) -> Result<Option<String>, Flow> {
    if value.is_nil() {
        return Ok(None);
    }
    if value.as_symbol_name() == Some("local") {
        return Ok(Some(family.loopback_host().to_string()));
    }
    match value.kind() {
        ValueKind::String => Ok(Some(process_owned_runtime_string(*value))),
        _ => Err(signal_wrong_type_string(*value)),
    }
}

fn network_service_protocol(socket_type: NetworkSocketType) -> &'static str {
    match socket_type {
        NetworkSocketType::Datagram => "udp",
        _ => "tcp",
    }
}

fn parse_network_numeric_service_port(service: &str) -> Option<u16> {
    let service = service.trim_start_matches(|ch: char| ch.is_ascii_whitespace());
    let service = service.strip_prefix('+').unwrap_or(service);
    if service.is_empty() || !service.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    service
        .parse::<u128>()
        .ok()
        .map(|port| (port % (1 << 16)) as u16)
}

fn parse_network_service_port(
    value: &Value,
    server: bool,
    socket_type: NetworkSocketType,
) -> Result<u16, Flow> {
    match value.kind() {
        ValueKind::T if server => Ok(0),
        ValueKind::Fixnum(port) if port >= 0 => Ok((port as u64 % (1 << 16)) as u16),
        ValueKind::String => {
            let service = process_owned_runtime_string(*value);
            if service.is_empty() {
                return Ok(0);
            }
            if let Some(port) = parse_network_numeric_service_port(&service) {
                return Ok(port);
            }
            sys::net::service_port(&service, network_service_protocol(socket_type)).ok_or_else(
                || {
                    signal(
                        "error",
                        vec![Value::string(format!("Unknown service: {}", service))],
                    )
                },
            )
        }
        _ => Err(signal_wrong_type_string(*value)),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum NetworkAddressSpec {
    Inet(SocketAddr),
    #[cfg(unix)]
    Local(std::path::PathBuf),
}

fn parse_network_address_spec(value: &Value) -> Result<NetworkAddressSpec, Flow> {
    #[cfg(unix)]
    if matches!(value.kind(), ValueKind::String) {
        return Ok(NetworkAddressSpec::Local(
            crate::emacs_core::fileio::lisp_file_name_to_path_buf(
                super::builtins::expect_lisp_string(value)?,
            ),
        ));
    }

    let Some(items) = value.as_vector_data() else {
        return Err(signal("error", vec![Value::string("Malformed :address")]));
    };

    match items.len() {
        5 => {
            let a = parse_lisp_sockaddr_part(items[0], 255)?;
            let b = parse_lisp_sockaddr_part(items[1], 255)?;
            let c = parse_lisp_sockaddr_part(items[2], 255)?;
            let d = parse_lisp_sockaddr_part(items[3], 255)?;
            let port = parse_lisp_sockaddr_part(items[4], u16::MAX as i64)?;
            Ok(NetworkAddressSpec::Inet(SocketAddr::new(
                IpAddr::V4(Ipv4Addr::new(a as u8, b as u8, c as u8, d as u8)),
                port as u16,
            )))
        }
        9 => {
            let mut segments = [0_u16; 8];
            for (idx, segment) in segments.iter_mut().enumerate() {
                *segment = parse_lisp_sockaddr_part(items[idx], u16::MAX as i64)? as u16;
            }
            let port = parse_lisp_sockaddr_part(items[8], u16::MAX as i64)?;
            Ok(NetworkAddressSpec::Inet(SocketAddr::new(
                IpAddr::V6(Ipv6Addr::new(
                    segments[0],
                    segments[1],
                    segments[2],
                    segments[3],
                    segments[4],
                    segments[5],
                    segments[6],
                    segments[7],
                )),
                port as u16,
            )))
        }
        _ => Err(signal("error", vec![Value::string("Malformed :address")])),
    }
}

fn parse_lisp_sockaddr_part(value: Value, max: i64) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) if (0..=max).contains(&n) => Ok(n),
        _ => Err(signal("error", vec![Value::string("Malformed :address")])),
    }
}

fn socket_addr_to_lisp_value(addr: SocketAddr) -> Value {
    match addr {
        SocketAddr::V4(v4) => {
            let octets = v4.ip().octets();
            int_vector(&[
                octets[0] as i64,
                octets[1] as i64,
                octets[2] as i64,
                octets[3] as i64,
                v4.port() as i64,
            ])
        }
        SocketAddr::V6(v6) => {
            let segments = v6.ip().segments();
            let mut vals = [0_i64; 9];
            for (idx, &seg) in segments.iter().enumerate() {
                vals[idx] = seg as i64;
            }
            vals[8] = v6.port() as i64;
            int_vector(&vals)
        }
    }
}

#[cfg(unix)]
fn unix_socket_addr_to_runtime_string(addr: Option<UnixSocketAddr>) -> String {
    addr.and_then(|addr| {
        addr.as_pathname()
            .map(|path| path.as_os_str().to_string_lossy().into_owned())
    })
    .unwrap_or_default()
}

#[cfg(unix)]
fn socket2_unix_sockaddr_to_runtime_string(addr: Option<&SockAddr>) -> String {
    addr.and_then(|addr| {
        addr.as_pathname()
            .map(|path| path.as_os_str().to_string_lossy().into_owned())
    })
    .unwrap_or_default()
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn validate_network_process_family(value: &Value) -> Result<(), Flow> {
    if value.is_nil()
        || matches!(value.kind(), ValueKind::Fixnum(_))
        || NetworkProcessFamilySymbol::from_symbol_value(value).is_some()
    {
        Ok(())
    } else {
        Err(signal(
            "error",
            vec![Value::string("Unknown address family")],
        ))
    }
}

fn parse_network_process_family(value: &Value) -> Result<NetworkProcessFamily, Flow> {
    if value.is_nil() {
        return Ok(NetworkProcessFamily::Unspecified);
    }
    match NetworkProcessFamilySymbol::from_symbol_value(value) {
        Some(NetworkProcessFamilySymbol::Local) => return Ok(NetworkProcessFamily::Local),
        Some(NetworkProcessFamilySymbol::Ipv4) => return Ok(NetworkProcessFamily::Ipv4),
        Some(NetworkProcessFamilySymbol::Ipv6) => return Ok(NetworkProcessFamily::Ipv6),
        None => {}
    }
    if let ValueKind::Fixnum(raw) = value.kind() {
        return network_process_family_from_raw(raw)
            .ok_or_else(|| signal("error", vec![Value::string("Unknown address family")]));
    }
    Err(signal(
        "error",
        vec![Value::string("Unknown address family")],
    ))
}

fn network_process_family_from_raw(raw: i64) -> Option<NetworkProcessFamily> {
    let raw = i32::try_from(raw).ok()?;
    Some(match sys::net::classify_family(raw) {
        sys::net::NetFamily::Unspecified => NetworkProcessFamily::Unspecified,
        sys::net::NetFamily::Ipv4 => NetworkProcessFamily::Ipv4,
        sys::net::NetFamily::Ipv6 => NetworkProcessFamily::Ipv6,
        sys::net::NetFamily::Local => NetworkProcessFamily::Local,
        sys::net::NetFamily::Other(r) => NetworkProcessFamily::Raw(r),
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
enum NetworkLookupHint {
    Numeric,
}

impl NetworkLookupHint {
    fn from_symbol_value(value: &Value) -> Option<Self> {
        value.as_symbol_name()?.parse().ok()
    }

    fn addrinfo_flags(self) -> i32 {
        match self {
            Self::Numeric => sys::net::ai_numerichost(),
        }
    }

    #[cfg(test)]
    fn name(self) -> &'static str {
        self.into()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
enum NumProcessorsQuery {
    All,
    Current,
}

impl NumProcessorsQuery {
    fn from_symbol_value(value: &Value) -> Option<Self> {
        value.as_symbol_name()?.parse().ok()
    }

    #[cfg(test)]
    fn name(self) -> &'static str {
        self.into()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NetworkSocketType {
    Stream,
    Datagram,
    #[cfg(unix)]
    Seqpacket,
}

fn parse_network_socket_type(value: &Value) -> Result<NetworkSocketType, Flow> {
    match value.as_symbol_name() {
        _ if value.is_nil() => Ok(NetworkSocketType::Stream),
        Some("datagram") => Ok(NetworkSocketType::Datagram),
        #[cfg(unix)]
        Some("seqpacket") => Ok(NetworkSocketType::Seqpacket),
        Some(_) | None => Err(signal(
            "error",
            vec![Value::string("Unsupported connection type")],
        )),
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn validate_network_socket_type(value: &Value) -> Result<(), Flow> {
    parse_network_socket_type(value).map(|_| ())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
enum ProcessConnectionType {
    Pipe,
    Pty,
}

impl ProcessConnectionType {
    fn from_symbol_value(value: &Value) -> Option<Self> {
        value.as_symbol_name()?.parse().ok()
    }

    fn uses_pty(self) -> bool {
        matches!(self, Self::Pty)
    }
}

fn resolve_process_connection_type_use_pty(
    connection_type: Option<&Value>,
    default_use_pty: bool,
) -> Result<bool, Flow> {
    match connection_type {
        None => Ok(default_use_pty),
        Some(value) if value.is_nil() => Ok(default_use_pty),
        Some(value) => ProcessConnectionType::from_symbol_value(value)
            .map(ProcessConnectionType::uses_pty)
            .ok_or_else(|| {
                // GNU `is_pty_from_symbol` (process.c) signals this through
                // `report_file_error ("Unknown connection type", symbol)`, which
                // reads the live `errno`.  At this point in `make-process` (before
                // any program lookup) the residual errno is ENOENT, so GNU emits
                // `(file-missing "Unknown connection type" "No such file or
                // directory" SYMBOL)`.  Match that data list exactly.
                signal_file_errno("Unknown connection type", *value, libc::ENOENT)
            }),
    }
}

#[derive(Clone, Debug)]
struct HostInterfaceEntry {
    name: String,
    family: NetworkAddressFamily,
    address: Value,
    list_broadcast: Value,
    info_broadcast: Value,
    netmask: Value,
    hwaddr: Option<Value>,
    flags: Value,
}

fn vector_nonnegative_integers(value: &Value) -> Option<Vec<i64>> {
    if !value.is_vector() {
        return None;
    };
    let locked = value.as_vector_data().unwrap().clone();
    let mut out = Vec::with_capacity(locked.len());
    for item in locked.iter() {
        out.push(value_as_nonnegative_integer(item)?);
    }
    Some(out)
}

fn int_vector(values: &[i64]) -> Value {
    Value::vector(values.iter().map(|v| Value::fixnum(*v)).collect())
}

fn loopback_ipv4_address() -> Value {
    int_vector(&[127, 0, 0, 1, 0])
}

fn loopback_ipv4_broadcast() -> Value {
    int_vector(&[0, 0, 0, 0, 0])
}

fn loopback_ipv4_netmask() -> Value {
    int_vector(&[255, 0, 0, 0, 0])
}

fn loopback_ipv6_address() -> Value {
    int_vector(&[0, 0, 0, 0, 0, 0, 0, 1, 0])
}

fn loopback_ipv6_broadcast() -> Value {
    int_vector(&[0, 0, 0, 0, 0, 0, 0, 1, 0])
}

fn loopback_ipv6_netmask() -> Value {
    int_vector(&[65535, 65535, 65535, 65535, 65535, 65535, 65535, 65535, 0])
}

fn loopback_hwaddr() -> Value {
    Value::cons(Value::fixnum(772), int_vector(&[0, 0, 0, 0, 0, 0]))
}

fn loopback_flags() -> Value {
    Value::list(vec![
        Value::symbol("running"),
        Value::symbol("loopback"),
        Value::symbol("up"),
    ])
}

fn zero_network_address(family: NetworkAddressFamily) -> Value {
    match family {
        NetworkAddressFamily::Ipv4 => int_vector(&[0, 0, 0, 0, 0]),
        NetworkAddressFamily::Ipv6 => int_vector(&[0, 0, 0, 0, 0, 0, 0, 0, 0]),
    }
}

fn network_directed_broadcast(
    family: NetworkAddressFamily,
    address: &Value,
    netmask: &Value,
) -> Option<Value> {
    let address_items = vector_nonnegative_integers(address)?;
    let netmask_items = vector_nonnegative_integers(netmask)?;
    match family {
        NetworkAddressFamily::Ipv4 => {
            if address_items.len() != 5 || netmask_items.len() != 5 {
                return None;
            }
            let mut out = [0_i64; 5];
            for idx in 0..4 {
                let addr = u8::try_from(address_items[idx]).ok()?;
                let mask = u8::try_from(netmask_items[idx]).ok()?;
                out[idx] = (addr | !mask) as i64;
            }
            Some(int_vector(&out))
        }
        NetworkAddressFamily::Ipv6 => {
            if address_items.len() != 9 || netmask_items.len() != 9 {
                return None;
            }
            let mut out = [0_i64; 9];
            for idx in 0..8 {
                let addr = u16::try_from(address_items[idx]).ok()?;
                let mask = u16::try_from(netmask_items[idx]).ok()?;
                out[idx] = (addr | !mask) as i64;
            }
            Some(int_vector(&out))
        }
    }
}

fn derive_network_interface_list_broadcast(
    family: NetworkAddressFamily,
    address: &Value,
    netmask: &Value,
    raw_broadcast: &Value,
) -> Value {
    network_directed_broadcast(family, address, netmask).unwrap_or(*raw_broadcast)
}

fn derive_network_interface_info_broadcast(
    family: NetworkAddressFamily,
    address: &Value,
    raw_broadcast: &Value,
) -> Value {
    if raw_broadcast == address {
        zero_network_address(family)
    } else {
        *raw_broadcast
    }
}

fn ip_to_value(ip: IpAddr) -> (NetworkAddressFamily, Value) {
    match ip {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            (
                NetworkAddressFamily::Ipv4,
                int_vector(&[
                    octets[0] as i64,
                    octets[1] as i64,
                    octets[2] as i64,
                    octets[3] as i64,
                    0,
                ]),
            )
        }
        IpAddr::V6(v6) => {
            let segments = v6.segments();
            let mut vals = [0_i64; 9];
            for (idx, &seg) in segments.iter().enumerate() {
                vals[idx] = seg as i64;
            }
            (NetworkAddressFamily::Ipv6, int_vector(&vals))
        }
    }
}

fn resolve_network_lookup_addresses(
    name: &str,
    family: Option<NetworkAddressFamily>,
    hint: Option<NetworkLookupHint>,
) -> Vec<Value> {
    use dns_lookup::{AddrFamily, AddrInfoHints, SockType};

    // Emacs forwards names through C APIs where embedded NUL terminates the
    // effective hostname. Match that behavior instead of rejecting interior NUL.
    let normalized_name = name.split('\0').next().unwrap_or_default();

    let hints = AddrInfoHints {
        flags: hint.map_or(0, NetworkLookupHint::addrinfo_flags),
        socktype: SockType::DGram.into(),
        address: match family {
            Some(NetworkAddressFamily::Ipv4) => AddrFamily::Inet.into(),
            Some(NetworkAddressFamily::Ipv6) => AddrFamily::Inet6.into(),
            None => 0, // AF_UNSPEC
        },
        ..AddrInfoHints::default()
    };

    let addrs = match dns_lookup::getaddrinfo(Some(normalized_name), None, Some(hints)) {
        Ok(iter) => iter,
        Err(_) => return Vec::new(),
    };

    let mut out = Vec::new();
    for result in addrs {
        let info = match result {
            Ok(info) => info,
            Err(_) => continue,
        };
        let (resolved_family, address) = ip_to_value(info.sockaddr.ip());
        let include = match family {
            Some(expected) => expected == resolved_family,
            None => true,
        };
        if include {
            out.push(address);
        }
    }

    out
}

fn interface_entry(name: &str, address: Value, full: bool) -> Value {
    if !full {
        return Value::cons(Value::string(name), address);
    }

    let (broadcast, netmask) = match address.kind() {
        ValueKind::Veclike(VecLikeType::Vector) if address.as_vector_data().unwrap().len() == 9 => {
            (loopback_ipv6_broadcast(), loopback_ipv6_netmask())
        }
        _ => (loopback_ipv4_broadcast(), loopback_ipv4_netmask()),
    };

    Value::list(vec![Value::string(name), address, broadcast, netmask])
}

fn format_ipv4_network_address(items: &[i64], omit_port: bool) -> Option<String> {
    if items.len() != 4 && items.len() != 5 {
        return None;
    }
    let octets: Vec<u8> = items[..4]
        .iter()
        .map(|v| u8::try_from(*v).ok())
        .collect::<Option<Vec<_>>>()?;
    let addr = format!("{}.{}.{}.{}", octets[0], octets[1], octets[2], octets[3]);
    if items.len() == 5 && !omit_port {
        let port = u16::try_from(items[4]).ok()?;
        Some(format!("{addr}:{port}"))
    } else {
        Some(addr)
    }
}

fn format_ipv6_network_address(items: &[i64], omit_port: bool) -> Option<String> {
    if items.len() != 8 && items.len() != 9 {
        return None;
    }
    let mut segments = Vec::with_capacity(8);
    for value in &items[..8] {
        let segment = u16::try_from(*value).ok()?;
        segments.push(format!("{segment:x}"));
    }
    let addr = segments.join(":");
    if items.len() == 9 && !omit_port {
        let port = u16::try_from(items[8]).ok()?;
        Some(format!("[{addr}]:{port}"))
    } else {
        Some(addr)
    }
}

// ---------------------------------------------------------------------------
// Builtins (eval-dependent)
// ---------------------------------------------------------------------------

/// (internal-default-interrupt-process &optional PROCESS CURRENT-GROUP) -> process-or-nil
pub(crate) fn builtin_internal_default_interrupt_process(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_internal_default_interrupt_process_impl(&mut eval.processes, &eval.buffers, args)
}

pub(crate) fn builtin_internal_default_interrupt_process_impl(
    processes: &mut ProcessManager,
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() > 2 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("internal-default-interrupt-process"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let (id, ret) =
        resolve_optional_process_with_explicit_return_in_state(processes, buffers, args.first())?;
    check_process_is_real_subprocess(processes, id)?;
    if let Some(proc) = processes.get_mut(id) {
        #[cfg(unix)]
        let _ = signal_process_or_unbacked_success(
            proc,
            libc::SIGINT,
            ProcessSignalRecipient::ProcessGroup,
        );
    }
    Ok(ret)
}

/// (internal-default-signal-process PROCESS SIGNAL &optional CURRENT-GROUP) -> int-or-nil
pub(crate) fn builtin_internal_default_signal_process(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_internal_default_signal_process_impl(&mut eval.processes, &eval.buffers, args)
}

pub(crate) fn builtin_internal_default_signal_process_impl(
    processes: &mut ProcessManager,
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("internal-default-signal-process", &args, 2)?;
    if args.len() > 3 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("internal-default-signal-process"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    let signal_num = parse_signal_number(&args[1])?;
    match resolve_signal_process_target_in_state(processes, buffers, args.first())? {
        SignalProcessTarget::Process(id) => {
            // `get_any_mut`, not `get_mut`: GNU's `CHECK_PROCESS` +
            // `XPROCESS (process)->pid` (src/process.c:7379-7382) does not ask
            // whether the process is still in `Vprocess_alist`.
            if let Some(proc) = processes.get_any_mut(id) {
                if proc.kind != ProcessKind::Real {
                    return Err(signal_cannot_signal_process(proc));
                }
                return Ok(Value::fixnum(signal_process_or_unbacked_success(
                    proc,
                    signal_num,
                    ProcessSignalRecipient::ImmediateProcess,
                ) as i64));
            }
            Ok(Value::fixnum(-1))
        }
        SignalProcessTarget::MissingNamedProcess => Ok(Value::NIL),
        SignalProcessTarget::Pid(pid) => {
            Ok(Value::fixnum(sys::send_signal(pid, signal_num) as i64))
        }
    }
}

fn process_mark_insert_emacs_byte_pos(
    buffers: &BufferManager,
    buf_id: BufferId,
    mark: Value,
) -> EmacsBytePos {
    match super::marker::marker_position_as_int_with_buffers(buffers, &mark) {
        Ok(pos) => buffers
            .get(buf_id)
            .map(|b| b.lisp_pos_to_full_buffer_emacs_byte_pos(LispCharPos1::new(pos)))
            .unwrap_or(EmacsBytePos::ZERO),
        Err(_) => buffers
            .get(buf_id)
            .map(|b| b.full_emacs_byte_range().end())
            .unwrap_or(EmacsBytePos::ZERO),
    }
}

fn adjusted_process_output_point(
    old_point: EmacsBytePos,
    insert_pos: EmacsBytePos,
    inserted_len: EmacsByteLen,
) -> EmacsBytePos {
    if old_point >= insert_pos {
        old_point.add_len(inserted_len)
    } else {
        old_point
    }
}

/// The two GNU-owned writes performed by the default process callbacks.
///
/// The variants deliberately encode the observable current-buffer contract:
/// calling `internal-default-process-filter` directly leaves the process
/// buffer current, while the default sentinel restores its caller's buffer.
/// Keeping that distinction in an enum makes adding a third, accidentally
/// ambiguous write policy a compile-time decision instead of another boolean.
enum DefaultProcessBufferInsertion<'a> {
    Output(&'a LispString),
    StatusMessage(&'a str),
}

impl DefaultProcessBufferInsertion<'_> {
    fn restores_callers_current_buffer(&self) -> bool {
        matches!(self, Self::StatusMessage(_))
    }

    fn fallback_byte_len(&self) -> usize {
        match self {
            Self::Output(text) => text.sbytes(),
            Self::StatusMessage(text) => text.len(),
        }
    }
}

/// Insert one default process callback payload at the process marker.
///
/// This is the NeoVM counterpart of GNU's process-buffer insertion state
/// machine in `read_process_output_before_insert` /
/// `read_process_output_after_insert` and
/// `internal-default-process-sentinel` (`src/process.c`).  The module owns the
/// target-buffer switch, read-only override, point adjustment, marker advance,
/// and callback-specific current-buffer restoration as one deep operation.
fn insert_default_process_buffer_payload(
    eval: &mut super::eval::Context,
    id: ProcessId,
    insertion: DefaultProcessBufferInsertion<'_>,
) -> EvalResult {
    let (buffer, mark) = match eval.processes.get_any(id) {
        Some(process) => (process.buffer, process.mark),
        None => return Ok(Value::NIL),
    };
    let Some(buffer_id) = buffer.as_buffer_id() else {
        return Ok(Value::NIL);
    };
    if eval.buffers.get(buffer_id).is_none() {
        return Ok(Value::NIL);
    }

    let saved_current = insertion
        .restores_callers_current_buffer()
        .then(|| eval.buffers.current_buffer_id())
        .flatten();
    eval.set_current_buffer_unrecorded(buffer_id)?;

    let insert_pos = process_mark_insert_emacs_byte_pos(&eval.buffers, buffer_id, mark);
    let saved_point = eval
        .buffers
        .get(buffer_id)
        .map(|buffer| buffer.point_emacs_byte_pos());
    let saved_read_only = eval
        .buffers
        .get(buffer_id)
        .map(|buffer| buffer.get_read_only());

    if let Some(buffer) = eval.buffers.get_mut(buffer_id) {
        buffer.set_read_only_value(false);
    }
    let _ = eval
        .buffers
        .goto_buffer_emacs_byte_pos(buffer_id, insert_pos);

    let fallback_byte_len = insertion.fallback_byte_len();
    match insertion {
        DefaultProcessBufferInsertion::Output(text) => {
            let change = super::editfns::text_change_for_empty_insertion_at_emacs_byte_pos(
                &eval.buffers,
                buffer_id,
                insert_pos,
                super::editfns::lisp_string_text_extent(text),
            )?;
            super::editfns::signal_before_text_change(eval, change)?;
            eval.buffers
                .insert_lisp_string_into_buffer_before_markers(buffer_id, text);
            super::editfns::signal_after_text_change(eval, change)?;
        }
        DefaultProcessBufferInsertion::StatusMessage(text) => {
            let _ = eval
                .buffers
                .insert_into_buffer_before_markers(buffer_id, text);
        }
    }

    let new_mark = eval
        .buffers
        .get(buffer_id)
        .map(|buffer| buffer.point_emacs_byte_pos())
        .unwrap_or(insert_pos.add_len(EmacsByteLen::new(fallback_byte_len)));

    if let (Some(buffer), Some(read_only)) = (eval.buffers.get_mut(buffer_id), saved_read_only) {
        buffer.set_read_only_value(read_only);
    }

    let inserted_len = EmacsByteLen::new(new_mark.get().saturating_sub(insert_pos.get()));
    if let Some(old_point) = saved_point {
        let adjusted_point = adjusted_process_output_point(old_point, insert_pos, inserted_len);
        let _ = eval
            .buffers
            .goto_buffer_emacs_byte_pos(buffer_id, adjusted_point);
    }

    if let Some(process) = eval.processes.get_any_mut(id) {
        let new_mark_pos = eval
            .buffers
            .get(buffer_id)
            .map(|buffer| Value::fixnum(buffer.emacs_byte_pos_to_lisp_char_pos(new_mark).as_i64()))
            .unwrap_or(Value::NIL);
        super::marker::builtin_set_marker_in_buffers(
            &mut eval.buffers,
            vec![process.mark, new_mark_pos, process.buffer],
        )?;
    }

    if let Some(saved_buffer_id) = saved_current {
        eval.restore_current_buffer_if_live(saved_buffer_id);
    }

    Ok(Value::NIL)
}

/// (internal-default-process-filter PROCESS STRING) -> nil
///
/// When no custom filter is set, insert output into the process's associated
/// buffer at the process mark position (or end of buffer when mark is None).
/// This matches GNU Emacs's `internal-default-process-filter` behavior.
pub(crate) fn builtin_internal_default_process_filter(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("internal-default-process-filter", &args, 2)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(&eval.processes, &args[0])?;
    let text = match args[1].as_lisp_string() {
        Some(text) => text.clone(),
        None => return Err(signal_wrong_type_string(args[1])),
    };
    if text.is_empty() {
        return Ok(Value::NIL);
    }

    // GNU `read_process_output_before_insert` (src/process.c) makes the PROCESS
    // buffer current before touching anything: `Fset_buffer (p->buffer)`. Every
    // check the insertion then runs -- above all the read-only barf in
    // `prepare_to_modify_buffer` -- therefore tests the buffer being written, not
    // whatever buffer happened to be current when output arrived.
    //
    // Without it, a filter called while an unrelated read-only buffer is current
    // signals `buffer-read-only' for THAT buffer: magit-blame calls this filter
    // with the blamed (read-only) source buffer current, so every blame chunk
    // errored and no blame information ever appeared (neomacs#192).
    //
    // GNU does not restore the current buffer here either; its caller
    // (`read_process_output`) unwinds it, which
    // `run_async_process_callback_preserving_state` mirrors.
    insert_default_process_buffer_payload(eval, id, DefaultProcessBufferInsertion::Output(&text))
}

/// (internal-default-process-sentinel PROCESS STRING) -> nil
pub(crate) fn builtin_internal_default_process_sentinel(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("internal-default-process-sentinel", &args, 2)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(&eval.processes, &args[0])?;
    let msg = expect_string_strict(&args[1])?;

    let (name, status_symbol) = match eval.processes.get_any(id) {
        Some(process) => (
            process_name_runtime(process.name),
            ProcessStatusSymbol::from_status_value(process.status),
        ),
        None => return Err(signal_wrong_type_processp(args[0])),
    };

    if status_symbol == Some(ProcessStatusSymbol::Run) {
        return Ok(Value::NIL);
    }

    let text = format!("\nProcess {name} {msg}");
    insert_default_process_buffer_payload(
        eval,
        id,
        DefaultProcessBufferInsertion::StatusMessage(&text),
    )
}

/// (gnutls-boot PROCESS TYPE PROPLIST) -> t or error
///
/// Upgrade a network process to TLS through the GNU-compatible `gnutls-boot` API.
/// PROCESS must be a network process with an open TCP socket.
/// TYPE is the credential type.  PROPLIST is a keyword plist; `:hostname`
/// supplies SNI and certificate hostname validation, while `:trustfiles`
/// supplies additional PEM trust anchors.
pub(crate) fn builtin_gnutls_boot(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("gnutls-boot", &args, 3)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(&eval.processes, &args[0])?;
    let parameters = parse_gnutls_boot_parameters(args[1], args[2])?;
    upgrade_process_to_tls::<RustlsBackend>(
        &mut eval.processes,
        id,
        &parameters.client,
        "gnutls-boot",
        signal_gnutls_boot_error,
    )?;

    Ok(Value::T)
}

pub(crate) fn builtin_gnutls_asynchronous_parameters(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("gnutls-asynchronous-parameters", &args, 2)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(&eval.processes, &args[0])?;
    let proc = eval
        .processes
        .get_mut(id)
        .ok_or_else(|| signal("error", vec![Value::string("Process not found")]))?;
    proc.gnutls_boot_parameters = args[1];
    Ok(Value::NIL)
}

pub(crate) fn builtin_gnutls_bye(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("gnutls-bye", &args, 2)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(&eval.processes, &args[0])?;
    let proc = eval
        .processes
        .get_mut(id)
        .ok_or_else(|| signal("error", vec![Value::string("Process not found")]))?;
    let Some(tls_stream) = proc.live_io.tls_stream.as_mut() else {
        return Ok(Value::NIL);
    };
    match tls_stream.send_close_notify(args[1].is_nil()) {
        Ok(result) => Ok(gnutls_close_notify_result_value(result)),
        Err(err) => Err(signal_process_io("gnutls-bye", None, err)),
    }
}

pub(crate) fn builtin_gnutls_deinit(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("gnutls-deinit", &args, 1)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(&eval.processes, &args[0])?;
    let proc = eval
        .processes
        .get_mut(id)
        .ok_or_else(|| signal("error", vec![Value::string("Process not found")]))?;
    if proc.live_io.tls_stream.take().is_some() {
        proc.gnutls_initstage = GnutlsInitStage::Callbacks;
        proc.gnutls_boot_parameters = Value::NIL;
        Ok(Value::T)
    } else {
        Ok(Value::NIL)
    }
}

pub(crate) fn builtin_gnutls_get_initstage(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("gnutls-get-initstage", &args, 1)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(&eval.processes, &args[0])?;
    let proc = eval
        .processes
        .get(id)
        .ok_or_else(|| signal("error", vec![Value::string("Process not found")]))?;
    Ok(Value::fixnum(i64::from(proc.gnutls_initstage)))
}

pub(crate) fn builtin_gnutls_peer_status(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("gnutls-peer-status", &args, 1)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(&eval.processes, &args[0])?;
    let proc = eval
        .processes
        .get(id)
        .ok_or_else(|| signal("error", vec![Value::string("Process not found")]))?;
    if proc.gnutls_initstage == GnutlsInitStage::Ready {
        Ok(proc
            .live_io
            .tls_stream
            .as_ref()
            .map(|tls| gnutls_peer_status_to_value(&tls.peer_status()))
            .unwrap_or(Value::NIL))
    } else {
        Ok(Value::NIL)
    }
}

/// (neomacs-open-tls-stream NAME BUFFER HOST PORT) -> process
///
/// Open a TCP network process and immediately upgrade it through Neomacs'
/// native TLS backend. This is intentionally separate from GNU's `gnutls-*`
/// API: rustls provides TLS transport, not libgnutls semantics.
pub(crate) fn builtin_neomacs_open_tls_stream(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("neomacs-open-tls-stream", &args, 4)?;
    let host = expect_string_strict(&args[2])?;
    let process = builtin_make_network_process(
        eval,
        vec![
            Value::keyword(":name"),
            args[0],
            Value::keyword(":buffer"),
            args[1],
            Value::keyword(":host"),
            args[2],
            Value::keyword(":service"),
            args[3],
        ],
    )?;
    let id = resolve_process_or_wrong_type_any_in_manager(&eval.processes, &process)?;
    let parameters = TlsClientParameters::default_roots(host);
    upgrade_process_to_tls::<RustlsBackend>(
        &mut eval.processes,
        id,
        &parameters,
        "neomacs-open-tls-stream",
        signal_neomacs_tls_error,
    )?;
    Ok(process)
}

fn upgrade_process_to_tls<B: TlsClientBackend>(
    processes: &mut ProcessManager,
    id: ProcessId,
    parameters: &TlsClientParameters,
    operation: &str,
    map_error: fn(TlsBackendError) -> Flow,
) -> Result<(), Flow> {
    let proc = processes
        .get_mut(id)
        .ok_or_else(|| signal("error", vec![Value::string("Process not found")]))?;

    if proc.kind != ProcessKind::Network {
        return Err(signal(
            "error",
            vec![Value::string(format!("{operation}: not a network process"))],
        ));
    }

    // Take the plain TCP stream; it will be owned by the TLS stream.
    let tcp_stream = match proc.live_io.network_socket.take() {
        Some(NetworkSocket::TcpStream(stream)) => stream,
        Some(other) => {
            proc.live_io.network_socket = Some(other);
            return Err(signal(
                "error",
                vec![Value::string(format!(
                    "{operation}: process is not a TCP stream"
                ))],
            ));
        }
        None => {
            return Err(signal(
                "error",
                vec![Value::string(format!(
                    "{operation}: no socket (already TLS or closed)"
                ))],
            ));
        }
    };

    proc.gnutls_initstage = GnutlsInitStage::HandshakeTried;
    let tls_stream = B::connect_client(tcp_stream, parameters).map_err(map_error)?;

    // Store the TLS stream. The poller still watches the underlying fd
    // (which is the same fd that was registered for the plain socket).
    proc.live_io.tls_stream = Some(tls_stream);
    proc.gnutls_initstage = GnutlsInitStage::Ready;
    proc.gnutls_boot_parameters = Value::NIL;

    Ok(())
}

fn signal_gnutls_boot_error(err: TlsBackendError) -> Flow {
    match err {
        TlsBackendError::InvalidHostname(_)
        | TlsBackendError::TrustFile { .. }
        | TlsBackendError::Connect(_) => signal("error", vec![Value::string(err.to_string())]),
        TlsBackendError::UnexpectedEof => signal(
            "gnutls-error",
            vec![
                Value::fixnum(-1),
                Value::string("TLS handshake: unexpected EOF"),
            ],
        ),
        TlsBackendError::Io(err) => signal(
            "gnutls-error",
            vec![
                Value::fixnum(-1),
                Value::string(format!("TLS handshake: {}", err)),
            ],
        ),
    }
}

fn signal_neomacs_tls_error(err: TlsBackendError) -> Flow {
    signal("error", vec![Value::string(err.to_string())])
}

/// (isearch-process-search-string STRING MESSAGE) -> nil
/// (minibuffer--sort-preprocess-history HISTORY) -> nil
/// (print--preprocess OBJECT) -> nil
///
/// Extracts sharing info from OBJECT needed to print it: fills the
/// `print-number-table` hash when `print-circle' is non-nil, and does nothing
/// otherwise.  Mirrors GNU `Fprint_preprocess` (src/print.c).
pub(crate) fn builtin_print_preprocess(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("print--preprocess", &args, 1)?;
    let object = args[0];

    // GNU: does nothing if `print-circle' is nil.
    let print_circle = eval
        .obarray
        .symbol_value("print-circle")
        .is_some_and(|v| v.is_truthy());
    if !print_circle {
        return Ok(Value::NIL);
    }

    let print_gensym = eval
        .obarray
        .symbol_value("print-gensym")
        .is_some_and(|v| v.is_truthy());
    let print_continuous_numbering = eval
        .obarray
        .symbol_value("print-continuous-numbering")
        .is_some_and(|v| v.is_truthy());

    // GNU: `if (!HASH_TABLE_P (Vprint_number_table)) Vprint_number_table = make-hash-table :test eq`.
    let table_value = match eval.obarray.symbol_value("print-number-table") {
        Some(v) if v.is_hash_table() => *v,
        _ => {
            let table = Value::hash_table(super::value::HashTableTest::Eq);
            eval.set_variable("print-number-table", table);
            table
        }
    };

    // Root the object and table across the (allocation-heavy) traversal.
    let roots = eval.save_specpdl_roots();
    eval.push_specpdl_root(object);
    eval.push_specpdl_root(table_value);
    super::print::preprocess_print_number_table(
        &object,
        table_value,
        print_gensym,
        print_continuous_numbering,
    );
    eval.restore_specpdl_roots(roots);

    Ok(Value::NIL)
}

/// (syntax-propertize--in-process-p) -> nil
/// (window--adjust-process-windows) -> nil
/// (window--process-window-list) -> nil
/// (window-adjust-process-window-size PROCESS WINDOW) -> nil
/// (window-adjust-process-window-size-largest PROCESS WINDOW) -> nil
/// (window-adjust-process-window-size-smallest PROCESS WINDOW) -> nil
/// (format-network-address ADDRESS &optional OMIT-PORT) -> string-or-nil
pub(crate) fn builtin_format_network_address(
    _eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_format_network_address_impl(args)
}

pub(crate) fn builtin_format_network_address_impl(args: Vec<Value>) -> EvalResult {
    expect_min_args("format-network-address", &args, 1)?;
    if args.len() > 2 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("format-network-address"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    let omit_port = args.get(1).is_some_and(|v| v.is_truthy());
    match args[0].kind() {
        ValueKind::String => Ok(args[0]),
        ValueKind::Nil => Ok(Value::NIL),
        ValueKind::Veclike(VecLikeType::Vector) => {
            let Some(items) = vector_nonnegative_integers(&args[0]) else {
                return Ok(Value::NIL);
            };
            if let Some(ipv4) = format_ipv4_network_address(&items, omit_port) {
                return Ok(Value::string(ipv4));
            }
            if let Some(ipv6) = format_ipv6_network_address(&items, omit_port) {
                return Ok(Value::string(ipv6));
            }
            Ok(Value::NIL)
        }
        ValueKind::Cons => {
            if let ValueKind::Fixnum(family) = args[0].cons_car().kind() {
                return Ok(Value::string(format!("<Family {family}>")));
            }
            Err(signal(
                "error",
                vec![Value::string(
                    "Format specifier doesn't match argument type",
                )],
            ))
        }
        _ => Ok(Value::NIL),
    }
}

/// (network-interface-list &optional FULL FAMILY) -> interface-list
pub(crate) fn builtin_network_interface_list(
    _eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_network_interface_list_impl(args)
}

pub(crate) fn builtin_network_interface_list_impl(args: Vec<Value>) -> EvalResult {
    if args.len() > 2 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("network-interface-list"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    let full = args.first().is_some_and(|v| v.is_truthy());
    let family = args.get(1).cloned().unwrap_or(Value::NIL);
    let requested_family = if family.is_nil() {
        None
    } else {
        Some(
            NetworkAddressFamily::from_symbol_value(&family).ok_or_else(|| {
                signal("error", vec![Value::string("Unsupported address family")])
            })?,
        )
    };
    let include_ipv4 = requested_family.is_none_or(|family| family == NetworkAddressFamily::Ipv4);
    let include_ipv6 = requested_family.is_none_or(|family| family == NetworkAddressFamily::Ipv6);

    let mut entries = Vec::new();
    if let Some(host_entries) = sys::interface_snapshot() {
        for entry in host_entries.into_iter().rev() {
            let include = match entry.family {
                NetworkAddressFamily::Ipv4 => include_ipv4,
                NetworkAddressFamily::Ipv6 => include_ipv6,
            };
            if !include {
                continue;
            }

            if full {
                entries.push(Value::list(vec![
                    Value::string(entry.name),
                    entry.address,
                    entry.list_broadcast,
                    entry.netmask,
                ]));
            } else {
                entries.push(Value::cons(Value::string(entry.name), entry.address));
            }
        }
    }

    if entries.is_empty() {
        if include_ipv6 {
            entries.push(interface_entry("lo", loopback_ipv6_address(), full));
        }
        if include_ipv4 {
            entries.push(interface_entry("lo", loopback_ipv4_address(), full));
        }
    }
    Ok(Value::list(entries))
}

/// (network-interface-info IFNAME) -> interface-info-or-nil
pub(crate) fn builtin_network_interface_info(
    _eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_network_interface_info_impl(args)
}

pub(crate) fn builtin_network_interface_info_impl(args: Vec<Value>) -> EvalResult {
    expect_args("network-interface-info", &args, 1)?;
    let ifname_raw = expect_string_strict(&args[0])?;
    // Match C-string interface-name handling: embedded NUL truncates lookup.
    let ifname = ifname_raw.split('\0').next().unwrap_or_default();
    // Emacs applies IFNAMSIZ-style byte limits, not character counts.
    if ifname.len() >= 16 {
        return Err(signal(
            "error",
            vec![Value::string("interface name too long")],
        ));
    }

    if let Some(host_entries) = sys::interface_snapshot() {
        let mut first_match: Option<HostInterfaceEntry> = None;
        let mut ipv4_match: Option<HostInterfaceEntry> = None;

        for entry in host_entries {
            if entry.name != ifname {
                continue;
            }
            if first_match.is_none() {
                first_match = Some(entry.clone());
            }
            if entry.family == NetworkAddressFamily::Ipv4 {
                ipv4_match = Some(entry);
                break;
            }
        }

        if let Some(entry) = ipv4_match.or(first_match) {
            return Ok(Value::list(vec![
                entry.address,
                entry.info_broadcast,
                entry.netmask,
                entry.hwaddr.unwrap_or(Value::NIL),
                entry.flags,
            ]));
        }
    }

    if ifname == "lo" {
        return Ok(Value::list(vec![
            loopback_ipv4_address(),
            loopback_ipv4_broadcast(),
            loopback_ipv4_netmask(),
            loopback_hwaddr(),
            loopback_flags(),
        ]));
    }

    Ok(Value::NIL)
}

/// (network-lookup-address-info NAME &optional FAMILY HINTS) -> address-list
pub(crate) fn builtin_network_lookup_address_info(
    _eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_network_lookup_address_info_impl(args)
}

pub(crate) fn builtin_network_lookup_address_info_impl(args: Vec<Value>) -> EvalResult {
    expect_min_args("network-lookup-address-info", &args, 1)?;
    if args.len() > 3 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("network-lookup-address-info"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let name = expect_network_lookup_hostname(&args[0])?;

    let family = args.get(1).cloned().unwrap_or(Value::NIL);
    let hint_value = args.get(2).cloned().unwrap_or(Value::NIL);

    let lookup_family = if family.is_nil() {
        None
    } else {
        Some(
            NetworkAddressFamily::from_symbol_value(&family)
                .ok_or_else(|| signal("error", vec![Value::string("Unsupported family")]))?,
        )
    };
    let lookup_hint = if hint_value.is_nil() {
        None
    } else {
        Some(
            NetworkLookupHint::from_symbol_value(&hint_value)
                .ok_or_else(|| signal("error", vec![Value::string("Unsupported hints value")]))?,
        )
    };
    let entries = resolve_network_lookup_addresses(&name, lookup_family, lookup_hint);
    Ok(Value::list(entries))
}

/// (signal-names) -> list-of-signal-name-strings
pub(crate) fn builtin_signal_names(
    _eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_signal_names_impl(args)
}

pub(crate) fn builtin_signal_names_impl(args: Vec<Value>) -> EvalResult {
    expect_args("signal-names", &args, 0)?;
    let names = vec![
        "RTMAX", "RTMAX-1", "RTMAX-2", "RTMAX-3", "RTMAX-4", "RTMAX-5", "RTMAX-6", "RTMAX-7",
        "RTMAX-8", "RTMAX-9", "RTMAX-10", "RTMAX-11", "RTMAX-12", "RTMAX-13", "RTMAX-14",
        "RTMIN+15", "RTMIN+14", "RTMIN+13", "RTMIN+12", "RTMIN+11", "RTMIN+10", "RTMIN+9",
        "RTMIN+8", "RTMIN+7", "RTMIN+6", "RTMIN+5", "RTMIN+4", "RTMIN+3", "RTMIN+2", "RTMIN+1",
        "RTMIN", "SYS", "PWR", "POLL", "WINCH", "PROF", "VTALRM", "XFSZ", "XCPU", "URG", "TTOU",
        "TTIN", "TSTP", "STOP", "CONT", "CHLD", "STKFLT", "TERM", "ALRM", "PIPE", "USR2", "SEGV",
        "USR1", "KILL", "FPE", "BUS", "ABRT", "TRAP", "ILL", "QUIT", "INT", "HUP", "EXIT",
    ];
    Ok(Value::list(
        names.into_iter().map(Value::string).collect::<Vec<_>>(),
    ))
}

/// (list-system-processes) -> process-id-list
pub(crate) fn builtin_list_system_processes(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("list-system-processes", &args, 0)?;
    if let Some(default_directory) = visible_default_directory_lisp(eval) {
        let operation = Value::symbol("list-system-processes");
        let handler = super::fileio::find_file_name_handler_lisp_for_eval(
            eval,
            &default_directory,
            operation,
        );
        if !handler.is_nil() {
            return eval.funcall_general(handler, vec![operation]);
        }
    }
    builtin_list_system_processes_impl(args)
}

pub(crate) fn builtin_list_system_processes_impl(args: Vec<Value>) -> EvalResult {
    expect_args("list-system-processes", &args, 0)?;

    let mut pids = sys::list_process_ids();
    pids.sort_unstable();
    Ok(Value::list(pids.into_iter().map(Value::fixnum).collect()))
}

/// (num-processors &optional QUERY) -> integer
pub(crate) fn builtin_num_processors(
    _eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_num_processors_impl(args)
}

pub(crate) fn builtin_num_processors_impl(args: Vec<Value>) -> EvalResult {
    if args.len() > 1 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("num-processors"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let query = args.first().and_then(NumProcessorsQuery::from_symbol_value);
    Ok(Value::fixnum(num_processors_count(query) as i64))
}

fn num_processors_count(query: Option<NumProcessorsQuery>) -> u64 {
    match query {
        Some(NumProcessorsQuery::All) => all_processors_count(),
        Some(NumProcessorsQuery::Current) => current_processors_count(),
        None => current_processors_count_overridable(),
    }
}

#[cfg(unix)]
fn current_processors_count_overridable() -> u64 {
    let omp_threads = std::env::var_os("OMP_NUM_THREADS");
    let omp_limit = std::env::var_os("OMP_THREAD_LIMIT");
    current_processors_count_overridable_with_env(
        omp_threads.as_deref().map(OsStrExt::as_bytes),
        omp_limit.as_deref().map(OsStrExt::as_bytes),
        current_processors_count(),
    )
}

#[cfg(not(unix))]
fn current_processors_count_overridable() -> u64 {
    let omp_threads = std::env::var("OMP_NUM_THREADS").ok();
    let omp_limit = std::env::var("OMP_THREAD_LIMIT").ok();
    current_processors_count_overridable_with_env(
        omp_threads.as_deref().map(str::as_bytes),
        omp_limit.as_deref().map(str::as_bytes),
        current_processors_count(),
    )
}

fn current_processors_count_overridable_with_env(
    omp_threads: Option<&[u8]>,
    omp_limit: Option<&[u8]>,
    current_count: u64,
) -> u64 {
    let omp_threads = omp_threads.and_then(parse_openmp_threads).unwrap_or(0);
    let mut omp_limit = omp_limit.and_then(parse_openmp_threads).unwrap_or(u64::MAX);
    if omp_limit == 0 {
        omp_limit = u64::MAX;
    }

    if omp_threads != 0 {
        return omp_threads.min(omp_limit);
    }

    current_count.min(omp_limit).max(1)
}

fn parse_openmp_threads(bytes: &[u8]) -> Option<u64> {
    let mut idx = 0;
    while idx < bytes.len() && bytes[idx].is_ascii_whitespace() {
        idx += 1;
    }
    if idx == bytes.len() || !bytes[idx].is_ascii_digit() {
        return None;
    }

    let start = idx;
    while idx < bytes.len() && bytes[idx].is_ascii_digit() {
        idx += 1;
    }
    let value = std::str::from_utf8(&bytes[start..idx])
        .ok()?
        .parse::<u64>()
        .ok()?;

    while idx < bytes.len() && bytes[idx].is_ascii_whitespace() {
        idx += 1;
    }

    if idx == bytes.len() || bytes[idx] == b',' {
        Some(value)
    } else {
        None
    }
}

fn current_processors_count() -> u64 {
    std::thread::available_parallelism()
        .map(|n| n.get() as u64)
        .unwrap_or(1)
        .max(1)
}

fn all_processors_count() -> u64 {
    let mut system = sysinfo::System::new();
    system.refresh_cpu_list(sysinfo::CpuRefreshKind::nothing());
    let count = system.cpus().len() as u64;
    if count == 0 {
        current_processors_count()
    } else {
        count
    }
}

/// (make-network-process &rest ARGS) -> process-or-nil
/// `make-network-process` with an explicit `:local` / `:remote` address
/// spec: binds or connects exactly the given inet or unix-domain address,
/// bypassing family/host/service resolution. Both arms return, so the
/// general resolution path below never runs for explicit addresses.
/// Extracted verbatim from builtin_make_network_process.
#[allow(clippy::too_many_arguments)]
fn connect_network_process_at_explicit_address(
    eval: &mut super::eval::Context,
    explicit_address: Value,
    remote_address_value: Value,
    name: LispString,
    mut contact: Value,
    filter_val: Value,
    sentinel_val: Value,
    log_val: Value,
    resolved_coding: ProcessCodingSystems,
    buffer: Value,
    plist_val: Value,
    nowait: bool,
    server: bool,
    noquery: bool,
    stop: bool,
    server_backlog: Option<i32>,
    socket_type: NetworkSocketType,
    socket_options: Vec<NetworkSocketOptionSpec>,
    tls_parameters: Option<super::tls::GnutlsBootParameters>,
    tls_parameters_value: Value,
) -> EvalResult {
    let address = parse_network_address_spec(&explicit_address)?;
    match address {
        NetworkAddressSpec::Inet(addr) => {
            if socket_type == NetworkSocketType::Datagram {
                if server {
                    let effective_options = tcp_server_socket_options(&socket_options);
                    let socket = bind_udp_socket(addr, &effective_options)?;
                    let local_addr = socket.local_addr().map_err(|e| {
                        signal(
                            LispCondition::FileError,
                            vec![Value::string(format!("getsockname: {}", e))],
                        )
                    })?;
                    let zero_datagram = datagram_zero_address_for(local_addr);
                    let (datagram_socket_addr, datagram_address) = if !remote_address_value.is_nil()
                    {
                        match parse_network_address_spec(&remote_address_value)? {
                            NetworkAddressSpec::Inet(remote) => {
                                (Some(remote), socket_addr_to_lisp_value(remote))
                            }
                            #[cfg(unix)]
                            NetworkAddressSpec::Local(_) => (None, zero_datagram),
                        }
                    } else {
                        (None, zero_datagram)
                    };
                    contact = process_contact_plist_put(
                        contact,
                        ProcessKeyword::Service.value(),
                        Value::fixnum(local_addr.port() as i64),
                    )?;
                    contact = process_contact_plist_put(
                        contact,
                        ProcessKeyword::Local.value(),
                        socket_addr_to_lisp_value(local_addr),
                    )?;
                    contact = process_contact_plist_put(
                        contact,
                        ProcessKeyword::Remote.value(),
                        datagram_address,
                    )?;

                    let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
                    eval.processes.sync_process_mark(&mut eval.buffers, id)?;
                    if let Some(proc) = eval.processes.get_mut(id) {
                        proc.childp = contact;
                        proc.thread = current_thread_handle(&eval.threads);
                        proc.plist = plist_val;
                        proc.status = ProcessStatusSymbol::Open.value();
                        proc.live_io.network_socket = Some(NetworkSocket::UdpSocket(socket));
                        proc.datagram_socket_addr = datagram_socket_addr;
                        proc.datagram_address = datagram_address;
                        if !filter_val.is_nil() {
                            proc.filter = filter_val;
                            proc.childp = process_contact_plist_put(
                                proc.childp,
                                ProcessKeyword::Filter.value(),
                                proc.filter,
                            )?;
                        }
                        if !sentinel_val.is_nil() {
                            proc.sentinel = sentinel_val;
                            proc.childp = process_contact_plist_put(
                                proc.childp,
                                ProcessKeyword::Sentinel.value(),
                                proc.sentinel,
                            )?;
                        }
                        if !log_val.is_nil() {
                            proc.log = log_val;
                            proc.childp = process_contact_plist_put(
                                proc.childp,
                                ProcessKeyword::Log.value(),
                                proc.log,
                            )?;
                        }
                        if !buffer.is_nil() {
                            proc.childp = process_contact_plist_put(
                                proc.childp,
                                ProcessKeyword::Buffer.value(),
                                buffer,
                            )?;
                        }
                        apply_connection_process_flags(proc, noquery, stop);
                    }
                    eval.processes.register_socket_fd(id).ok();
                    return Ok(Value::make_process(id));
                }

                let socket = bind_udp_client_socket(addr, &socket_options)?;
                contact = process_contact_plist_put(
                    contact,
                    ProcessKeyword::Remote.value(),
                    socket_addr_to_lisp_value(addr),
                )?;
                contact = process_contact_plist_put(
                    contact,
                    ProcessKeyword::Local.value(),
                    socket_addr_to_lisp_value(udp_unspecified_addr_for(addr)),
                )?;

                let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
                eval.processes.sync_process_mark(&mut eval.buffers, id)?;
                if let Some(proc) = eval.processes.get_mut(id) {
                    proc.live_io.network_socket = Some(NetworkSocket::UdpSocket(socket));
                    proc.datagram_socket_addr = Some(addr);
                    proc.datagram_address = socket_addr_to_lisp_value(addr);
                    proc.status = ProcessStatusSymbol::Open.value();
                    proc.childp = contact;
                    proc.plist = plist_val;
                    proc.thread = current_thread_handle(&eval.threads);
                    if !filter_val.is_nil() {
                        proc.filter = filter_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Filter.value(),
                            proc.filter,
                        )?;
                    }
                    if !sentinel_val.is_nil() {
                        proc.sentinel = sentinel_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Sentinel.value(),
                            proc.sentinel,
                        )?;
                    }
                    if !buffer.is_nil() {
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Buffer.value(),
                            buffer,
                        )?;
                    }
                    apply_connection_process_flags(proc, noquery, stop);
                }
                eval.processes.register_socket_fd(id).ok();
                return Ok(Value::make_process(id));
            }

            #[cfg(unix)]
            if socket_type == NetworkSocketType::Seqpacket {
                return Err(signal(
                    "error",
                    vec![Value::string("Unsupported connection type")],
                ));
            }

            if server {
                let effective_options = tcp_server_socket_options(&socket_options);
                let listener = bind_tcp_listener_socket(
                    addr,
                    server_backlog.unwrap_or(5),
                    &effective_options,
                )?;
                let local_addr = listener.local_addr().map_err(|e| {
                    signal(
                        LispCondition::FileError,
                        vec![Value::string(format!("getsockname: {}", e))],
                    )
                })?;
                contact = process_contact_plist_put(
                    contact,
                    ProcessKeyword::Service.value(),
                    Value::fixnum(local_addr.port() as i64),
                )?;
                contact = process_contact_plist_put(
                    contact,
                    ProcessKeyword::Local.value(),
                    socket_addr_to_lisp_value(local_addr),
                )?;

                let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
                eval.processes.sync_process_mark(&mut eval.buffers, id)?;
                if let Some(proc) = eval.processes.get_mut(id) {
                    proc.childp = contact;
                    proc.thread = current_thread_handle(&eval.threads);
                    proc.plist = plist_val;
                    proc.live_io.network_socket = Some(NetworkSocket::TcpListener(listener));
                    if !filter_val.is_nil() {
                        proc.filter = filter_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Filter.value(),
                            proc.filter,
                        )?;
                    }
                    if !sentinel_val.is_nil() {
                        proc.sentinel = sentinel_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Sentinel.value(),
                            proc.sentinel,
                        )?;
                    }
                    if !log_val.is_nil() {
                        proc.log = log_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Log.value(),
                            proc.log,
                        )?;
                    }
                    if !buffer.is_nil() {
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Buffer.value(),
                            buffer,
                        )?;
                    }
                    apply_connection_process_flags(proc, noquery, stop);
                }
                eval.processes.register_socket_fd(id).ok();
                return Ok(Value::make_process(id));
            }

            if nowait {
                let start = start_pending_tcp_stream_connect(vec![addr], &socket_options)?;
                let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
                eval.processes.sync_process_mark(&mut eval.buffers, id)?;
                if let Some(proc) = eval.processes.get_mut(id) {
                    proc.status = process_status_connect_value();
                    proc.childp = contact;
                    proc.plist = plist_val;
                    proc.thread = current_thread_handle(&eval.threads);
                    if !filter_val.is_nil() {
                        proc.filter = filter_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Filter.value(),
                            proc.filter,
                        )?;
                    }
                    if !sentinel_val.is_nil() {
                        proc.sentinel = sentinel_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Sentinel.value(),
                            proc.sentinel,
                        )?;
                    }
                    if !buffer.is_nil() {
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Buffer.value(),
                            buffer,
                        )?;
                    }
                    match start {
                        PendingNetworkConnectStart::Started(started) => {
                            let local_addr = started.stream.local_addr().ok();
                            proc.live_io.network_socket =
                                Some(NetworkSocket::TcpStream(started.stream));
                            proc.live_io.pending_network_connect =
                                Some(PendingNetworkConnect::Tcp {
                                    remaining_addrs: started.remaining_addrs,
                                    socket_options: socket_options.clone(),
                                });
                            ProcessManager::update_tcp_client_contact(
                                proc,
                                started.remote_addr,
                                local_addr,
                            )?;
                        }
                        PendingNetworkConnectStart::Failed(code) => {
                            proc.status = process_status_failed_value(code);
                        }
                    }
                    if proc.live_io.pending_network_connect.is_some() {
                        proc.gnutls_boot_parameters = tls_parameters_value;
                    }
                    apply_connection_process_flags(proc, noquery, stop);
                }
                if eval
                    .processes
                    .get(id)
                    .is_some_and(|proc| proc.live_io.pending_network_connect.is_some())
                {
                    eval.processes.register_socket_writable_fd(id).ok();
                }
                return Ok(Value::make_process(id));
            }

            let stream = connect_tcp_stream_socket(addr, &socket_options, contact)?;
            let remote_addr = stream.peer_addr().ok();
            let local_addr = stream.local_addr().ok();

            let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
            eval.processes.sync_process_mark(&mut eval.buffers, id)?;
            if let Some(addr) = remote_addr {
                contact = process_contact_plist_put(
                    contact,
                    ProcessKeyword::Remote.value(),
                    socket_addr_to_lisp_value(addr),
                )?;
            }
            if let Some(addr) = local_addr {
                contact = process_contact_plist_put(
                    contact,
                    ProcessKeyword::Local.value(),
                    socket_addr_to_lisp_value(addr),
                )?;
            }
            if let Some(proc) = eval.processes.get_mut(id) {
                proc.live_io.network_socket = Some(NetworkSocket::TcpStream(stream));
                proc.status = process_status_run_value();
                proc.childp = contact;
                proc.plist = plist_val;
                proc.thread = current_thread_handle(&eval.threads);
                if !filter_val.is_nil() {
                    proc.filter = filter_val;
                    proc.childp = process_contact_plist_put(
                        proc.childp,
                        ProcessKeyword::Filter.value(),
                        proc.filter,
                    )?;
                }
                if !sentinel_val.is_nil() {
                    proc.sentinel = sentinel_val;
                    proc.childp = process_contact_plist_put(
                        proc.childp,
                        ProcessKeyword::Sentinel.value(),
                        proc.sentinel,
                    )?;
                }
                if !buffer.is_nil() {
                    proc.childp = process_contact_plist_put(
                        proc.childp,
                        ProcessKeyword::Buffer.value(),
                        buffer,
                    )?;
                }
                apply_connection_process_flags(proc, noquery, stop);
            }
            if let Some(parameters) = tls_parameters.clone() {
                upgrade_process_to_tls::<RustlsBackend>(
                    &mut eval.processes,
                    id,
                    &parameters.client,
                    "make-network-process",
                    signal_gnutls_boot_error,
                )?;
            }
            eval.processes.register_socket_fd(id).ok();
            // GNU fires NO sentinel for a synchronous (non-:nowait)
            // connect: `connect_network_socket` (process.c) sets the
            // status without `exec_sentinel`; only the deferred `:nowait`
            // completion path in `wait_reading_process_output` delivers
            // "open\n" / "failed with code N\n".
            Ok(Value::make_process(id))
        }
        #[cfg(unix)]
        NetworkAddressSpec::Local(path) => {
            if socket_type == NetworkSocketType::Datagram {
                let path_value =
                    Value::heap_string(crate::emacs_core::fileio::path_to_lisp_file_name(&path));
                if server {
                    let socket = bind_unix_datagram_socket(&path, &socket_options)?;
                    let zero_datagram = datagram_zero_unix_address();
                    let (datagram_unix_path, datagram_address) = if !remote_address_value.is_nil() {
                        match parse_network_address_spec(&remote_address_value)? {
                            NetworkAddressSpec::Local(remote_path) => {
                                let remote_value = Value::heap_string(
                                    crate::emacs_core::fileio::path_to_lisp_file_name(&remote_path),
                                );
                                (Some(remote_path), remote_value)
                            }
                            NetworkAddressSpec::Inet(_) => (None, zero_datagram),
                        }
                    } else {
                        (None, zero_datagram)
                    };
                    contact = process_contact_plist_put(
                        contact,
                        ProcessKeyword::Local.value(),
                        path_value,
                    )?;
                    contact = process_contact_plist_put(
                        contact,
                        ProcessKeyword::Remote.value(),
                        datagram_address,
                    )?;

                    let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
                    eval.processes.sync_process_mark(&mut eval.buffers, id)?;
                    if let Some(proc) = eval.processes.get_mut(id) {
                        proc.childp = contact;
                        proc.thread = current_thread_handle(&eval.threads);
                        proc.plist = plist_val;
                        proc.status = ProcessStatusSymbol::Open.value();
                        proc.live_io.network_socket = Some(NetworkSocket::UnixDatagram(socket));
                        proc.datagram_address = datagram_address;
                        proc.datagram_unix_path = datagram_unix_path;
                        if !filter_val.is_nil() {
                            proc.filter = filter_val;
                            proc.childp = process_contact_plist_put(
                                proc.childp,
                                ProcessKeyword::Filter.value(),
                                proc.filter,
                            )?;
                        }
                        if !sentinel_val.is_nil() {
                            proc.sentinel = sentinel_val;
                            proc.childp = process_contact_plist_put(
                                proc.childp,
                                ProcessKeyword::Sentinel.value(),
                                proc.sentinel,
                            )?;
                        }
                        if !log_val.is_nil() {
                            proc.log = log_val;
                            proc.childp = process_contact_plist_put(
                                proc.childp,
                                ProcessKeyword::Log.value(),
                                proc.log,
                            )?;
                        }
                        if !buffer.is_nil() {
                            proc.childp = process_contact_plist_put(
                                proc.childp,
                                ProcessKeyword::Buffer.value(),
                                buffer,
                            )?;
                        }
                        apply_connection_process_flags(proc, noquery, stop);
                    }
                    eval.processes.register_socket_fd(id).ok();
                    return Ok(Value::make_process(id));
                }

                let socket = unbound_unix_datagram_socket(&socket_options)?;
                contact =
                    process_contact_plist_put(contact, ProcessKeyword::Remote.value(), path_value)?;
                contact = process_contact_plist_put(
                    contact,
                    ProcessKeyword::Local.value(),
                    Value::string(""),
                )?;

                let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
                eval.processes.sync_process_mark(&mut eval.buffers, id)?;
                if let Some(proc) = eval.processes.get_mut(id) {
                    proc.live_io.network_socket = Some(NetworkSocket::UnixDatagram(socket));
                    proc.status = ProcessStatusSymbol::Open.value();
                    proc.childp = contact;
                    proc.plist = plist_val;
                    proc.thread = current_thread_handle(&eval.threads);
                    proc.datagram_address = path_value;
                    proc.datagram_unix_path = Some(path);
                    if !filter_val.is_nil() {
                        proc.filter = filter_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Filter.value(),
                            proc.filter,
                        )?;
                    }
                    if !sentinel_val.is_nil() {
                        proc.sentinel = sentinel_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Sentinel.value(),
                            proc.sentinel,
                        )?;
                    }
                    if !buffer.is_nil() {
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Buffer.value(),
                            buffer,
                        )?;
                    }
                    apply_connection_process_flags(proc, noquery, stop);
                }
                eval.processes.register_socket_fd(id).ok();
                return Ok(Value::make_process(id));
            }

            if socket_type == NetworkSocketType::Seqpacket {
                if server {
                    let listener = bind_unix_seqpacket_listener_socket(
                        &path,
                        server_backlog.unwrap_or(5),
                        &socket_options,
                    )?;
                    contact = process_contact_plist_put(
                        contact,
                        ProcessKeyword::Local.value(),
                        Value::heap_string(crate::emacs_core::fileio::path_to_lisp_file_name(
                            &path,
                        )),
                    )?;

                    let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
                    eval.processes.sync_process_mark(&mut eval.buffers, id)?;
                    if let Some(proc) = eval.processes.get_mut(id) {
                        proc.childp = contact;
                        proc.thread = current_thread_handle(&eval.threads);
                        proc.plist = plist_val;
                        proc.live_io.network_socket =
                            Some(NetworkSocket::SeqpacketListener(listener));
                        if !filter_val.is_nil() {
                            proc.filter = filter_val;
                            proc.childp = process_contact_plist_put(
                                proc.childp,
                                ProcessKeyword::Filter.value(),
                                proc.filter,
                            )?;
                        }
                        if !sentinel_val.is_nil() {
                            proc.sentinel = sentinel_val;
                            proc.childp = process_contact_plist_put(
                                proc.childp,
                                ProcessKeyword::Sentinel.value(),
                                proc.sentinel,
                            )?;
                        }
                        if !log_val.is_nil() {
                            proc.log = log_val;
                            proc.childp = process_contact_plist_put(
                                proc.childp,
                                ProcessKeyword::Log.value(),
                                proc.log,
                            )?;
                        }
                        if !buffer.is_nil() {
                            proc.childp = process_contact_plist_put(
                                proc.childp,
                                ProcessKeyword::Buffer.value(),
                                buffer,
                            )?;
                        }
                        apply_connection_process_flags(proc, noquery, stop);
                    }
                    eval.processes.register_socket_fd(id).ok();
                    return Ok(Value::make_process(id));
                }

                let socket = connect_unix_seqpacket_socket(&path, &socket_options)?;
                contact = process_contact_plist_put(
                    contact,
                    ProcessKeyword::Remote.value(),
                    Value::heap_string(crate::emacs_core::fileio::path_to_lisp_file_name(&path)),
                )?;
                contact = process_contact_plist_put(
                    contact,
                    ProcessKeyword::Local.value(),
                    Value::string(""),
                )?;

                let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
                eval.processes.sync_process_mark(&mut eval.buffers, id)?;
                if let Some(proc) = eval.processes.get_mut(id) {
                    proc.live_io.network_socket = Some(NetworkSocket::SeqpacketStream(socket));
                    proc.status = process_status_run_value();
                    proc.childp = contact;
                    proc.plist = plist_val;
                    proc.thread = current_thread_handle(&eval.threads);
                    if !filter_val.is_nil() {
                        proc.filter = filter_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Filter.value(),
                            proc.filter,
                        )?;
                    }
                    if !sentinel_val.is_nil() {
                        proc.sentinel = sentinel_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Sentinel.value(),
                            proc.sentinel,
                        )?;
                    }
                    if !buffer.is_nil() {
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Buffer.value(),
                            buffer,
                        )?;
                    }
                    apply_connection_process_flags(proc, noquery, stop);
                }

                eval.processes.register_socket_fd(id).ok();

                return Ok(Value::make_process(id));
            }

            if server {
                let listener =
                    bind_unix_listener_socket(&path, server_backlog.unwrap_or(5), &socket_options)?;
                contact = process_contact_plist_put(
                    contact,
                    ProcessKeyword::Local.value(),
                    Value::heap_string(crate::emacs_core::fileio::path_to_lisp_file_name(&path)),
                )?;

                let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
                eval.processes.sync_process_mark(&mut eval.buffers, id)?;
                if let Some(proc) = eval.processes.get_mut(id) {
                    proc.childp = contact;
                    proc.thread = current_thread_handle(&eval.threads);
                    proc.plist = plist_val;
                    proc.live_io.network_socket = Some(NetworkSocket::UnixListener(listener));
                    if !filter_val.is_nil() {
                        proc.filter = filter_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Filter.value(),
                            proc.filter,
                        )?;
                    }
                    if !sentinel_val.is_nil() {
                        proc.sentinel = sentinel_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Sentinel.value(),
                            proc.sentinel,
                        )?;
                    }
                    if !log_val.is_nil() {
                        proc.log = log_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Log.value(),
                            proc.log,
                        )?;
                    }
                    if !buffer.is_nil() {
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Buffer.value(),
                            buffer,
                        )?;
                    }
                    apply_connection_process_flags(proc, noquery, stop);
                }
                eval.processes.register_socket_fd(id).ok();
                return Ok(Value::make_process(id));
            }

            contact = process_contact_plist_put(
                contact,
                ProcessKeyword::Remote.value(),
                Value::heap_string(crate::emacs_core::fileio::path_to_lisp_file_name(&path)),
            )?;
            contact = process_contact_plist_put(
                contact,
                ProcessKeyword::Local.value(),
                Value::string(""),
            )?;

            if nowait {
                let start = start_nonblocking_unix_stream_socket(&path, &socket_options)?;
                let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
                eval.processes.sync_process_mark(&mut eval.buffers, id)?;
                if let Some(proc) = eval.processes.get_mut(id) {
                    proc.status = process_status_connect_value();
                    proc.childp = contact;
                    proc.plist = plist_val;
                    proc.thread = current_thread_handle(&eval.threads);
                    if !filter_val.is_nil() {
                        proc.filter = filter_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Filter.value(),
                            proc.filter,
                        )?;
                    }
                    if !sentinel_val.is_nil() {
                        proc.sentinel = sentinel_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Sentinel.value(),
                            proc.sentinel,
                        )?;
                    }
                    match start {
                        Ok(stream) => {
                            proc.live_io.network_socket = Some(NetworkSocket::UnixStream(stream));
                            proc.live_io.pending_network_connect =
                                Some(PendingNetworkConnect::Local);
                        }
                        Err(err) => {
                            proc.status = process_status_failed_value(io_error_status_code(&err));
                        }
                    }
                    apply_connection_process_flags(proc, noquery, stop);
                }
                if eval
                    .processes
                    .get(id)
                    .is_some_and(|proc| proc.live_io.pending_network_connect.is_some())
                {
                    eval.processes.register_socket_writable_fd(id).ok();
                }
                return Ok(Value::make_process(id));
            }

            let stream = connect_unix_stream_socket(&path, &socket_options)?;
            let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
            eval.processes.sync_process_mark(&mut eval.buffers, id)?;
            if let Some(proc) = eval.processes.get_mut(id) {
                proc.live_io.network_socket = Some(NetworkSocket::UnixStream(stream));
                proc.status = process_status_run_value();
                proc.childp = contact;
                proc.plist = plist_val;
                proc.thread = current_thread_handle(&eval.threads);
                if !filter_val.is_nil() {
                    proc.filter = filter_val;
                    proc.childp = process_contact_plist_put(
                        proc.childp,
                        ProcessKeyword::Filter.value(),
                        proc.filter,
                    )?;
                }
                if !sentinel_val.is_nil() {
                    proc.sentinel = sentinel_val;
                    proc.childp = process_contact_plist_put(
                        proc.childp,
                        ProcessKeyword::Sentinel.value(),
                        proc.sentinel,
                    )?;
                }
                if !buffer.is_nil() {
                    proc.childp = process_contact_plist_put(
                        proc.childp,
                        ProcessKeyword::Buffer.value(),
                        buffer,
                    )?;
                }
                apply_connection_process_flags(proc, noquery, stop);
            }

            eval.processes.register_socket_fd(id).ok();

            Ok(Value::make_process(id))
        }
    }
}

/// `make-network-process` for `:type 'datagram` (UDP): binds a server
/// socket or creates a connected client socket. Every path returns, so
/// the stream-mode server/client paths below never see datagram type.
/// Extracted verbatim from builtin_make_network_process.
#[allow(clippy::too_many_arguments)]
fn connect_datagram_network_process(
    eval: &mut super::eval::Context,
    family: NetworkProcessFamily,
    host: Option<String>,
    service: Value,
    name: LispString,
    mut contact: Value,
    filter_val: Value,
    sentinel_val: Value,
    log_val: Value,
    resolved_coding: ProcessCodingSystems,
    buffer: Value,
    plist_val: Value,
    server: bool,
    noquery: bool,
    stop: bool,
    socket_type: NetworkSocketType,
    socket_options: Vec<NetworkSocketOptionSpec>,
) -> EvalResult {
    let port = parse_network_service_port(&service, server, socket_type)?;
    let host_str = host
        .clone()
        .unwrap_or_else(|| family.loopback_host().to_string());
    if server {
        let effective_options = tcp_server_socket_options(&socket_options);
        let socket = bind_udp_socket_host(host_str.as_str(), port, family, &effective_options)?;
        let local_addr = socket.local_addr().map_err(|e| {
            signal(
                LispCondition::FileError,
                vec![Value::string(format!("getsockname: {}", e))],
            )
        })?;
        let zero_datagram = datagram_zero_address_for(local_addr);
        contact = process_contact_plist_put(
            contact,
            ProcessKeyword::Service.value(),
            Value::fixnum(local_addr.port() as i64),
        )?;
        contact = process_contact_plist_put(
            contact,
            ProcessKeyword::Local.value(),
            socket_addr_to_lisp_value(local_addr),
        )?;
        contact =
            process_contact_plist_put(contact, ProcessKeyword::Remote.value(), zero_datagram)?;

        let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
        eval.processes.sync_process_mark(&mut eval.buffers, id)?;
        if let Some(proc) = eval.processes.get_mut(id) {
            proc.childp = contact;
            proc.thread = current_thread_handle(&eval.threads);
            proc.plist = plist_val;
            proc.status = ProcessStatusSymbol::Open.value();
            proc.live_io.network_socket = Some(NetworkSocket::UdpSocket(socket));
            proc.datagram_address = zero_datagram;
            proc.datagram_socket_addr = None;
            if !filter_val.is_nil() {
                proc.filter = filter_val;
                proc.childp = process_contact_plist_put(
                    proc.childp,
                    ProcessKeyword::Filter.value(),
                    proc.filter,
                )?;
            }
            if !sentinel_val.is_nil() {
                proc.sentinel = sentinel_val;
                proc.childp = process_contact_plist_put(
                    proc.childp,
                    ProcessKeyword::Sentinel.value(),
                    proc.sentinel,
                )?;
            }
            if !log_val.is_nil() {
                proc.log = log_val;
                proc.childp =
                    process_contact_plist_put(proc.childp, ProcessKeyword::Log.value(), proc.log)?;
            }
            if !buffer.is_nil() {
                proc.childp =
                    process_contact_plist_put(proc.childp, ProcessKeyword::Buffer.value(), buffer)?;
            }
            apply_connection_process_flags(proc, noquery, stop);
        }
        eval.processes.register_socket_fd(id).ok();
        return Ok(Value::make_process(id));
    }

    let (socket, remote_addr) =
        connect_udp_socket_host(host_str.as_str(), port, family, &socket_options)?;
    contact = process_contact_plist_put(
        contact,
        ProcessKeyword::Remote.value(),
        socket_addr_to_lisp_value(remote_addr),
    )?;
    contact = process_contact_plist_put(
        contact,
        ProcessKeyword::Local.value(),
        socket_addr_to_lisp_value(udp_unspecified_addr_for(remote_addr)),
    )?;

    let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
    eval.processes.sync_process_mark(&mut eval.buffers, id)?;
    if let Some(proc) = eval.processes.get_mut(id) {
        proc.live_io.network_socket = Some(NetworkSocket::UdpSocket(socket));
        proc.datagram_socket_addr = Some(remote_addr);
        proc.datagram_address = socket_addr_to_lisp_value(remote_addr);
        proc.status = ProcessStatusSymbol::Open.value();
        proc.childp = contact;
        proc.plist = plist_val;
        proc.thread = current_thread_handle(&eval.threads);
        if !filter_val.is_nil() {
            proc.filter = filter_val;
            proc.childp = process_contact_plist_put(
                proc.childp,
                ProcessKeyword::Filter.value(),
                proc.filter,
            )?;
        }
        if !sentinel_val.is_nil() {
            proc.sentinel = sentinel_val;
            proc.childp = process_contact_plist_put(
                proc.childp,
                ProcessKeyword::Sentinel.value(),
                proc.sentinel,
            )?;
        }
        if !buffer.is_nil() {
            proc.childp =
                process_contact_plist_put(proc.childp, ProcessKeyword::Buffer.value(), buffer)?;
        }
        apply_connection_process_flags(proc, noquery, stop);
    }
    eval.processes.register_socket_fd(id).ok();
    Ok(Value::make_process(id))
}

/// `make-network-process` server mode for stream sockets: bind, listen,
/// and register the accepting process. Always returns. Extracted
/// verbatim from builtin_make_network_process.
#[allow(clippy::too_many_arguments)]
fn listen_stream_network_process(
    eval: &mut super::eval::Context,
    family: NetworkProcessFamily,
    host: Option<String>,
    service: Value,
    name: LispString,
    mut contact: Value,
    filter_val: Value,
    sentinel_val: Value,
    log_val: Value,
    resolved_coding: ProcessCodingSystems,
    buffer: Value,
    plist_val: Value,
    noquery: bool,
    stop: bool,
    server_backlog: Option<i32>,
    socket_type: NetworkSocketType,
    socket_options: Vec<NetworkSocketOptionSpec>,
) -> EvalResult {
    let port = parse_network_service_port(&service, true, socket_type)?;
    let host_str = host
        .clone()
        .unwrap_or_else(|| family.loopback_host().to_string());
    let effective_options = tcp_server_socket_options(&socket_options);
    let listener = bind_tcp_listener_host(
        host_str.as_str(),
        port,
        family,
        server_backlog.unwrap_or(5),
        &effective_options,
    )?;
    let local_addr = listener.local_addr().map_err(|e| {
        signal(
            LispCondition::FileError,
            vec![Value::string(format!("getsockname: {}", e))],
        )
    })?;
    let local = socket_addr_to_lisp_value(local_addr);
    let actual_service = Value::fixnum(local_addr.port() as i64);
    contact = process_contact_plist_put(contact, ProcessKeyword::Service.value(), actual_service)?;
    contact = process_contact_plist_put(contact, ProcessKeyword::Local.value(), local)?;

    let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
    eval.processes.sync_process_mark(&mut eval.buffers, id)?;
    if let Some(proc) = eval.processes.get_mut(id) {
        proc.childp = contact;
        proc.thread = current_thread_handle(&eval.threads);
        proc.plist = plist_val;
        proc.live_io.network_socket = Some(NetworkSocket::TcpListener(listener));
        if !filter_val.is_nil() {
            proc.filter = filter_val;
            proc.childp = process_contact_plist_put(
                proc.childp,
                ProcessKeyword::Filter.value(),
                proc.filter,
            )?;
        }
        if !sentinel_val.is_nil() {
            proc.sentinel = sentinel_val;
            proc.childp = process_contact_plist_put(
                proc.childp,
                ProcessKeyword::Sentinel.value(),
                proc.sentinel,
            )?;
        }
        if !log_val.is_nil() {
            proc.log = log_val;
            proc.childp =
                process_contact_plist_put(proc.childp, ProcessKeyword::Log.value(), proc.log)?;
        }
        if !buffer.is_nil() {
            proc.childp =
                process_contact_plist_put(proc.childp, ProcessKeyword::Buffer.value(), buffer)?;
        }
        apply_connection_process_flags(proc, noquery, stop);
    }
    eval.processes.register_socket_fd(id).ok();
    Ok(Value::make_process(id))
}

/// `make-network-process` for `:family 'local` (unix-domain sockets):
/// server bind/listen or client connect on a filesystem socket path,
/// stream or datagram. Diverges on every path (non-unix builds signal),
/// so the inet resolution below never runs for local family. Extracted
/// verbatim from builtin_make_network_process.
#[allow(clippy::too_many_arguments)]
#[allow(unused_variables)] // family/host_value feed cfg(unix)-gated arms
fn connect_local_socket_process(
    eval: &mut super::eval::Context,
    family: NetworkProcessFamily,
    host_value: Value,
    service: Value,
    name: LispString,
    mut contact: Value,
    filter_val: Value,
    sentinel_val: Value,
    log_val: Value,
    resolved_coding: ProcessCodingSystems,
    buffer: Value,
    plist_val: Value,
    nowait: bool,
    server: bool,
    noquery: bool,
    stop: bool,
    server_backlog: Option<i32>,
    socket_type: NetworkSocketType,
    socket_options: Vec<NetworkSocketOptionSpec>,
    tls_parameters: Option<super::tls::GnutlsBootParameters>,
    remote_address_value: Value,
) -> EvalResult {
    #[cfg(not(unix))]
    {
        return Err(signal(
            "error",
            vec![Value::string("Unknown address family")],
        ));
    }

    #[cfg(unix)]
    {
        let service_path = crate::emacs_core::fileio::lisp_file_name_to_path_buf(
            super::builtins::expect_lisp_string(&service)?,
        );
        if !host_value.is_nil() {
            contact = process_contact_plist_put(contact, ProcessKeyword::Host.value(), Value::NIL)?;
        }

        if socket_type == NetworkSocketType::Datagram {
            let service_path_value = Value::heap_string(
                crate::emacs_core::fileio::path_to_lisp_file_name(&service_path),
            );
            if server {
                let socket = bind_unix_datagram_socket(&service_path, &socket_options)?;
                let zero_datagram = datagram_zero_unix_address();
                let (datagram_unix_path, datagram_address) = if !remote_address_value.is_nil() {
                    match parse_network_address_spec(&remote_address_value)? {
                        NetworkAddressSpec::Local(remote_path) => {
                            let remote_value = Value::heap_string(
                                crate::emacs_core::fileio::path_to_lisp_file_name(&remote_path),
                            );
                            (Some(remote_path), remote_value)
                        }
                        NetworkAddressSpec::Inet(_) => (None, zero_datagram),
                    }
                } else {
                    (None, zero_datagram)
                };
                contact = process_contact_plist_put(
                    contact,
                    ProcessKeyword::Local.value(),
                    service_path_value,
                )?;
                contact = process_contact_plist_put(
                    contact,
                    ProcessKeyword::Remote.value(),
                    datagram_address,
                )?;

                let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
                eval.processes.sync_process_mark(&mut eval.buffers, id)?;
                if let Some(proc) = eval.processes.get_mut(id) {
                    proc.childp = contact;
                    proc.thread = current_thread_handle(&eval.threads);
                    proc.plist = plist_val;
                    proc.status = ProcessStatusSymbol::Open.value();
                    proc.live_io.network_socket = Some(NetworkSocket::UnixDatagram(socket));
                    proc.datagram_address = datagram_address;
                    proc.datagram_unix_path = datagram_unix_path;
                    if !filter_val.is_nil() {
                        proc.filter = filter_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Filter.value(),
                            proc.filter,
                        )?;
                    }
                    if !sentinel_val.is_nil() {
                        proc.sentinel = sentinel_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Sentinel.value(),
                            proc.sentinel,
                        )?;
                    }
                    if !log_val.is_nil() {
                        proc.log = log_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Log.value(),
                            proc.log,
                        )?;
                    }
                    if !buffer.is_nil() {
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Buffer.value(),
                            buffer,
                        )?;
                    }
                    apply_connection_process_flags(proc, noquery, stop);
                }
                eval.processes.register_socket_fd(id).ok();
                return Ok(Value::make_process(id));
            }

            let socket = unbound_unix_datagram_socket(&socket_options)?;
            contact = process_contact_plist_put(
                contact,
                ProcessKeyword::Remote.value(),
                service_path_value,
            )?;
            contact = process_contact_plist_put(
                contact,
                ProcessKeyword::Local.value(),
                Value::string(""),
            )?;

            let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
            eval.processes.sync_process_mark(&mut eval.buffers, id)?;
            if let Some(proc) = eval.processes.get_mut(id) {
                proc.live_io.network_socket = Some(NetworkSocket::UnixDatagram(socket));
                proc.status = ProcessStatusSymbol::Open.value();
                proc.childp = contact;
                proc.plist = plist_val;
                proc.thread = current_thread_handle(&eval.threads);
                proc.datagram_address = service_path_value;
                proc.datagram_unix_path = Some(service_path);
                if !filter_val.is_nil() {
                    proc.filter = filter_val;
                    proc.childp = process_contact_plist_put(
                        proc.childp,
                        ProcessKeyword::Filter.value(),
                        proc.filter,
                    )?;
                }
                if !sentinel_val.is_nil() {
                    proc.sentinel = sentinel_val;
                    proc.childp = process_contact_plist_put(
                        proc.childp,
                        ProcessKeyword::Sentinel.value(),
                        proc.sentinel,
                    )?;
                }
                if !buffer.is_nil() {
                    proc.childp = process_contact_plist_put(
                        proc.childp,
                        ProcessKeyword::Buffer.value(),
                        buffer,
                    )?;
                }
                apply_connection_process_flags(proc, noquery, stop);
            }

            eval.processes.register_socket_fd(id).ok();
            return Ok(Value::make_process(id));
        }

        if socket_type == NetworkSocketType::Seqpacket {
            if server {
                let listener = bind_unix_seqpacket_listener_socket(
                    &service_path,
                    server_backlog.unwrap_or(5),
                    &socket_options,
                )?;
                contact = process_contact_plist_put(
                    contact,
                    ProcessKeyword::Local.value(),
                    Value::heap_string(crate::emacs_core::fileio::path_to_lisp_file_name(
                        &service_path,
                    )),
                )?;

                let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
                eval.processes.sync_process_mark(&mut eval.buffers, id)?;
                if let Some(proc) = eval.processes.get_mut(id) {
                    proc.childp = contact;
                    proc.thread = current_thread_handle(&eval.threads);
                    proc.plist = plist_val;
                    proc.live_io.network_socket = Some(NetworkSocket::SeqpacketListener(listener));
                    if !filter_val.is_nil() {
                        proc.filter = filter_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Filter.value(),
                            proc.filter,
                        )?;
                    }
                    if !sentinel_val.is_nil() {
                        proc.sentinel = sentinel_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Sentinel.value(),
                            proc.sentinel,
                        )?;
                    }
                    if !log_val.is_nil() {
                        proc.log = log_val;
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Log.value(),
                            proc.log,
                        )?;
                    }
                    if !buffer.is_nil() {
                        proc.childp = process_contact_plist_put(
                            proc.childp,
                            ProcessKeyword::Buffer.value(),
                            buffer,
                        )?;
                    }
                    apply_connection_process_flags(proc, noquery, stop);
                }
                eval.processes.register_socket_fd(id).ok();
                return Ok(Value::make_process(id));
            }

            let socket = connect_unix_seqpacket_socket(&service_path, &socket_options)?;
            contact = process_contact_plist_put(
                contact,
                ProcessKeyword::Remote.value(),
                Value::heap_string(crate::emacs_core::fileio::path_to_lisp_file_name(
                    &service_path,
                )),
            )?;
            contact = process_contact_plist_put(
                contact,
                ProcessKeyword::Local.value(),
                Value::string(""),
            )?;

            let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
            eval.processes.sync_process_mark(&mut eval.buffers, id)?;
            if let Some(proc) = eval.processes.get_mut(id) {
                proc.live_io.network_socket = Some(NetworkSocket::SeqpacketStream(socket));
                proc.status = process_status_run_value();
                proc.childp = contact;
                proc.plist = plist_val;
                proc.thread = current_thread_handle(&eval.threads);
                if !filter_val.is_nil() {
                    proc.filter = filter_val;
                    proc.childp = process_contact_plist_put(
                        proc.childp,
                        ProcessKeyword::Filter.value(),
                        proc.filter,
                    )?;
                }
                if !sentinel_val.is_nil() {
                    proc.sentinel = sentinel_val;
                    proc.childp = process_contact_plist_put(
                        proc.childp,
                        ProcessKeyword::Sentinel.value(),
                        proc.sentinel,
                    )?;
                }
                if !buffer.is_nil() {
                    proc.childp = process_contact_plist_put(
                        proc.childp,
                        ProcessKeyword::Buffer.value(),
                        buffer,
                    )?;
                }
                apply_connection_process_flags(proc, noquery, stop);
            }

            eval.processes.register_socket_fd(id).ok();

            return Ok(Value::make_process(id));
        }

        if server {
            let listener = bind_unix_listener_socket(
                &service_path,
                server_backlog.unwrap_or(5),
                &socket_options,
            )?;
            contact = process_contact_plist_put(
                contact,
                ProcessKeyword::Local.value(),
                Value::heap_string(crate::emacs_core::fileio::path_to_lisp_file_name(
                    &service_path,
                )),
            )?;

            let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
            eval.processes.sync_process_mark(&mut eval.buffers, id)?;
            if let Some(proc) = eval.processes.get_mut(id) {
                proc.childp = contact;
                proc.thread = current_thread_handle(&eval.threads);
                proc.plist = plist_val;
                proc.live_io.network_socket = Some(NetworkSocket::UnixListener(listener));
                if !filter_val.is_nil() {
                    proc.filter = filter_val;
                    proc.childp = process_contact_plist_put(
                        proc.childp,
                        ProcessKeyword::Filter.value(),
                        proc.filter,
                    )?;
                }
                if !sentinel_val.is_nil() {
                    proc.sentinel = sentinel_val;
                    proc.childp = process_contact_plist_put(
                        proc.childp,
                        ProcessKeyword::Sentinel.value(),
                        proc.sentinel,
                    )?;
                }
                if !log_val.is_nil() {
                    proc.log = log_val;
                    proc.childp = process_contact_plist_put(
                        proc.childp,
                        ProcessKeyword::Log.value(),
                        proc.log,
                    )?;
                }
                if !buffer.is_nil() {
                    proc.childp = process_contact_plist_put(
                        proc.childp,
                        ProcessKeyword::Buffer.value(),
                        buffer,
                    )?;
                }
                apply_connection_process_flags(proc, noquery, stop);
            }
            if let Some(parameters) = tls_parameters.clone() {
                upgrade_process_to_tls::<RustlsBackend>(
                    &mut eval.processes,
                    id,
                    &parameters.client,
                    "make-network-process",
                    signal_gnutls_boot_error,
                )?;
            }
            eval.processes.register_socket_fd(id).ok();
            return Ok(Value::make_process(id));
        }

        contact = process_contact_plist_put(
            contact,
            ProcessKeyword::Remote.value(),
            Value::heap_string(crate::emacs_core::fileio::path_to_lisp_file_name(
                &service_path,
            )),
        )?;
        contact =
            process_contact_plist_put(contact, ProcessKeyword::Local.value(), Value::string(""))?;

        if nowait {
            let start = start_nonblocking_unix_stream_socket(&service_path, &socket_options)?;
            let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
            eval.processes.sync_process_mark(&mut eval.buffers, id)?;
            if let Some(proc) = eval.processes.get_mut(id) {
                proc.status = process_status_connect_value();
                proc.childp = contact;
                proc.plist = plist_val;
                proc.thread = current_thread_handle(&eval.threads);
                if !filter_val.is_nil() {
                    proc.filter = filter_val;
                    proc.childp = process_contact_plist_put(
                        proc.childp,
                        ProcessKeyword::Filter.value(),
                        proc.filter,
                    )?;
                }
                if !sentinel_val.is_nil() {
                    proc.sentinel = sentinel_val;
                    proc.childp = process_contact_plist_put(
                        proc.childp,
                        ProcessKeyword::Sentinel.value(),
                        proc.sentinel,
                    )?;
                }
                if !buffer.is_nil() {
                    proc.childp = process_contact_plist_put(
                        proc.childp,
                        ProcessKeyword::Buffer.value(),
                        buffer,
                    )?;
                }
                match start {
                    Ok(stream) => {
                        proc.live_io.network_socket = Some(NetworkSocket::UnixStream(stream));
                        proc.live_io.pending_network_connect = Some(PendingNetworkConnect::Local);
                    }
                    Err(err) => {
                        proc.status = process_status_failed_value(io_error_status_code(&err));
                    }
                }
                apply_connection_process_flags(proc, noquery, stop);
            }
            if eval
                .processes
                .get(id)
                .is_some_and(|proc| proc.live_io.pending_network_connect.is_some())
            {
                eval.processes.register_socket_writable_fd(id).ok();
            }
            return Ok(Value::make_process(id));
        }

        let stream = connect_unix_stream_socket(&service_path, &socket_options)?;
        let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
        eval.processes.sync_process_mark(&mut eval.buffers, id)?;
        if let Some(proc) = eval.processes.get_mut(id) {
            proc.live_io.network_socket = Some(NetworkSocket::UnixStream(stream));
            proc.status = process_status_run_value();
            proc.childp = contact;
            proc.plist = plist_val;
            proc.thread = current_thread_handle(&eval.threads);
            if !filter_val.is_nil() {
                proc.filter = filter_val;
                proc.childp = process_contact_plist_put(
                    proc.childp,
                    ProcessKeyword::Filter.value(),
                    proc.filter,
                )?;
            }
            if !sentinel_val.is_nil() {
                proc.sentinel = sentinel_val;
                proc.childp = process_contact_plist_put(
                    proc.childp,
                    ProcessKeyword::Sentinel.value(),
                    proc.sentinel,
                )?;
            }
            if !buffer.is_nil() {
                proc.childp =
                    process_contact_plist_put(proc.childp, ProcessKeyword::Buffer.value(), buffer)?;
            }
            apply_connection_process_flags(proc, noquery, stop);
        }

        eval.processes.register_socket_fd(id).ok();

        Ok(Value::make_process(id))
    }
}

pub(crate) fn builtin_make_network_process(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if args.is_empty() {
        return Ok(Value::NIL);
    }
    check_keyword_arg_pairs(&args)?;
    eval.sync_process_read_config_from_visible_variables();

    // ---- Parse all keyword arguments ----
    let mut name: Option<LispString> = None;
    let mut host_value = Value::NIL;
    let mut service: Option<Value> = None;
    let mut server = false;
    let mut server_value = Value::NIL;
    let mut family_value = Value::NIL;
    let mut local_address_value = Value::NIL;
    let mut remote_address_value = Value::NIL;
    let mut nowait = false;
    let mut socket_type = NetworkSocketType::Stream;
    let mut contact = Value::list(args.clone());
    let mut filter_val = Value::NIL;
    let mut sentinel_val = Value::NIL;
    let mut log_val = Value::NIL;
    let mut buffer_val = Value::NIL;
    let mut coding_val = Value::NIL;
    let mut tls_parameters_val = Value::NIL;
    let mut noquery = false;
    let mut plist_val = Value::NIL;
    let mut stop_val = Value::NIL;
    let socket_options = collect_network_socket_options(&args);

    let mut seen_keywords = Vec::new();
    let mut i = 0usize;
    while i < args.len() {
        let key = &args[i];
        let value = args.get(i + 1).cloned().unwrap_or(Value::NIL);
        let Some(keyword) = ProcessKeyword::from_value(key) else {
            i += 2;
            continue;
        };
        if process_keyword_already_seen(&mut seen_keywords, keyword) {
            i += 2;
            continue;
        }
        match keyword {
            ProcessKeyword::Name => name = Some(expect_process_name_lisp_string(&value)?),
            ProcessKeyword::Host => host_value = value,
            ProcessKeyword::Service => service = Some(value),
            ProcessKeyword::Server => {
                server = value.is_truthy();
                server_value = value;
            }
            ProcessKeyword::Family => family_value = value,
            ProcessKeyword::Type => socket_type = parse_network_socket_type(&value)?,
            ProcessKeyword::Nowait => nowait = value.is_truthy(),
            ProcessKeyword::Filter => filter_val = value,
            ProcessKeyword::Sentinel => sentinel_val = value,
            ProcessKeyword::Log => log_val = value,
            ProcessKeyword::Buffer => buffer_val = value,
            ProcessKeyword::Coding => coding_val = value,
            ProcessKeyword::TlsParameters => tls_parameters_val = value,
            ProcessKeyword::Noquery => noquery = value.is_truthy(),
            ProcessKeyword::Stop => stop_val = value,
            ProcessKeyword::Local => local_address_value = value,
            ProcessKeyword::Remote => remote_address_value = value,
            ProcessKeyword::Plist => plist_val = value,
            _ => {}
        }
        i += 2;
    }

    let Some(name) = name else {
        return Err(signal(
            "error",
            vec![Value::string("Missing :name keyword parameter")],
        ));
    };

    if server && nowait {
        return Err(signal(
            "error",
            vec![Value::string("`:server' is incompatible with `:nowait'")],
        ));
    }
    let plist_val = copy_process_plist(plist_val)?;
    let stop = stop_val.is_truthy();
    let server_backlog = if server {
        Some(network_server_backlog(server_value)?)
    } else {
        None
    };
    let tls_parameters = parse_make_network_tls_parameters(tls_parameters_val)?;

    // Resolve :buffer to a buffer name (creating buffer if needed).
    let buffer = if !buffer_val.is_nil() {
        parse_make_process_buffer(eval, &buffer_val)?
    } else {
        Value::NIL
    };

    // Capture the dynamic coding environment once and pass it through every
    // connection strategy.  GNU's `set_network_socket_coding_system` gives
    // `coding-system-for-read/write` precedence over the operation/default
    // codings; URL binds the read side to `binary` around this call.
    let multibyte = ProcessBufferMultibyteness {
        process_buffer: match resolve_buffer_for_process_lookup_in_state(&eval.buffers, &buffer) {
            Ok(Some(bid)) => eval
                .buffers
                .get(bid)
                .map(|b| b.get_multibyte())
                .unwrap_or(true),
            // No buffer (or unresolved) -> `buffer_defaults` is multibyte.
            _ => true,
        },
        current_buffer: eval
            .buffers
            .current_buffer()
            .map(|buffer| buffer.get_multibyte())
            .unwrap_or(true),
    };
    let mut coding_environment = NetworkProcessCodingEnvironment {
        coding_system_for_read: eval.visible_variable_value_or_nil("coding-system-for-read"),
        coding_system_for_write: eval.visible_variable_value_or_nil("coding-system-for-write"),
        operation_coding_system: Value::NIL,
        default_process_coding_system: eval
            .visible_variable_value_or_nil("default-process-coding-system"),
        short_circuit: ConnectionProcessUnibyteShortCircuit::network(multibyte),
    };

    let explicit_address = if server {
        local_address_value
    } else {
        remote_address_value
    };
    if !explicit_address.is_nil() {
        // A `:local`/`:remote` address carries no HOST and SERVICE, so GNU's
        // `NILP (host) || NILP (service)` guard leaves `coding_systems` nil and
        // the alist is never reached (src/process.c:3325-3330).
        let resolved_coding =
            resolve_network_process_coding_systems(coding_val, coding_environment);
        return connect_network_process_at_explicit_address(
            eval,
            explicit_address,
            remote_address_value,
            name,
            contact,
            filter_val,
            sentinel_val,
            log_val,
            resolved_coding,
            buffer,
            plist_val,
            nowait,
            server,
            noquery,
            stop,
            server_backlog,
            socket_type,
            socket_options,
            tls_parameters,
            tls_parameters_val,
        );
    }

    let family = parse_network_process_family(&family_value)?;
    let host = parse_network_host(&host_value, family)?;

    let service = service.unwrap_or(Value::NIL);
    if service.is_nil() {
        return Err(signal_wrong_type_string(Value::NIL));
    }
    if coding_val.is_nil() {
        coding_environment.operation_coding_system =
            find_network_operation_coding_system(eval, &name, buffer, host_value, service)?;
    }
    let resolved_coding = resolve_network_process_coding_systems(coding_val, coding_environment);

    if family.is_local() {
        return connect_local_socket_process(
            eval,
            family,
            host_value,
            service,
            name,
            contact,
            filter_val,
            sentinel_val,
            log_val,
            resolved_coding,
            buffer,
            plist_val,
            nowait,
            server,
            noquery,
            stop,
            server_backlog,
            socket_type,
            socket_options,
            tls_parameters,
            remote_address_value,
        );
    }

    if socket_type == NetworkSocketType::Datagram {
        return connect_datagram_network_process(
            eval,
            family,
            host,
            service,
            name,
            contact,
            filter_val,
            sentinel_val,
            log_val,
            resolved_coding,
            buffer,
            plist_val,
            server,
            noquery,
            stop,
            socket_type,
            socket_options,
        );
    }

    #[cfg(unix)]
    if socket_type == NetworkSocketType::Seqpacket {
        return Err(signal(
            "error",
            vec![Value::string("Unsupported connection type")],
        ));
    }

    if server {
        return listen_stream_network_process(
            eval,
            family,
            host,
            service,
            name,
            contact,
            filter_val,
            sentinel_val,
            log_val,
            resolved_coding,
            buffer,
            plist_val,
            noquery,
            stop,
            server_backlog,
            socket_type,
            socket_options,
        );
    }

    // ---- Client mode: establish TCP connection ----
    let host_str = host.unwrap_or_else(|| family.loopback_host().to_string());
    let port = parse_network_service_port(&service, false, socket_type)?;

    if nowait {
        let immediate_start = nowait_tcp_immediate_addrs(host_value, &host_str, port, family)
            .map(|addrs| start_pending_tcp_stream_connect(addrs, &socket_options))
            .transpose()?;
        let pending_dns = if immediate_start.is_none() {
            Some(start_async_network_dns_lookup(
                host_str.clone(),
                port,
                family,
                NetworkSocketType::Stream,
                socket_options.clone(),
                eval.processes.wait_notifier(),
            ))
        } else {
            None
        };

        let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
        eval.processes.sync_process_mark(&mut eval.buffers, id)?;
        if let Some(proc) = eval.processes.get_mut(id) {
            proc.status = process_status_connect_value();
            proc.childp = contact;
            proc.plist = plist_val;
            proc.thread = current_thread_handle(&eval.threads);
            if !filter_val.is_nil() {
                proc.filter = filter_val;
                proc.childp = process_contact_plist_put(
                    proc.childp,
                    ProcessKeyword::Filter.value(),
                    proc.filter,
                )?;
            }
            if !sentinel_val.is_nil() {
                proc.sentinel = sentinel_val;
                proc.childp = process_contact_plist_put(
                    proc.childp,
                    ProcessKeyword::Sentinel.value(),
                    proc.sentinel,
                )?;
            }
            if !buffer.is_nil() {
                proc.childp =
                    process_contact_plist_put(proc.childp, ProcessKeyword::Buffer.value(), buffer)?;
            }
            if let Some(start) = immediate_start {
                match start {
                    PendingNetworkConnectStart::Started(started) => {
                        let local_addr = started.stream.local_addr().ok();
                        proc.live_io.network_socket =
                            Some(NetworkSocket::TcpStream(started.stream));
                        proc.live_io.pending_network_connect = Some(PendingNetworkConnect::Tcp {
                            remaining_addrs: started.remaining_addrs,
                            socket_options: socket_options.clone(),
                        });
                        ProcessManager::update_tcp_client_contact(
                            proc,
                            started.remote_addr,
                            local_addr,
                        )?;
                    }
                    PendingNetworkConnectStart::Failed(code) => {
                        proc.status = process_status_failed_value(code);
                    }
                }
            } else if let Some(pending_dns) = pending_dns {
                proc.live_io.pending_network_connect =
                    Some(PendingNetworkConnect::Dns(pending_dns));
            }
            if proc.live_io.pending_network_connect.is_some() {
                proc.gnutls_boot_parameters = tls_parameters_val;
            }
            apply_connection_process_flags(proc, noquery, stop);
        }
        if eval.processes.get(id).is_some_and(|proc| {
            proc.live_io.network_socket.is_some() && proc.live_io.pending_network_connect.is_some()
        }) {
            eval.processes.register_socket_writable_fd(id).ok();
        }
        return Ok(Value::make_process(id));
    }

    let stream =
        connect_tcp_stream_host(host_str.as_str(), port, family, &socket_options, contact)?;
    let remote_addr = stream.peer_addr().ok();
    let local_addr = stream.local_addr().ok();

    let id = create_network_process_record(eval, name, buffer, resolved_coding)?;
    eval.processes.sync_process_mark(&mut eval.buffers, id)?;
    if let Some(addr) = remote_addr {
        contact = process_contact_plist_put(
            contact,
            ProcessKeyword::Remote.value(),
            socket_addr_to_lisp_value(addr),
        )?;
    }
    if let Some(addr) = local_addr {
        contact = process_contact_plist_put(
            contact,
            ProcessKeyword::Local.value(),
            socket_addr_to_lisp_value(addr),
        )?;
    }
    if let Some(proc) = eval.processes.get_mut(id) {
        proc.live_io.network_socket = Some(NetworkSocket::TcpStream(stream));
        proc.status = process_status_run_value();
        proc.childp = contact;
        proc.plist = plist_val;
        proc.thread = current_thread_handle(&eval.threads);
        if !filter_val.is_nil() {
            proc.filter = filter_val;
            proc.childp = process_contact_plist_put(
                proc.childp,
                ProcessKeyword::Filter.value(),
                proc.filter,
            )?;
        }
        if !sentinel_val.is_nil() {
            proc.sentinel = sentinel_val;
            proc.childp = process_contact_plist_put(
                proc.childp,
                ProcessKeyword::Sentinel.value(),
                proc.sentinel,
            )?;
        }
        if !buffer.is_nil() {
            proc.childp =
                process_contact_plist_put(proc.childp, ProcessKeyword::Buffer.value(), buffer)?;
        }
        apply_connection_process_flags(proc, noquery, stop);
    }

    if let Some(parameters) = tls_parameters {
        upgrade_process_to_tls::<RustlsBackend>(
            &mut eval.processes,
            id,
            &parameters.client,
            "make-network-process",
            signal_gnutls_boot_error,
        )?;
    }

    eval.processes.register_socket_fd(id).ok();

    // GNU fires NO sentinel for a synchronous (non-:nowait) connect -- see the
    // TCP branch above.
    Ok(Value::make_process(id))
}

/// (make-pipe-process &rest ARGS) -> process-or-nil
pub(crate) fn builtin_make_pipe_process(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    eval.sync_process_read_config_from_visible_variables();
    let coding_variables = read_connection_process_coding_variables(eval);
    builtin_make_pipe_process_impl(
        &mut eval.processes,
        &mut eval.buffers,
        &eval.threads,
        Some(&eval.coding_systems),
        coding_variables,
        args,
    )
}

/// Capture the dynamic coding variables the way GNU reads them: at the moment
/// the primitive runs, in the buffer that is current then.
fn read_connection_process_coding_variables(
    eval: &mut super::eval::Context,
) -> ConnectionProcessCodingVariables {
    ConnectionProcessCodingVariables {
        coding_system_for_read: eval.visible_variable_value_or_nil("coding-system-for-read"),
        coding_system_for_write: eval.visible_variable_value_or_nil("coding-system-for-write"),
        default_process_coding_system: eval
            .visible_variable_value_or_nil("default-process-coding-system"),
    }
}

pub(crate) fn builtin_make_pipe_process_impl(
    processes: &mut ProcessManager,
    buffers: &mut BufferManager,
    threads: &ThreadManager,
    coding_systems: Option<&super::coding::CodingSystemManager>,
    coding_variables: ConnectionProcessCodingVariables,
    args: Vec<Value>,
) -> EvalResult {
    if args.is_empty() {
        return Ok(Value::NIL);
    }
    check_keyword_arg_pairs(&args)?;

    let contact = Value::list(args.clone());
    let mut name: Option<LispString> = None;
    let mut buffer: Option<Value> = None;
    let mut filter = Value::NIL;
    let mut sentinel = Value::NIL;
    let mut coding = Value::NIL;
    let mut coding_present = false;
    let mut noquery = false;
    let mut stop = false;
    let mut plist = Value::NIL;

    let mut seen_keywords = Vec::new();
    let mut i = 0usize;
    while i < args.len() {
        let key = &args[i];
        let value = args.get(i + 1).cloned().unwrap_or(Value::NIL);
        let Some(keyword) = ProcessKeyword::from_value(key) else {
            i += 2;
            continue;
        };
        if process_keyword_already_seen(&mut seen_keywords, keyword) {
            i += 2;
            continue;
        }
        match keyword {
            ProcessKeyword::Name => {
                name = Some(expect_process_name_lisp_string(&value)?);
            }
            ProcessKeyword::Buffer => {
                buffer = Some(parse_make_process_buffer_in_state(buffers, &value)?);
            }
            ProcessKeyword::Filter => filter = value,
            ProcessKeyword::Sentinel => sentinel = value,
            ProcessKeyword::Coding => {
                coding = value;
                coding_present = true;
            }
            ProcessKeyword::Noquery => noquery = value.is_truthy(),
            ProcessKeyword::Stop => stop = value.is_truthy(),
            ProcessKeyword::Plist => plist = value,
            _ => {}
        }
        i += 2;
    }

    let Some(name) = name else {
        return Err(signal(
            "error",
            vec![Value::string("Missing :name keyword parameter")],
        ));
    };

    let resolved_buffer = match buffer {
        Some(explicit) => explicit,
        None => {
            // Issue #131: buffer-name lookup/creation takes a `&str`; a lossy
            // UTF-8 rendering is the right display form here and avoids the
            // buggy storage-string sentinels.
            let name_runtime = crate::emacs_core::emacs_char::to_utf8_lossy(name.as_bytes());
            let id = buffers
                .find_buffer_by_name(&name_runtime)
                .unwrap_or_else(|| buffers.create_buffer(&name_runtime));
            Value::make_buffer(id)
        }
    };
    // GNU runs `Fmake_pipe_process`'s own chain here (src/process.c:2517-2570)
    // and then validates the RESULT, not the `:coding` keyword, by handing it
    // to `setup_process_coding_systems` -> `setup_coding_system`
    // ("This may signal an error", :2573).  So a bad `coding-system-for-read`
    // signals too -- measured under GNU 31.0.90.
    let resolved_coding = resolve_pipe_process_coding_systems(
        coding,
        coding_variables.pipe(process_buffer_multibyteness(buffers, resolved_buffer)),
    );
    validate_resolved_process_coding_systems(coding_systems, resolved_coding)?;
    let plist = copy_process_plist(plist)?;
    let (pipe_reader, pipe_writer) = os_pipe::pipe().map_err(|error| {
        signal(
            LispCondition::FileError,
            vec![Value::string(format!("Creating pipe: {error}"))],
        )
    })?;
    let pipe_reader = ChildOutputReader::Shared(pipe_reader);

    let id = processes.create_process_with_kind_lisp(
        name,
        resolved_buffer,
        LispString::from_utf8("pipe"),
        Vec::new(),
        ProcessKindWithoutDevice::Pipe,
        resolved_coding,
    );
    processes.sync_process_mark(buffers, id)?;
    if let Some(proc) = processes.get_mut(id) {
        proc.childp = contact;
        proc.thread = current_thread_handle(threads);
        proc.plist = plist;
        if !filter.is_nil() {
            proc.filter = filter;
        }
        if !sentinel.is_nil() {
            proc.sentinel = sentinel;
        }
        proc.coding_explicitly_set = coding_present;
        apply_connection_process_flags(proc, noquery, stop);
    }
    if processes.get(id).is_some_and(process_filter_accepts_output)
        && let Some(poller) = processes.wait_backend.poller()
    {
        ProcessManager::register_child_stdout_with_poller(poller, &pipe_reader, id);
    }
    if let Some(proc) = processes.get_mut(id) {
        proc.live_io.child_stdout = Some(pipe_reader);
        proc.live_io.module_pipe_writer = Some(pipe_writer);
    }
    Ok(Value::make_process(id))
}

/// (make-serial-process &rest ARGS) -> process-or-nil
pub(crate) fn builtin_make_serial_process(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    eval.sync_process_read_config_from_visible_variables();
    let coding_variables = read_connection_process_coding_variables(eval);
    builtin_make_serial_process_impl(
        &mut eval.processes,
        &mut eval.buffers,
        &eval.threads,
        Some(&eval.coding_systems),
        coding_variables,
        args,
    )
}

pub(crate) fn builtin_make_serial_process_impl(
    processes: &mut ProcessManager,
    buffers: &mut BufferManager,
    threads: &ThreadManager,
    coding_systems: Option<&super::coding::CodingSystemManager>,
    coding_variables: ConnectionProcessCodingVariables,
    args: Vec<Value>,
) -> EvalResult {
    if args.is_empty() {
        return Ok(Value::NIL);
    }
    check_keyword_arg_pairs(&args)?;

    let contact = Value::list(args.clone());
    let mut name: Option<LispString> = None;
    let mut port: Option<Value> = None;
    let mut port_name: Option<LispString> = None;
    let mut speed: Option<Value> = None;
    let mut buffer: Option<Value> = None;
    let mut filter = Value::NIL;
    let mut sentinel = Value::NIL;
    let mut coding = Value::NIL;
    let mut coding_present = false;
    let mut noquery = false;
    let mut stop = false;
    let mut plist = Value::NIL;

    let mut seen_keywords = Vec::new();
    let mut i = 0usize;
    while i < args.len() {
        let key = &args[i];
        let value = args.get(i + 1).cloned().unwrap_or(Value::NIL);
        let Some(keyword) = ProcessKeyword::from_value(key) else {
            i += 2;
            continue;
        };
        if process_keyword_already_seen(&mut seen_keywords, keyword) {
            i += 2;
            continue;
        }
        match keyword {
            ProcessKeyword::Name => {
                name = Some(expect_process_name_lisp_string(&value)?);
            }
            ProcessKeyword::Port => {
                if value.is_nil() {
                    port = None;
                } else {
                    let string = super::builtins::expect_lisp_string(&value)?.clone();
                    port = Some(value);
                    port_name = Some(string);
                }
            }
            ProcessKeyword::Speed => speed = Some(value),
            ProcessKeyword::Buffer => buffer = Some(value),
            ProcessKeyword::Filter => filter = value,
            ProcessKeyword::Sentinel => sentinel = value,
            ProcessKeyword::Coding => {
                coding = value;
                coding_present = true;
            }
            ProcessKeyword::Noquery => noquery = value.is_truthy(),
            ProcessKeyword::Stop => stop = value.is_truthy(),
            ProcessKeyword::Plist => plist = value,
            _ => {}
        }
        i += 2;
    }

    // GNU checks the PORT before it looks at `:speed` at all
    // (src/process.c:3193-3200), so a missing or non-string port beats a
    // non-fixnum speed.  Measured, GNU 31.0.90:
    //   (make-serial-process :speed "x")          => (error "No port specified")
    //   (make-serial-process :port 1 :speed "x")  => (wrong-type-argument stringp 1)
    let (Some(port), Some(port_name)) = (port, port_name) else {
        return Err(signal("error", vec![Value::string("No port specified")]));
    };
    let Some(speed) = speed else {
        return Err(signal("error", vec![Value::string(":speed not specified")]));
    };
    if !speed.is_nil() && !speed.is_fixnum() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("fixnump"), speed],
        ));
    }
    let name = name.unwrap_or_else(|| port_name.clone());

    // GNU `serial_open`, src/process.c:3212 -- and everything below this line
    // is downstream of it.  A port that cannot be opened is reported before
    // the process buffer is created, before the coding chain runs and before
    // `serial-process-configure` is called, so `:buffer "x" :bytesize 5` on a
    // nonexistent port still reports `file-missing` and still leaves no buffer
    // named "x" behind.  All three orderings measured against GNU 31.0.90.
    let device = open_serial_port(port, &port_name)?;

    // GNU creates the buffer here (:3226), which is why a LATER failure
    // -- an undefined coding system, an invalid `:bytesize`, a `tcgetattr` on
    // something that is not a tty -- leaves the buffer behind even though it
    // unwinds the process record itself.
    let resolved_buffer = match buffer {
        Some(explicit) if !explicit.is_nil() => {
            parse_make_process_buffer_in_state(buffers, &explicit)?
        }
        _ => {
            let name_runtime = crate::emacs_core::emacs_char::to_utf8_lossy(name.as_bytes());
            let id = buffers
                .find_buffer_by_name(&name_runtime)
                .unwrap_or_else(|| buffers.create_buffer(&name_runtime));
            Value::make_buffer(id)
        }
    };

    // GNU resolves and then validates through `setup_process_coding_systems`
    // (src/process.c:3277), the same two steps `make-pipe-process` takes.
    let resolved_coding = resolve_serial_process_coding_systems(coding, coding_variables.serial());
    validate_resolved_process_coding_systems(coding_systems, resolved_coding)?;
    let plist = copy_process_plist(plist)?;

    // GNU `Fserial_process_configure (nargs, args)`, src/process.c:3284, whose
    // first act is to return without touching the device when the contact's
    // `:speed` is nil (:3098-3099, documented as "the serial port is not
    // configured any further").  That is measurable and not a shortcut: a
    // `:speed nil` port on `/dev/null` is created successfully, where the same
    // port with `:speed 9600` reports `Failed tcgetattr`.
    let childp = if process_contact_plist_get(contact, ProcessKeyword::Speed.value()).is_nil() {
        contact
    } else {
        configure_serial_device(&device, contact, contact)?
    };

    let id = processes.create_serial_process(name, resolved_buffer, device, resolved_coding);
    processes.sync_process_mark(buffers, id)?;
    if let Some(proc) = processes.get_mut(id) {
        proc.childp = childp;
        proc.thread = current_thread_handle(threads);
        proc.plist = plist;
        if !filter.is_nil() {
            proc.filter = filter;
        }
        if !sentinel.is_nil() {
            proc.sentinel = sentinel;
        }
        proc.coding_explicitly_set = coding_present;
        apply_connection_process_flags(proc, noquery, stop);
    }
    processes.register_serial_read_fd(id);
    Ok(Value::make_process(id))
}

/// (serial-process-configure &rest ARGS) -> nil
pub(crate) fn builtin_serial_process_configure(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_serial_process_configure_impl(&mut eval.processes, &eval.buffers, args)
}

pub(crate) fn builtin_serial_process_configure_impl(
    processes: &mut ProcessManager,
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    check_keyword_arg_pairs(&args)?;

    let contact = Value::list(args);
    let process_designator = [
        ProcessKeyword::Process,
        ProcessKeyword::Name,
        ProcessKeyword::Buffer,
        ProcessKeyword::Port,
    ]
    .into_iter()
    .map(|keyword| process_contact_plist_get(contact, keyword.value()))
    .find(|value| !value.is_nil())
    .unwrap_or(Value::NIL);
    let id = resolve_get_process_designator_in_state(processes, buffers, &process_designator)?;
    let proc = processes
        .get_mut(id)
        .ok_or_else(|| signal_wrong_type_processp(Value::make_process(id)))?;
    if proc.kind != ProcessKind::Serial {
        return Err(signal("error", vec![Value::string("Not a serial process")]));
    }
    // GNU `Fserial_process_configure`, src/process.c:3098-3099: a contact whose
    // `:speed` is nil means "do not configure the port any further", so the
    // primitive returns before `serial_configure` and never touches the device.
    if process_contact_plist_get(proc.childp, ProcessKeyword::Speed.value()).is_nil() {
        return Ok(Value::NIL);
    }
    // GNU reaches the device through `p->outfd` (src/sysdep.c:3162, :3303).
    // A serial process that has been `delete-process`ed has no device left;
    // GNU would `tcgetattr` a closed descriptor and report `Failed tcgetattr`,
    // which is what an `EBADF` here produces too.
    let current = proc.childp;
    let result = match proc.live_io.serial_port.as_ref() {
        Some(device) => configure_serial_device(device, current, contact),
        None => Err(signal_file_errno(
            "Failed tcgetattr",
            Value::NIL,
            libc::EBADF,
        )),
    };
    proc.childp = result?;
    Ok(Value::NIL)
}

/// (set-network-process-option PROCESS OPTION VALUE &optional NO-ERROR) -> t-or-nil
pub(crate) fn builtin_set_network_process_option(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if (3..=4).contains(&args.len())
        && let Some(id) = pending_network_connect_id(&eval.processes, args[0])?
    {
        eval.wait_while_network_process_connecting(id)?;
    }
    builtin_set_network_process_option_impl(&mut eval.processes, args)
}

pub(crate) fn builtin_set_network_process_option_impl(
    processes: &mut ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() < 3 || args.len() > 4 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("set-network-process-option"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    let id = resolve_live_process_or_wrong_type_in_manager(processes, &args[0])?;
    let proc = processes.get_mut(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    if proc.kind != ProcessKind::Network {
        return Err(signal(
            "error",
            vec![Value::string("Process is not a network process")],
        ));
    }

    if args[1].as_symbol_name().is_none() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), args[1]],
        ));
    }
    let no_error = args.get(3).is_some_and(|v| v.is_truthy());
    let Some(keyword) = ProcessKeyword::from_value(&args[1]) else {
        return if no_error {
            Ok(Value::NIL)
        } else {
            Err(signal(
                "error",
                vec![Value::string("Unknown or unsupported option")],
            ))
        };
    };
    let Some(option) = NetworkSocketOption::from_keyword(keyword) else {
        return if no_error {
            Ok(Value::NIL)
        } else {
            Err(signal(
                "error",
                vec![Value::string("Unknown or unsupported option")],
            ))
        };
    };

    let spec = NetworkSocketOptionSpec {
        keyword,
        option,
        value: args[2],
    };
    apply_network_socket_option_to_process(proc, spec)?;
    proc.childp = process_contact_plist_put(proc.childp, args[1], args[2])?;
    Ok(Value::T)
}

// `start-process', `start-process-shell-command', `start-file-process' and
// `start-file-process-shell-command' are NOT here: GNU has no C version of
// any of them.  They are Lisp over `make-process' (which IS in C,
// src/process.c:1767) -- lisp/subr.el:3466, lisp/subr.el:5063,
// lisp/simple.el:5249 and lisp/subr.el:5076 -- and we load those files.
// DIVERGENCES.md 149 deleted the Rust subrs that used to shadow them.

/// (call-process PROGRAM &optional INFILE DESTINATION DISPLAY &rest ARGS)
///
/// Runs the command synchronously using `std::process::Command`, captures
/// output.  Returns the exit code as an integer.
pub(crate) fn builtin_call_process(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    super::callproc::builtin_call_process(eval, args)
}

/// (call-process-shell-command COMMAND &optional INFILE DESTINATION DISPLAY &rest ARGS)
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_call_process_shell_command(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    super::callproc::builtin_call_process_shell_command(eval, args)
}

/// (process-file PROGRAM &optional INFILE DESTINATION DISPLAY &rest ARGS)
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_process_file(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    super::callproc::builtin_process_file(eval, args)
}

/// (process-file-shell-command COMMAND &optional INFILE DESTINATION DISPLAY &rest ARGS)
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_process_file_shell_command(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    super::callproc::builtin_process_file_shell_command(eval, args)
}

/// (process-lines PROGRAM &rest ARGS) -> list of lines
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_process_lines(
    _eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    super::callproc::builtin_process_lines(_eval, args)
}

/// (process-lines-ignore-status PROGRAM &rest ARGS) -> list of lines
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_process_lines_ignore_status(
    _eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    super::callproc::builtin_process_lines_ignore_status(_eval, args)
}

/// (process-lines-handling-status PROGRAM STATUS-HANDLER &rest ARGS) -> list of lines
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_process_lines_handling_status(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    super::callproc::builtin_process_lines_handling_status(eval, args)
}

/// (call-process-region START END PROGRAM &optional DELETE DESTINATION DISPLAY &rest ARGS)
///
/// Pipes buffer region from START to END through PROGRAM.
pub(crate) fn builtin_call_process_region(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    super::callproc::builtin_call_process_region(eval, args)
}

/// (delete-process PROCESS) -> nil
pub(crate) fn builtin_delete_process(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() > 1 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("delete-process"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let id = if let Some(process) = args.first() {
        if process.as_symbol_name() == Some("message") {
            resolve_get_process_designator_in_state(&eval.processes, &eval.buffers, &Value::NIL)?
        } else {
            resolve_get_process_designator_in_state(&eval.processes, &eval.buffers, process)?
        }
    } else {
        resolve_optional_process_or_current_buffer_in_state(&eval.processes, &eval.buffers, None)?
    };
    let was_terminal = eval
        .processes
        .get(id)
        .is_some_and(|proc| process_status_is_terminal_for_notify(&proc.status));
    let was_pending_notification = eval
        .processes
        .get(id)
        .is_some_and(|proc| proc.status_notify_pending);
    eval.delete_process_running_its_sentinel(id, !was_terminal || was_pending_notification)?;
    Ok(Value::NIL)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_delete_process_impl(
    processes: &mut ProcessManager,
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() > 1 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("delete-process"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let id = if let Some(process) = args.first() {
        if process.as_symbol_name() == Some("message") {
            resolve_get_process_designator_in_state(processes, buffers, &Value::NIL)?
        } else {
            resolve_get_process_designator_in_state(processes, buffers, process)?
        }
    } else {
        resolve_optional_process_or_current_buffer_in_state(processes, buffers, None)?
    };
    processes.delete_process(id);
    Ok(Value::NIL)
}

/// (continue-process &optional PROCESS CURRENT-GROUP) -> process-or-nil
pub(crate) fn builtin_continue_process(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() > 2 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("continue-process"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let (id, _) = resolve_optional_process_with_explicit_return_in_state(
        &eval.processes,
        &eval.buffers,
        args.first(),
    )?;
    let ret = builtin_continue_process_impl(&mut eval.processes, &eval.buffers, args)?;
    if eval
        .processes
        .get_any(id)
        .is_some_and(|proc| proc.kind == ProcessKind::Real && process_status_is_run(&proc.status))
    {
        eval.notify_process_status_sentinel(id)?;
    }
    Ok(ret)
}

pub(crate) fn builtin_continue_process_impl(
    processes: &mut ProcessManager,
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() > 2 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("continue-process"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let (id, ret) =
        resolve_optional_process_with_explicit_return_in_state(processes, buffers, args.first())?;
    // GNU's `Fcontinue_process` handles a network, serial or pipe process
    // before it resolves anything (src/process.c:7294-7315) and sets
    // `p->command' whether or not the connection is still listed -- hence
    // `get_any_mut'.  The signalling branch stays live-only: a retired REAL
    // process was rejected by the resolver above, as GNU's `p->infd < 0'
    // rejects it at :7087-7089.
    let is_live = processes.get(id).is_some();
    let mut continued = false;
    if let Some(proc) = processes.get_any_mut(id) {
        if matches!(
            proc.kind,
            ProcessKind::Network | ProcessKind::Pipe | ProcessKind::Serial
        ) {
            proc.command = Value::NIL;
            if proc.kind == ProcessKind::Serial {
                proc.status = ProcessStatusSymbol::Open.value();
            }
        } else if is_live {
            // GNU `process_send_signal(SIGCONT)` discards `raw_status_new`
            // before publishing `run`, so a queued stop transition cannot
            // overwrite the explicit continuation on the next status pass.
            proc.status_notify_pending = false;
            proc.pending_status = Value::NIL;
            proc.status = process_status_run_value();
            #[cfg(unix)]
            let _ =
                deliver_process_signal(proc, libc::SIGCONT, ProcessSignalRecipient::ProcessGroup);
            continued = true;
        }
    }
    if continued {
        // GNU's `p->tick = ++process_tick;` at src/process.c:7178, between the
        // `pset_status (p, Qrun)` above and the `status_notify (NULL, NULL)`
        // at :7181 -- which `builtin_continue_process` runs, so the tick is
        // consumed in the same call.
        processes.record_status_change(StatusChangeSite::ProcessSendSignalSigcont, id);
    }
    Ok(ret)
}

/// (interrupt-process &optional PROCESS CURRENT-GROUP) -> process-or-nil
pub(crate) fn builtin_interrupt_process(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() > 2 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("interrupt-process"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    eval.funcall_general(
        Value::symbol("run-hook-with-args-until-success"),
        vec![
            Value::symbol("interrupt-process-functions"),
            args.first().copied().unwrap_or(Value::NIL),
            args.get(1).copied().unwrap_or(Value::NIL),
        ],
    )
}

/// (kill-process &optional PROCESS CURRENT-GROUP) -> process-or-nil
pub(crate) fn builtin_kill_process(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_kill_process_impl(&mut eval.processes, &eval.buffers, args)
}

pub(crate) fn builtin_kill_process_impl(
    processes: &mut ProcessManager,
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() > 2 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("kill-process"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let (id, ret) =
        resolve_optional_process_with_explicit_return_in_state(processes, buffers, args.first())?;
    check_process_is_real_subprocess(processes, id)?;
    if let Some(proc) = processes.get_mut(id) {
        kill_real_process_child(proc, signal_kill_number());
    }
    Ok(ret)
}

/// (signal-process PROCESS SIGNAL &optional CURRENT-GROUP) -> int-or-nil
pub(crate) fn builtin_signal_process(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("signal-process", &args, 2)?;
    if args.len() > 3 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("signal-process"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    eval.funcall_general(
        Value::symbol("run-hook-with-args-until-success"),
        vec![
            Value::symbol("signal-process-functions"),
            args[0],
            args[1],
            args.get(2).copied().unwrap_or(Value::NIL),
        ],
    )
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_signal_process_impl(
    processes: &mut ProcessManager,
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("signal-process", &args, 2)?;
    if args.len() > 3 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("signal-process"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    // The type check precedes the liveness check here too: a retired pipe must
    // reach `signal_cannot_signal_process` below (GNU's `p->pid <= 0` at
    // src/process.c:7381-7382), not this early -1.
    if let Some(process) = args.first()
        && !process.is_nil()
        && is_stale_real_process_designator_in_manager(processes, process)
    {
        return Ok(Value::fixnum(-1));
    }

    let signal_num = parse_signal_number(&args[1])?;
    match resolve_signal_process_target_in_state(processes, buffers, args.first())? {
        SignalProcessTarget::Process(id) => {
            // `get_any_mut`, matching `internal-default-signal-process` above:
            // GNU's `CHECK_PROCESS` + `XPROCESS (process)->pid`
            // (src/process.c:7379-7382) asks the object, not the alist.  This
            // twin is only reachable from tests, but it carried the shape the
            // rest of this file no longer has, which is how the class comes
            // back.
            if let Some(proc) = processes.get_any_mut(id) {
                if proc.kind != ProcessKind::Real {
                    return Err(signal_cannot_signal_process(proc));
                }
                return Ok(Value::fixnum(signal_process_or_unbacked_success(
                    proc,
                    signal_num,
                    ProcessSignalRecipient::ImmediateProcess,
                ) as i64));
            }
            Ok(Value::fixnum(-1))
        }
        SignalProcessTarget::MissingNamedProcess => Ok(Value::NIL),
        SignalProcessTarget::Pid(pid) => {
            Ok(Value::fixnum(sys::send_signal(pid, signal_num) as i64))
        }
    }
}

/// (stop-process &optional PROCESS CURRENT-GROUP) -> process-or-nil
pub(crate) fn builtin_stop_process(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_stop_process_impl(&mut eval.processes, &eval.buffers, args)
}

pub(crate) fn builtin_stop_process_impl(
    processes: &mut ProcessManager,
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() > 2 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("stop-process"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let (id, ret) =
        resolve_optional_process_with_explicit_return_in_state(processes, buffers, args.first())?;
    // GNU's `Fstop_process` special-cases a network, serial or pipe process
    // before it resolves anything (src/process.c:7267-7278): it sets
    // `p->command' to t and returns the process, with no liveness test at all.
    let is_live = processes.get(id).is_some();
    if let Some(proc) = processes.get_any_mut(id) {
        if matches!(
            proc.kind,
            ProcessKind::Network | ProcessKind::Pipe | ProcessKind::Serial
        ) {
            proc.command = Value::T;
        } else if is_live {
            #[cfg(unix)]
            let _ =
                deliver_process_signal(proc, libc::SIGTSTP, ProcessSignalRecipient::ProcessGroup);
        }
    }
    Ok(ret)
}

/// (quit-process &optional PROCESS CURRENT-GROUP) -> process-or-nil
pub(crate) fn builtin_quit_process(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_quit_process_impl(&mut eval.processes, &eval.buffers, args)
}

pub(crate) fn builtin_quit_process_impl(
    processes: &mut ProcessManager,
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() > 2 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("quit-process"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let (id, ret) =
        resolve_optional_process_with_explicit_return_in_state(processes, buffers, args.first())?;
    check_process_is_real_subprocess(processes, id)?;
    if let Some(proc) = processes.get_mut(id) {
        // Send SIGQUIT to the child process.
        #[cfg(unix)]
        let _ = deliver_process_signal(proc, libc::SIGQUIT, ProcessSignalRecipient::ProcessGroup);
    }
    Ok(ret)
}

/// (process-attributes PID) -> alist-or-nil
pub(crate) fn builtin_process_attributes(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-attributes", &args, 1)?;
    if let Some(default_directory) = visible_default_directory_lisp(eval) {
        let operation = Value::symbol("process-attributes");
        let handler = super::fileio::find_file_name_handler_lisp_for_eval(
            eval,
            &default_directory,
            operation,
        );
        if !handler.is_nil() {
            return eval.funcall_general(handler, vec![operation, args[0]]);
        }
    }
    builtin_process_attributes_impl(args)
}

pub(crate) fn builtin_process_attributes_impl(args: Vec<Value>) -> EvalResult {
    expect_args("process-attributes", &args, 1)?;
    let pid = cons_to_os_pid(args[0])?;
    if !sys::process_is_alive(pid) {
        return Ok(Value::NIL);
    }

    let mut attrs = Vec::new();
    if let Some((euid, egid)) = sys::process_effective_ids(pid) {
        attrs.push(Value::cons(
            Value::symbol("group"),
            Value::string(sys::group_name(egid).unwrap_or_else(|| egid.to_string())),
        ));
        attrs.push(Value::cons(
            Value::symbol("egid"),
            Value::fixnum(egid as i64),
        ));
        attrs.push(Value::cons(
            Value::symbol("user"),
            Value::string(sys::user_name(euid).unwrap_or_else(|| euid.to_string())),
        ));
        attrs.push(Value::cons(
            Value::symbol("euid"),
            Value::fixnum(euid as i64),
        ));
    }

    let stat = sys::process_stat(pid).unwrap_or_else(|| sys::ProcStatSnapshot::fallback(pid));
    attrs.push(Value::cons(
        Value::symbol("comm"),
        Value::string(stat.comm.clone()),
    ));
    attrs.push(Value::cons(
        Value::symbol("state"),
        Value::string(stat.state),
    ));
    attrs.push(Value::cons(Value::symbol("ppid"), Value::fixnum(stat.ppid)));
    attrs.push(Value::cons(Value::symbol("pgrp"), Value::fixnum(stat.pgrp)));
    attrs.push(Value::cons(Value::symbol("sess"), Value::fixnum(stat.sess)));
    attrs.push(Value::cons(
        Value::symbol("tpgid"),
        Value::fixnum(stat.tpgid),
    ));
    attrs.push(Value::cons(
        Value::symbol("minflt"),
        Value::fixnum(stat.minflt),
    ));
    attrs.push(Value::cons(
        Value::symbol("majflt"),
        Value::fixnum(stat.majflt),
    ));
    attrs.push(Value::cons(
        Value::symbol("cminflt"),
        Value::fixnum(stat.cminflt),
    ));
    attrs.push(Value::cons(
        Value::symbol("cmajflt"),
        Value::fixnum(stat.cmajflt),
    ));
    attrs.push(Value::cons(
        Value::symbol("utime"),
        time_list_from_ticks(stat.utime_ticks, sys::clock_ticks_per_second()),
    ));
    attrs.push(Value::cons(
        Value::symbol("stime"),
        time_list_from_ticks(stat.stime_ticks, sys::clock_ticks_per_second()),
    ));
    let total_ticks = stat.utime_ticks.saturating_add(stat.stime_ticks);
    attrs.push(Value::cons(
        Value::symbol("time"),
        time_list_from_ticks(total_ticks, sys::clock_ticks_per_second()),
    ));
    attrs.push(Value::cons(
        Value::symbol("cutime"),
        time_list_from_ticks(stat.cutime_ticks, sys::clock_ticks_per_second()),
    ));
    attrs.push(Value::cons(
        Value::symbol("cstime"),
        time_list_from_ticks(stat.cstime_ticks, sys::clock_ticks_per_second()),
    ));
    let total_child_ticks = stat.cutime_ticks.saturating_add(stat.cstime_ticks);
    attrs.push(Value::cons(
        Value::symbol("ctime"),
        time_list_from_ticks(total_child_ticks, sys::clock_ticks_per_second()),
    ));
    attrs.push(Value::cons(Value::symbol("pri"), Value::fixnum(stat.pri)));
    attrs.push(Value::cons(Value::symbol("nice"), Value::fixnum(stat.nice)));
    attrs.push(Value::cons(
        Value::symbol("thcount"),
        Value::fixnum(stat.thcount),
    ));
    let hz = sys::clock_ticks_per_second();
    let start_epoch_time = sys::boot_time_secs().map(|boot_secs| {
        let (start_rel_secs, start_rel_usecs) = ticks_to_secs_usecs(stat.start_ticks, hz);
        (boot_secs.saturating_add(start_rel_secs), start_rel_usecs)
    });
    let (start_secs, start_usecs) = start_epoch_time.unwrap_or((0, 0));
    attrs.push(Value::cons(
        Value::symbol("start"),
        time_list_from_secs_usecs(start_secs, start_usecs),
    ));
    attrs.push(Value::cons(
        Value::symbol("vsize"),
        Value::fixnum(stat.vsize / 1024),
    ));
    attrs.push(Value::cons(Value::symbol("rss"), Value::fixnum(stat.rss)));
    let elapsed = match (now_epoch_secs_usecs(), start_epoch_time) {
        (Some(now), Some(start)) => nonnegative_time_diff(now, start),
        _ => (0, 0),
    };
    attrs.push(Value::cons(
        Value::symbol("etime"),
        time_list_from_secs_usecs(elapsed.0, elapsed.1),
    ));
    let elapsed_secs = elapsed.0 as f64 + (elapsed.1 as f64 / 1_000_000.0);
    let total_cpu_secs = if hz > 0 {
        (total_ticks as f64) / (hz as f64)
    } else {
        0.0
    };
    let pcpu = if elapsed_secs > 0.0 {
        (total_cpu_secs * 100.0) / elapsed_secs
    } else {
        0.0
    };
    attrs.push(Value::cons(
        Value::symbol("pcpu"),
        Value::make_float(if pcpu.is_finite() { pcpu.max(0.0) } else { 0.0 }),
    ));
    let pmem = sys::total_memory_kb()
        .filter(|mem_total_kb| *mem_total_kb > 0)
        .map(|mem_total_kb| (stat.rss as f64 * 100.0) / mem_total_kb as f64)
        .unwrap_or(0.0);
    attrs.push(Value::cons(
        Value::symbol("pmem"),
        Value::make_float(if pmem.is_finite() { pmem.max(0.0) } else { 0.0 }),
    ));
    attrs.push(Value::cons(
        Value::symbol("args"),
        Value::string(sys::process_cmdline(pid, &stat.comm)),
    ));
    attrs.push(Value::cons(
        Value::symbol("ttname"),
        Value::string(stat.ttname),
    ));

    Ok(Value::list(attrs))
}

/// GNU's `CONS_TO_INTEGER (x, pid_t, pid)` (src/lisp.h:4188-4191) -- the one
/// conversion from a Lisp NUMBER to an OS pid, shared by `Fprocess_attributes`
/// and `internal-default-signal-process` (src/process.c:7375-7376).
///
/// It is `cons_to_signed` over `pid_t`'s FULL signed range, so a NEGATIVE
/// result is in domain: at the POSIX level `kill (-pgid, sig)` signals a
/// process GROUP, and GNU relies on that rather than range-checking the
/// argument.  Non-numbers are the caller's business -- `Fprocess_attributes`
/// raises `numberp` here, while `internal-default-signal-process` sends them
/// to `get_process` instead (:7369-7370) -- so the `numberp` arm below is only
/// reachable from the former.
fn cons_to_os_pid(value: Value) -> Result<i64, Flow> {
    let pid = match value.kind() {
        ValueKind::Fixnum(n) => n,
        ValueKind::Veclike(VecLikeType::Bignum) => i64::try_from(value.as_bignum().unwrap())
            .map_err(|_| signal_process_attributes_pid_range_error())?,
        ValueKind::Float => {
            let f = value.xfloat();
            if !f.is_finite()
                || f.fract() != 0.0
                || f < process_id_min() as f64
                || f > process_id_max() as f64
            {
                return Err(signal_process_attributes_pid_range_error());
            }
            f as i64
        }
        _ => return Err(signal_wrong_type_numberp(value)),
    };
    if pid < process_id_min() || pid > process_id_max() {
        return Err(signal_process_attributes_pid_range_error());
    }
    Ok(pid)
}

fn process_id_min() -> i64 {
    cfg_select! {
        unix => { libc::pid_t::MIN as i64 }
        _ => { i32::MIN as i64 }
    }
}

fn process_id_max() -> i64 {
    cfg_select! {
        unix => { libc::pid_t::MAX as i64 }
        _ => { i32::MAX as i64 }
    }
}

/// (make-process &rest ARGS) -> process-or-nil
pub(crate) fn builtin_make_process(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if !args.is_empty() {
        check_keyword_arg_pairs(&args)?;
        if make_process_keyword_arg(&args, ProcessKeyword::FileHandler).is_truthy() {
            let default_directory = visible_default_directory_lisp(eval);
            if let Some(default_directory) = default_directory {
                let operation = Value::symbol("make-process");
                let handler = super::fileio::find_file_name_handler_lisp_for_eval(
                    eval,
                    &default_directory,
                    operation,
                );
                if !handler.is_nil() {
                    let mut call_args = Vec::with_capacity(args.len() + 1);
                    call_args.push(operation);
                    call_args.extend_from_slice(&args);
                    return eval.funcall_general(handler, call_args);
                }
            }
        }
    }

    let use_pty = process_connection_type_is_pty(&eval.obarray);
    let executable_search = ExecutableSearch::capture(eval);
    let subprocess_cwd = super::callproc::subprocess_default_directory(eval);
    let child_environment = Some(super::environment::ChildEnvironment::materialize(
        eval,
        subprocess_cwd.as_deref(),
    ));
    let coding_environment = make_process_coding_environment(eval, &args)?;
    eval.sync_process_read_config_from_visible_variables();
    // `find-operation-coding-system` can hand back a cons this call just
    // allocated (`Fcons (val, val)` at src/coding.c:10861, or whatever a
    // function-valued alist entry returned), and process creation allocates.
    // The other three inputs are values of live symbols and are reachable
    // through the obarray.
    let roots = eval.save_specpdl_roots();
    eval.push_specpdl_root(coding_environment.operation_coding_system);
    let process = builtin_make_process_impl_with_environment(
        &mut eval.processes,
        &mut eval.buffers,
        &eval.threads,
        args,
        use_pty,
        child_environment,
        Some(executable_search),
        subprocess_cwd,
        Some(&eval.coding_systems),
        coding_environment,
    );
    eval.restore_specpdl_roots(roots);
    process
}

/// Read the ambient half of GNU's `Fmake_process` coding chain, including the
/// one step that can run Lisp.
///
/// `(find-operation-coding-system 'start-process NAME BUFFER COMMAND...)` is
/// asked only when the chain would reach it and only when there is a PROGRAM to
/// match, because `start-process` has `target-idx` 2 (src/coding.c:11784) --
/// the alist is matched against the program, not against the process name
/// (measured: an entry keyed on the name does not fire).  GNU guards the call
/// the same way at src/process.c:1970.
fn make_process_coding_environment(
    eval: &mut super::eval::Context,
    args: &[Value],
) -> Result<MakeProcessCodingEnvironment, Flow> {
    let mut env = MakeProcessCodingEnvironment {
        coding_system_for_read: eval.visible_variable_value_or_nil("coding-system-for-read"),
        coding_system_for_write: eval.visible_variable_value_or_nil("coding-system-for-write"),
        default_process_coding_system: eval
            .visible_variable_value_or_nil("default-process-coding-system"),
        operation_coding_system: Value::NIL,
    };
    let coding = make_process_keyword_arg(args, ProcessKeyword::Coding);
    if !make_process_consults_coding_alist(coding, env)
        || eval
            .visible_variable_value_or_nil("process-coding-system-alist")
            .is_nil()
    {
        return Ok(env);
    }
    let command = make_process_keyword_arg(args, ProcessKeyword::Command);
    if !command.is_cons() {
        return Ok(env);
    }

    let name = make_process_keyword_arg(args, ProcessKeyword::Name);
    let buffer_arg = make_process_keyword_arg(args, ProcessKeyword::Buffer);
    // GNU has already run `Fget_buffer_create` on `:buffer` by this point
    // (src/process.c:1849-1851), so a function-valued alist entry sees the
    // buffer object rather than its name.
    let buffer = if buffer_arg.is_nil() {
        Value::NIL
    } else {
        parse_make_process_buffer(eval, &buffer_arg)?
    };

    let mut operation_args = vec![Value::symbol("start-process"), name, buffer];
    let mut rest = command;
    while rest.is_cons() {
        operation_args.push(rest.cons_car());
        rest = rest.cons_cdr();
    }

    // Root every heap value: a function-valued alist entry runs arbitrary Lisp
    // and can trigger GC.
    let roots = eval.save_specpdl_roots();
    for value in &operation_args {
        eval.push_specpdl_root(*value);
    }
    let result = super::builtins::builtin_find_operation_coding_system(eval, operation_args);
    eval.restore_specpdl_roots(roots);
    env.operation_coding_system = result?;
    Ok(env)
}

/// The first value a `make-process`-shaped keyword list gives for KEYWORD, or
/// nil -- GNU's `plist_get (contact, ...)`, which every one of `Fmake_process`'s
/// reads goes through (src/process.c:1849-1910).
fn make_process_keyword_arg(args: &[Value], keyword: ProcessKeyword) -> Value {
    let mut i = 0usize;
    while i + 1 < args.len() {
        if ProcessKeyword::from_value(&args[i]) == Some(keyword) {
            return args[i + 1];
        }
        i += 2;
    }
    Value::NIL
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_make_process_impl(
    processes: &mut ProcessManager,
    buffers: &mut BufferManager,
    threads: &ThreadManager,
    args: Vec<Value>,
    default_use_pty: bool,
) -> EvalResult {
    builtin_make_process_impl_with_environment(
        processes,
        buffers,
        threads,
        args,
        default_use_pty,
        None,
        None,
        None,
        None,
        MakeProcessCodingEnvironment::unbound(),
    )
}

#[allow(clippy::too_many_arguments)] // process creation keeps host/runtime services independently borrowed
fn builtin_make_process_impl_with_environment(
    processes: &mut ProcessManager,
    buffers: &mut BufferManager,
    threads: &ThreadManager,
    args: Vec<Value>,
    default_use_pty: bool,
    child_environment: Option<super::environment::ChildEnvironment>,
    executable_search: Option<ExecutableSearch>,
    subprocess_cwd: Option<PathBuf>,
    coding_systems: Option<&super::coding::CodingSystemManager>,
    coding_environment: MakeProcessCodingEnvironment,
) -> EvalResult {
    if args.is_empty() {
        return Ok(Value::NIL);
    }
    check_keyword_arg_pairs(&args)?;

    let mut name: Option<LispString> = None;
    let mut buffer: Option<Value> = None;
    let mut command: Option<Vec<LispString>> = None;
    let mut filter = Value::NIL;
    let mut sentinel = Value::NIL;
    let mut connection_type: Option<Value> = None;
    let mut stderr_target = Value::NIL;
    let mut coding_val: Option<Value> = None;
    let mut noquery = false;
    let mut stop_val = Value::NIL;

    let mut seen_keywords = Vec::new();
    let mut i = 0usize;
    while i < args.len() {
        let key = &args[i];
        let value = args.get(i + 1).cloned().unwrap_or(Value::NIL);
        let Some(keyword) = ProcessKeyword::from_value(key) else {
            i += 2;
            continue;
        };
        if process_keyword_already_seen(&mut seen_keywords, keyword) {
            i += 2;
            continue;
        }
        match keyword {
            ProcessKeyword::Name => name = Some(expect_process_name_lisp_string(&value)?),
            ProcessKeyword::Buffer => {
                buffer = Some(parse_make_process_buffer_in_state(buffers, &value)?)
            }
            ProcessKeyword::Command => command = Some(parse_make_process_command(&value)?),
            ProcessKeyword::Filter => filter = value,
            ProcessKeyword::Sentinel => sentinel = value,
            ProcessKeyword::ConnectionType => connection_type = Some(value),
            ProcessKeyword::Stderr => stderr_target = value,
            ProcessKeyword::Coding => coding_val = Some(value),
            ProcessKeyword::Noquery => noquery = value.is_truthy(),
            ProcessKeyword::Stop => stop_val = value,
            _ => {}
        }
        i += 2;
    }

    let Some(name) = name else {
        return Err(signal(
            "error",
            vec![Value::string("Missing :name keyword parameter")],
        ));
    };

    // Determine PTY vs pipe exactly as GNU's `is_pty_from_symbol` does:
    // nil inherits `process-connection-type`; only `pipe` and `pty` are
    // accepted explicit symbols.
    //
    // GNU's `Fmake_process` (src/process.c) decides STDIN/STDOUT's pty-vs-pipe
    // *solely* from `:connection-type` / `process-connection-type` and stores it
    // in `pty_in`/`pty_out`.  Supplying `:stderr` only routes the child's
    // *stderr* to a separate pipe process (`stderrproc`); it does NOT flip
    // stdin/stdout to a pipe.  In `create_process`, the pty is allocated when
    // `pty_in || pty_out`, stdin/stdout use the pty channels, and the stderr
    // pipe is wired through a wholly separate `forkerr` fd.  Hence with the
    // default connection-type (pty) and `:stderr`, GNU reports
    // `(process-tty-name p 'stdout)` => "/dev/pts/N" and `'stderr` => nil.
    //
    // The previous code wrongly forced `use_pty = false` whenever `:stderr` was
    // given, downgrading stdout from a pty to a pipe and diverging from GNU.
    let use_pty =
        resolve_process_connection_type_use_pty(connection_type.as_ref(), default_use_pty)?;

    let command = command.unwrap_or_default();
    if !stop_val.is_nil() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("null"), stop_val],
        ));
    }
    // NOTE: `:coding` is deliberately NOT validated here.  GNU validates the
    // RESOLVED pair, and it does so inside `create_process`, after the program
    // has been found -- measured, `(make-process :coding 'no-such-xyz :command
    // '("no-such-program-xyz"))` signals `file-missing`, not
    // `coding-system-error`.  The check now lives beside the resolver below.
    let (program, argv) = if command.is_empty() {
        (LispString::from_utf8(""), Vec::new())
    } else {
        (command[0].clone(), command[1..].to_vec())
    };
    let executable = if program.is_empty() {
        None
    } else if let Some(search) = executable_search {
        Some(search.resolve(&program, ExecutableLookupMode::MakeProcess)?)
    } else {
        None
    };

    // GNU resolves the pair at src/process.c:1950-2008 and only validates it
    // later, inside `create_process` -> `setup_process_coding_systems` ->
    // `setup_coding_system` (src/process.c:2277, src/coding.c:5678).  That
    // ordering is measurable and this is the point that reproduces it: a
    // program that cannot be found still signals `file-missing` first, an
    // undefined coding system signals `coding-system-error`, and neither leaves
    // a process behind.
    let resolved_coding =
        resolve_make_process_coding_systems(coding_val.unwrap_or(Value::NIL), coding_environment);
    validate_process_coding_component(coding_systems, resolved_coding.decode)?;
    validate_process_coding_component(coding_systems, resolved_coding.encode)?;
    let stderrproc = if stderr_target.is_nil() {
        Value::NIL
    } else if let Some(stderr_id) = process_value_to_id(&stderr_target) {
        // An existing process (object or legacy id) is reused as the stderr
        // pipe; GNU requires it to be a pipe process.
        let stderr_proc = processes
            .get_any(stderr_id)
            .ok_or_else(|| signal_wrong_type_processp(stderr_target))?;
        if stderr_proc.kind != ProcessKind::Pipe {
            return Err(signal(
                "error",
                vec![Value::string("Process is not a pipe process")],
            ));
        }
        Value::make_process(stderr_id)
    } else {
        builtin_make_pipe_process_impl(
            processes,
            buffers,
            threads,
            coding_systems,
            // GNU builds this pipe with `CALLN (Fmake_pipe_process, ...)`
            // (src/process.c:1883), so the stderr pipe runs the PIPE chain --
            // it is not handed `Fmake_process`'s answer.  Measured: a
            // `coding-system-for-read` binding decodes the child's stderr.
            coding_environment.connection_variables(),
            vec![
                ProcessKeyword::Name.value(),
                Value::heap_string(name.concat(&LispString::from_unibyte(b" stderr".to_vec()))),
                ProcessKeyword::Buffer.value(),
                stderr_target,
                ProcessKeyword::Noquery.value(),
                Value::bool_val(noquery),
            ],
        )?
    };
    let id = processes.create_process_lisp_resolved(
        name,
        buffer.unwrap_or(Value::NIL),
        program,
        argv,
        executable,
        resolved_coding,
    );
    processes.sync_process_mark(buffers, id)?;

    // GNU `make_process` (src/process.c) initialises every process's locking
    // thread to the creating thread (`pset_thread (p, Fcurrent_thread ())`),
    // so `process-thread` returns that thread rather than nil.  The network /
    // serial / pipe creators already do this; the subprocess path must too.
    if let Some(proc) = processes.get_mut(id) {
        proc.thread = current_thread_handle(threads);
    }

    // Set filter and sentinel if provided.
    if !filter.is_nil()
        && let Some(proc) = processes.get_mut(id)
    {
        proc.filter = filter;
    }
    if !sentinel.is_nil()
        && let Some(proc) = processes.get_mut(id)
    {
        proc.sentinel = sentinel;
    }
    if !stderrproc.is_nil()
        && let Some(proc) = processes.get_mut(id)
    {
        proc.stderrproc = stderrproc;
    }
    #[cfg(windows)]
    if let Some(stderr_id) = stderrproc.as_process_id()
        && let Some(stderr_proc) = processes.get_mut(stderr_id)
    {
        stderr_proc.stderr_pipe_owner_status_deferred_at = None;
    }
    if let Some(proc) = processes.get_mut(id) {
        proc.default_directory = subprocess_cwd;
        if noquery {
            proc.exit_query_policy = ExitQueryPolicy::NoQuery;
        }
    }

    // The pair resolved above is installed unconditionally: there is no branch
    // in which a real subprocess keeps a coding system nobody resolved.  GNU
    // does NOT run these through `coding_inherit_eol_type` here (unlike
    // `set-process-coding-system`).
    //
    // `coding_explicitly_set` stays a record of whether the CALLER passed
    // `:coding`, because a PTY status heuristic keys off it; the resolver
    // answering for an absent `:coding` must not look like an explicit one.
    if let Some(proc) = processes.get_mut(id) {
        proc.coding_decode = resolved_coding.decode;
        proc.coding_encode = resolved_coding.encode;
        proc.coding_explicitly_set = coding_val.is_some_and(|coding| !coding.is_nil());
    }

    match processes.spawn_child_with_environment(id, use_pty, child_environment) {
        Ok(ChildSpawnOutcome::Spawned) => {}
        // GNU's parent never sees the exec failure: the forked child reports it
        // on its own stderr and exits 127/126 (src/callproc.c:1206-1216), so
        // `make-process` returns a live process object that dies immediately.
        Ok(ChildSpawnOutcome::ExecFailed(errno)) => {
            processes.set_child_status_pending(
                id,
                process_status_exit_value(ChildSpawnOutcome::exec_failure_exit_code(errno)),
            );
        }
        // A launcher failure proper -- no pty could be allocated, the record
        // vanished.  The pipe path still reports a failed exec this way; that
        // asymmetry is measured but not reproduced, see DIVERGENCES.md 174.
        Err(e) => {
            return Err(signal(
                LispCondition::FileMissing,
                vec![Value::string("Doing vfork"), Value::string(e)],
            ));
        }
    }

    Ok(Value::make_process(id))
}

#[derive(Clone, Copy, Debug)]
struct AcceptProcessOutputRequest {
    wait: ProcessOutputWaitRequest,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    target_process: Option<ProcessId>,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    just_this_one: bool,
}

impl AcceptProcessOutputRequest {
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn wait_timing_is_poll(self) -> bool {
        self.wait.timing().is_poll()
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn wait_timing_is_finite(self) -> bool {
        self.wait.timing().is_finite()
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn wait_timing_is_forever(self) -> bool {
        self.wait.timing().is_forever()
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn completes_on_any_process_activity(self) -> bool {
        self.target_process.is_none()
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn completes_on_target_process_activity(self, process: ProcessId) -> bool {
        self.target_process == Some(process)
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn services_only_target_process_output(self) -> bool {
        self.just_this_one
    }
}

fn parse_accept_process_output_request(
    processes: &mut ProcessManager,
    args: &[Value],
) -> Result<Option<AcceptProcessOutputRequest>, Flow> {
    if args.len() > 4 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("accept-process-output"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    if let Some(process) = args.first()
        && !process.is_nil()
        && resolve_live_process_designator_in_manager(processes, process).is_none()
    {
        if is_stale_process_id_designator_in_manager(processes, process) {
            return Ok(None);
        }
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), *process],
        ));
    }

    if let Some(seconds) = args.get(1) {
        if let Some(milliseconds) = args.get(2) {
            if !milliseconds.is_nil() && !milliseconds.is_fixnum() {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("fixnump"), *milliseconds],
                ));
            }
            if milliseconds.is_nil() {
                if !seconds.is_nil() && !seconds.is_number() {
                    return Err(signal(
                        LispCondition::WrongTypeArgument,
                        vec![Value::symbol("numberp"), *seconds],
                    ));
                }
            } else if !seconds.is_nil() && !seconds.is_fixnum() {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("fixnump"), *seconds],
                ));
            }
        } else if !seconds.is_nil() && !seconds.is_number() {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("numberp"), *seconds],
            ));
        }
    }

    let target_id = if let Some(process) = args.first() {
        if !process.is_nil() {
            resolve_live_process_designator_in_manager(processes, process)
        } else {
            None
        }
    } else {
        None
    };

    let just_this_one = target_id.is_some() && args.get(3).is_some_and(|value| value.is_truthy());
    let allow_timers = if target_id.is_some() {
        !args.get(3).is_some_and(|v| v.is_fixnum())
    } else {
        true
    };
    let milliseconds_supplied = args.get(2).is_some_and(|value| !value.is_nil());
    let positive_timeout = accept_process_output_positive_timeout(args);
    let timing = if let Some(timeout) = positive_timeout {
        ProcessOutputWaitTiming::For(timeout)
    } else if target_id.is_some()
        && !milliseconds_supplied
        && args.get(1).is_none_or(|value| value.is_nil())
    {
        ProcessOutputWaitTiming::Forever
    } else {
        ProcessOutputWaitTiming::Poll
    };
    Ok(Some(AcceptProcessOutputRequest {
        wait: ProcessOutputWaitRequest::new(timing, target_id, just_this_one, allow_timers),
        target_process: target_id,
        just_this_one,
    }))
}

fn accept_process_output_positive_timeout(args: &[Value]) -> Option<Duration> {
    let total_seconds = if let Some(milliseconds) = args.get(2).filter(|value| !value.is_nil()) {
        let milliseconds = milliseconds.as_fixnum().unwrap_or(0) as f64 / 1000.0;
        let seconds = args
            .get(1)
            .filter(|value| !value.is_nil())
            .and_then(|value| value.as_fixnum())
            .unwrap_or(0) as f64;
        seconds + milliseconds
    } else if let Some(seconds) = args.get(1).filter(|value| !value.is_nil()) {
        seconds
            .as_fixnum()
            .map(|value| value as f64)
            .or_else(|| seconds.as_float())
            .unwrap_or(0.0)
    } else {
        return None;
    };

    (total_seconds > 0.0).then(|| Duration::from_secs_f64(total_seconds))
}

/// (process-send-string PROCESS STRING) -> nil
pub(crate) fn builtin_process_send_string(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-send-string", &args, 2)?;
    let input = args[1]
        .as_lisp_string()
        .cloned()
        .ok_or_else(|| signal_wrong_type_string(args[1]))?;
    if let Some(id) = process_value_to_id(&args[0])
        && is_stale_process_id_designator_in_manager(&eval.processes, &args[0])
    {
        return Err(signal_process_not_running_in_manager(&eval.processes, id));
    }
    let id = resolve_get_process_designator_in_state(&eval.processes, &eval.buffers, &args[0])?;
    eval.wait_while_network_process_connecting(id)?;
    // GNU `send_process` runs `update_status` (:6725-6726) before it tests
    // `p->status`, and by then `handle_child_signal` has already recorded a
    // child that exited.  This port makes the recording here.
    if eval
        .processes
        .observe(UpdateStatusSite::SendProcess, id)
        .is_some_and(|observed| !observed.allows_send())
    {
        return Err(signal_process_not_running_in_manager(&eval.processes, id));
    }
    let encoded = encode_process_send_input(&eval.processes, id, &input, eval.eol_conversion());
    eval.send_process_input_reentrant(id, &encoded)?;
    Ok(Value::NIL)
}

/// The `ProcessManager`-only spelling of `process-send-string`, for unit
/// fixtures that have no `Context`.
///
/// `#[cfg(test)]` since entry 143: with no `Context` there is no
/// `inhibit-eol-conversion' to read, so it names `EolConversion::Enabled`
/// below.  A production caller must not be able to inherit that assumption by
/// reaching for the shorter spelling.
#[cfg(test)]
pub(crate) fn builtin_process_send_string_impl(
    processes: &mut ProcessManager,
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-send-string", &args, 2)?;
    let input = args[1]
        .as_lisp_string()
        .cloned()
        .ok_or_else(|| signal_wrong_type_string(args[1]))?;
    if let Some(id) = process_value_to_id(&args[0])
        && is_stale_process_id_designator_in_manager(processes, &args[0])
    {
        return Err(signal_process_not_running_in_manager(processes, id));
    }
    let id = resolve_get_process_designator_in_state(processes, buffers, &args[0])?;
    if processes
        .observe(UpdateStatusSite::SendProcess, id)
        .is_some_and(|observed| !observed.allows_send())
    {
        return Err(signal_process_not_running_in_manager(processes, id));
    }
    // GNU `send_process` (src/process.c) encodes the data through the process's
    // ENCODE coding system before writing it to the child's fd, applying both
    // character-code and EOL conversion.  Encode the input here so a process
    // whose output coding requests CRLF/CR (e.g. `dos`/`mac`/`utf-8-dos`) sends
    // the converted bytes; binary/raw-text encode systems pass the bytes
    // through unchanged.
    let encoded = encode_process_send_input(
        processes,
        id,
        &input,
        crate::emacs_core::coding::EolConversion::Enabled,
    );
    if !processes.send_input(id, &encoded)? {
        return Err(signal("error", vec![Value::string("Process not found")]));
    }
    Ok(Value::NIL)
}

/// (process-status PROCESS) -> symbol
pub(crate) fn builtin_process_status(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_process_status_impl(&mut eval.processes, &eval.buffers, args)
}

pub(crate) fn builtin_process_status_impl(
    processes: &mut ProcessManager,
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-status", &args, 1)?;
    let Some(id) = resolve_process_for_status_in_state(processes, buffers, &args[0])? else {
        return Ok(Value::NIL);
    };
    // GNU `Fprocess_status` runs `update_status` when `raw_status_new` is set
    // (src/process.c:1188-1189), and `raw_status_new` is already set by then
    // because `handle_child_signal` recorded it from the SIGCHLD handler
    // (:7746-7747).  This port cannot record from a handler, so `observe`
    // makes GNU's recording here instead -- see `process/child_status.rs`.
    // `process-live-p` rides on this one: lisp/subr.el:3538-3540 defines it
    // as `(memq (process-status process) '(run open listen connect stop))`.
    match processes.observe(UpdateStatusSite::ProcessStatus, id) {
        Some(observed) => Ok(observed.public_status_symbol()),
        None => Ok(Value::NIL),
    }
}

/// (process-exit-status PROCESS) -> integer
pub(crate) fn builtin_process_exit_status(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_process_exit_status_impl(&mut eval.processes, args)
}

pub(crate) fn builtin_process_exit_status_impl(
    processes: &mut ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-exit-status", &args, 1)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    // GNU `Fprocess_exit_status` runs `update_status` too (src/process.c:
    // 1212-1213), on a record `handle_child_signal` has already made.  Same
    // recording as `builtin_process_status_impl`; see `child_status.rs`.
    let observed = processes
        .observe(UpdateStatusSite::ProcessExitStatus, id)
        .ok_or_else(|| signal_wrong_type_processp(args[0]))?;
    let status = observed.settled_status();
    let proc = observed.process();
    match ProcessStatusSymbol::from_status_value(status) {
        Some(ProcessStatusSymbol::Exit) => Ok(Value::fixnum(process_status_code_value(status))),
        Some(ProcessStatusSymbol::Failed) => Ok(Value::fixnum(process_status_code_value(status))),
        Some(ProcessStatusSymbol::Signal) => {
            if proc.kind == ProcessKind::Real {
                Ok(Value::fixnum(process_status_code_value(status)))
            } else {
                Ok(Value::fixnum(0))
            }
        }
        Some(ProcessStatusSymbol::Stop) => {
            if proc.kind == ProcessKind::Real {
                Ok(Value::fixnum(process_status_code_value(status)))
            } else {
                Ok(Value::fixnum(0))
            }
        }
        _ => Ok(Value::fixnum(0)),
    }
}

/// (process-list) -> list of process ids
pub(crate) fn builtin_process_list(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_process_list_impl(&eval.processes, args)
}

pub(crate) fn builtin_process_list_impl(
    processes: &ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-list", &args, 0)?;
    let ids = processes.list_processes();
    let values: Vec<Value> = ids.iter().map(|id| Value::make_process(*id)).collect();
    Ok(Value::list(values))
}

/// (process-name PROCESS) -> string
pub(crate) fn builtin_process_name(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_process_name_impl(&eval.processes, args)
}

pub(crate) fn builtin_process_name_impl(
    processes: &ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-name", &args, 1)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    match processes.get_any(id) {
        Some(proc) => Ok(proc.name),
        None => Err(signal_wrong_type_processp(args[0])),
    }
}

/// (process-buffer PROCESS) -> buffer or nil
pub(crate) fn builtin_process_buffer(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_process_buffer_impl(&eval.processes, args)
}

pub(crate) fn builtin_process_buffer_impl(
    processes: &ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-buffer", &args, 1)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    match processes.get_any(id) {
        Some(proc) => Ok(proc.buffer),
        None => Err(signal_wrong_type_processp(args[0])),
    }
}

/// (process-coding-system PROCESS) -> (decode . encode)
pub(crate) fn builtin_process_coding_system(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_process_coding_system_impl(&eval.processes, args)
}

pub(crate) fn builtin_process_coding_system_impl(
    processes: &ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-coding-system", &args, 1)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    let proc = processes.get_any(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    Ok(Value::cons(proc.coding_decode, proc.coding_encode))
}

/// (process-datagram-address PROCESS) -> address-or-nil
pub(crate) fn builtin_process_datagram_address(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() == 1
        && let Some(id) = pending_network_connect_id(&eval.processes, args[0])?
    {
        eval.wait_while_network_process_connecting(id)?;
    }
    builtin_process_datagram_address_impl(&eval.processes, args)
}

pub(crate) fn builtin_process_datagram_address_impl(
    processes: &ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-datagram-address", &args, 1)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    let Some(proc) = processes.get_any(id) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        ));
    };
    if process_has_datagram_semantics(proc) {
        Ok(proc.datagram_address)
    } else {
        Ok(Value::NIL)
    }
}

/// (process-inherit-coding-system-flag PROCESS) -> bool
pub(crate) fn builtin_process_inherit_coding_system_flag(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_process_inherit_coding_system_flag_impl(&eval.processes, args)
}

pub(crate) fn builtin_process_inherit_coding_system_flag_impl(
    processes: &ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-inherit-coding-system-flag", &args, 1)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    let proc = processes.get_any(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    Ok(Value::bool_val(proc.inherit_coding_system_flag))
}

/// (set-process-buffer PROCESS BUFFER) -> BUFFER
pub(crate) fn builtin_set_process_buffer(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_set_process_buffer_impl(&mut eval.processes, &mut eval.buffers, args)
}

pub(crate) fn builtin_set_process_buffer_impl(
    processes: &mut ProcessManager,
    buffers: &mut BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-process-buffer", &args, 2)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    // `if (!NILP (buffer)) CHECK_BUFFER (buffer);` (src/process.c:1302-1303),
    // and `CHECK_BUFFER` is `CHECK_TYPE (BUFFERP (x), Qbufferp, x)` and
    // nothing else (src/buffer.h:762-766).  A DEAD buffer is a buffer, and
    // handing one over is how a process legitimately reaches the state
    // `read_and_insert_process_output` (:6464),
    // `internal-default-process-sentinel` (:7969-7971) and
    // `setup_process_coding_systems` (:8395) each guard against.
    match args[1].kind() {
        ValueKind::Nil | ValueKind::Veclike(VecLikeType::Buffer) => {}
        _ => return Err(signal_wrong_type_bufferp(args[1])),
    };
    let proc = processes.get_any_mut(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    if proc.buffer != args[1] {
        proc.buffer = args[1];
        update_process_mark(buffers, proc)?;
    }
    if process_uses_contact_plist(proc) {
        proc.childp =
            process_contact_plist_put(proc.childp, ProcessKeyword::Buffer.value(), args[1])?;
    }
    Ok(args[1])
}

/// (set-process-coding-system PROCESS &optional DECODING ENCODING) -> nil
pub(crate) fn builtin_set_process_coding_system(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_set_process_coding_system_impl(&mut eval.processes, &eval.coding_systems, args)
}

pub(crate) fn builtin_set_process_coding_system_impl(
    processes: &mut ProcessManager,
    coding_systems: &super::coding::CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("set-process-coding-system", &args, 1)?;
    if args.len() > 3 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("set-process-coding-system"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    // GNU `Fset_process_coding_system` (src/process.c): CHECK_PROCESS first,
    // then DECODING and ENCODING (both defaulting to nil) are validated, then
    // ENCODING (only) is passed through `coding_inherit_eol_type` so a
    // nil/undecided-EOL encode coding normalizes (e.g. nil -> raw-text-unix,
    // utf-8 -> utf-8-unix). DECODING is stored as-is (nil stays nil).
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    let decoding = args.get(1).cloned().unwrap_or(Value::NIL);
    let encoding = args.get(2).cloned().unwrap_or(Value::NIL);
    super::coding::builtin_check_coding_system(coding_systems, vec![decoding])?;
    super::coding::builtin_check_coding_system(coding_systems, vec![encoding])?;
    let encoding = super::coding::coding_inherit_eol_type_unix(coding_systems, encoding);

    let proc = processes.get_any_mut(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    proc.coding_decode = decoding;
    // GNU `set-process-coding-system` ends in `setup_process_coding_systems`
    // (src/process.c:8036), which re-runs `setup_coding_system` -- and that
    // zeroes both `coding->mode` (:5683, so the `CODING_MODE_LAST_BLOCK` latch
    // goes down) and `coding->carryover_bytes` (:5703).
    proc.coding_state.reset();
    proc.coding_encode = encoding;
    proc.coding_explicitly_set = true;
    Ok(Value::NIL)
}

/// (set-process-datagram-address PROCESS ADDRESS) -> nil
pub(crate) fn builtin_set_process_datagram_address(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() == 2
        && let Some(id) = pending_network_connect_id(&eval.processes, args[0])?
    {
        eval.wait_while_network_process_connecting(id)?;
    }
    builtin_set_process_datagram_address_impl(&mut eval.processes, args)
}

pub(crate) fn builtin_set_process_datagram_address_impl(
    processes: &mut ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-process-datagram-address", &args, 2)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    let Some(proc) = processes.get_any_mut(id) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        ));
    };
    match proc.live_io.network_socket.as_ref() {
        Some(NetworkSocket::UdpSocket(socket)) => {
            let Ok(NetworkAddressSpec::Inet(addr)) = parse_network_address_spec(&args[1]) else {
                return Ok(Value::NIL);
            };
            let family_matches = socket
                .local_addr()
                .ok()
                .map(|local_addr| local_addr.is_ipv4() == addr.is_ipv4())
                .or_else(|| {
                    proc.datagram_socket_addr
                        .map(|remote_addr| remote_addr.is_ipv4() == addr.is_ipv4())
                })
                .unwrap_or(true);
            if !family_matches {
                return Ok(Value::NIL);
            }
            proc.datagram_socket_addr = Some(addr);
            proc.datagram_address = socket_addr_to_lisp_value(addr);
            Ok(args[1])
        }
        #[cfg(unix)]
        Some(NetworkSocket::UnixDatagram(_)) => {
            let Ok(NetworkAddressSpec::Local(path)) = parse_network_address_spec(&args[1]) else {
                return Ok(Value::NIL);
            };
            proc.datagram_unix_path = Some(path);
            proc.datagram_address = args[1];
            Ok(args[1])
        }
        _ => Ok(Value::NIL),
    }
}

/// (set-process-inherit-coding-system-flag PROCESS FLAG) -> FLAG
pub(crate) fn builtin_set_process_inherit_coding_system_flag(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_set_process_inherit_coding_system_flag_impl(&mut eval.processes, args)
}

pub(crate) fn builtin_set_process_inherit_coding_system_flag_impl(
    processes: &mut ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-process-inherit-coding-system-flag", &args, 2)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    let proc = processes.get_any_mut(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    proc.inherit_coding_system_flag = args[1].is_truthy();
    Ok(args[1])
}

/// (set-process-thread PROCESS THREAD) -> thread-or-nil
pub(crate) fn builtin_set_process_thread(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_set_process_thread_impl(&mut eval.processes, &eval.threads, args)
}

pub(crate) fn builtin_set_process_thread_impl(
    processes: &mut ProcessManager,
    threads: &ThreadManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-process-thread", &args, 2)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    let value = if args[1].is_nil() {
        Value::NIL
    } else if threads.thread_id_from_handle(&args[1]).is_some() {
        args[1]
    } else {
        return Err(signal_wrong_type_threadp(args[1]));
    };
    let proc = processes.get_any_mut(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    proc.thread = value;
    Ok(value)
}

/// (set-process-window-size PROCESS HEIGHT WIDTH) -> t-or-nil
pub(crate) fn builtin_set_process_window_size(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_set_process_window_size_impl(&mut eval.processes, args)
}

pub(crate) fn builtin_set_process_window_size_impl(
    processes: &mut ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-process-window-size", &args, 3)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    let height = expect_ushort_dimension(&args[1])?;
    let width = expect_ushort_dimension(&args[2])?;
    let is_live = processes.get(id).is_some();
    let proc = processes.get_any_mut(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    proc.window_rows = Some(i64::from(height));
    proc.window_cols = Some(i64::from(width));
    if !is_live {
        return Ok(Value::NIL);
    }
    if let Some(ref pty_master) = proc.live_io.pty_master {
        let pty_size = portable_pty::PtySize {
            rows: height,
            cols: width,
            pixel_width: 0,
            pixel_height: 0,
        };
        return Ok(Value::bool_val(pty_master.resize(pty_size).is_ok()));
    }
    if proc.kind == ProcessKind::Real && !process_has_subprocess_backing(proc) {
        return Ok(Value::T);
    }
    Ok(Value::NIL)
}

/// (process-menu-visit-buffer LINE) -> nil
/// (process-tty-name PROCESS &optional STREAM) -> string-or-nil
pub(crate) fn builtin_process_tty_name(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_process_tty_name_impl(&eval.processes, args)
}

pub(crate) fn builtin_process_tty_name_impl(
    processes: &ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("process-tty-name", &args, 1)?;
    if args.len() > 2 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("process-tty-name"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    let proc = processes.get_any(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    let stream = args.get(1).cloned().unwrap_or(Value::NIL);
    let tty_value = || proc.tty_name;

    match ProcessTtyStream::from_value(&stream) {
        None if stream.is_nil() => Ok(tty_value()),
        Some(ProcessTtyStream::Stdin) => {
            if proc.tty_stdin {
                Ok(tty_value())
            } else {
                Ok(Value::NIL)
            }
        }
        Some(ProcessTtyStream::Stdout) => {
            if proc.tty_stdout {
                Ok(tty_value())
            } else {
                Ok(Value::NIL)
            }
        }
        Some(ProcessTtyStream::Stderr) => {
            if proc.tty_stderr && proc.stderrproc.is_nil() {
                Ok(tty_value())
            } else {
                Ok(Value::NIL)
            }
        }
        None => Err(signal(
            "error",
            vec![Value::string("Unknown stream"), stream],
        )),
    }
}

/// (process-mark PROCESS) -> marker
pub(crate) fn builtin_process_mark(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_process_mark_impl(&eval.processes, &eval.buffers, args)
}

pub(crate) fn builtin_process_mark_impl(
    processes: &ProcessManager,
    _buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-mark", &args, 1)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    let proc = processes.get_any(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    Ok(proc.mark)
}

/// (process-type PROCESS) -> symbol
pub(crate) fn builtin_process_type(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_process_type_impl(&eval.processes, &eval.buffers, args)
}

pub(crate) fn builtin_process_type_impl(
    processes: &ProcessManager,
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-type", &args, 1)?;
    let id = resolve_get_process_designator_in_state(processes, buffers, &args[0])?;
    let proc = processes.get_any(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    Ok(proc.proc_type)
}

/// (process-thread PROCESS) -> object-or-nil
pub(crate) fn builtin_process_thread(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_process_thread_impl(&eval.processes, args)
}

pub(crate) fn builtin_process_thread_impl(
    processes: &ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-thread", &args, 1)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    let proc = processes.get_any(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    Ok(proc.thread)
}

/// (process-send-region PROCESS START END) -> nil
pub(crate) fn builtin_process_send_region(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-send-region", &args, 3)?;

    if let Some(id) = process_value_to_id(&args[0])
        && is_stale_process_id_designator_in_manager(&eval.processes, &args[0])
    {
        let _ = super::position::LispRegionArgs::from_values(&eval.buffers, args[1], args[2])?;
        return Err(signal_process_not_running_in_manager(&eval.processes, id));
    }

    let id = resolve_get_process_designator_in_state(&eval.processes, &eval.buffers, &args[0])?;
    eval.wait_while_network_process_connecting(id)?;
    // `process-send-region` reaches GNU's `send_process` too, and therefore
    // the same `update_status` at :6726.
    if eval
        .processes
        .observe(UpdateStatusSite::SendProcess, id)
        .is_some_and(|observed| !observed.allows_send())
    {
        return Err(signal_process_not_running_in_manager(&eval.processes, id));
    }
    let region_args =
        super::position::LispRegionArgs::from_values(&eval.buffers, args[1], args[2])?;

    let region_text = {
        let buf = eval
            .buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let region = checked_region_bytes(buf, region_args)?;
        buf.buffer_substring_lisp_string_range(region)
    };

    let encoded =
        encode_process_send_input(&eval.processes, id, &region_text, eval.eol_conversion());
    eval.send_process_input_reentrant(id, &encoded)?;
    Ok(Value::NIL)
}

/// The `ProcessManager`-only spelling of `process-send-region`; `#[cfg(test)]`
/// for the reason [`builtin_process_send_string_impl`] gives.
#[cfg(test)]
pub(crate) fn builtin_process_send_region_impl(
    processes: &mut ProcessManager,
    buffers: &mut BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-send-region", &args, 3)?;

    if let Some(id) = process_value_to_id(&args[0])
        && is_stale_process_id_designator_in_manager(processes, &args[0])
    {
        let _ = super::position::LispRegionArgs::from_values(&*buffers, args[1], args[2])?;
        return Err(signal_process_not_running_in_manager(processes, id));
    }

    let id = resolve_get_process_designator_in_state(processes, buffers, &args[0])?;
    if processes
        .observe(UpdateStatusSite::SendProcess, id)
        .is_some_and(|observed| !observed.allows_send())
    {
        return Err(signal_process_not_running_in_manager(processes, id));
    }
    let region_args = super::position::LispRegionArgs::from_values(&*buffers, args[1], args[2])?;

    let region_text = {
        let buf = buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let region = checked_region_bytes(buf, region_args)?;
        buf.buffer_substring_lisp_string_range(region)
    };

    // Encode the region text through the process's ENCODE coding system, exactly
    // like `process-send-string` (GNU `send_process`).
    let encoded = encode_process_send_input(
        processes,
        id,
        &region_text,
        crate::emacs_core::coding::EolConversion::Enabled,
    );
    if !processes.send_input(id, &encoded)? {
        return Err(signal("error", vec![Value::string("Process not found")]));
    }
    Ok(Value::NIL)
}

/// (process-send-eof &optional PROCESS) -> process-or-nil
pub(crate) fn builtin_process_send_eof(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() <= 1 {
        let maybe_id = args
            .first()
            .map(|process| {
                resolve_get_process_designator_in_state(&eval.processes, &eval.buffers, process)
            })
            .unwrap_or_else(|| {
                resolve_optional_process_or_current_buffer_in_state(
                    &eval.processes,
                    &eval.buffers,
                    None,
                )
            })
            .ok();
        if let Some(id) = maybe_id
            && eval.processes.get(id).is_some_and(|proc| {
                proc.kind == ProcessKind::Network && proc.live_io.pending_network_connect.is_some()
            })
        {
            eval.wait_while_network_process_connecting(id)?;
        }
        if let Some(id) = maybe_id
            && eval.processes.get(id).is_some_and(|proc| proc.tty_stdin)
        {
            if eval
                .processes
                .observe(UpdateStatusSite::ProcessSendEof, id)
                .is_some_and(|observed| !observed.allows_send())
            {
                return Err(signal_process_not_running_in_manager(&eval.processes, id));
            }
            if let Some(proc) = eval.processes.get_mut(id) {
                proc.eof_sent_to_process = true;
            }
            // GNU process.c sends an unencoded EOT byte through `send_process'
            // when the child's input is a PTY.  Route it through the same
            // reentrant write queue as ordinary input so EOF is ordered after
            // every byte already accepted by `process-send-string'.
            eval.send_process_input_reentrant(id, &LispString::from_unibyte(vec![0x04]))?;
            return Ok(args.first().copied().unwrap_or(Value::NIL));
        }
    }
    builtin_process_send_eof_impl(&mut eval.processes, &eval.buffers, args)
}

fn send_eof_to_process(proc: &mut Process) -> EvalResult {
    // GNU returns a datagram process untouched before both the liveness gate
    // and every EOF state mutation (src/process.c:7444-7445).
    if process_has_datagram_semantics(proc) {
        return Ok(Value::NIL);
    }
    proc.eof_sent_to_process = true;

    // GNU's serial branch only drains the device; it neither closes the
    // writer nor replaces it with /dev/null (src/process.c:7470-7478).
    if proc.kind == ProcessKind::Serial {
        return Ok(Value::NIL);
    }

    // EPIPE already changed GNU's `outfd` to -1. `process-send-eof` still
    // enters the non-PTY/non-serial branch and installs /dev/null, but it does
    // not try to shut down the old descriptor because it is negative. Mirror
    // that semantic state transition without touching the retained readable
    // owner of a bidirectional Rust transport.
    if proc.input_disposition == ProcessInputDisposition::Disconnected {
        proc.input_disposition = ProcessInputDisposition::Discard;
        return Ok(Value::NIL);
    }

    if let Some(tls) = proc.live_io.tls_stream.as_mut() {
        tls.send_close_notify(false)
            .map(|_| ())
            .map_err(|err| signal_process_io("Sending EOF to process", None, err))?;
        proc.input_disposition = ProcessInputDisposition::Discard;
        return Ok(Value::NIL);
    }

    if let Some(socket) = proc.live_io.network_socket.as_ref() {
        if let Some(result) = socket.shutdown_write() {
            result.map_err(|err| signal_process_io("Sending EOF to process", None, err))?;
            proc.input_disposition = ProcessInputDisposition::Discard;
        }
        return Ok(Value::NIL);
    }

    let _ = proc.live_io.child.close_stdin();
    // GNU opens the null output device even when there was no old descriptor
    // to close. `Discard` models that sink without allocating an OS fd.
    proc.input_disposition = ProcessInputDisposition::Discard;
    Ok(Value::NIL)
}

pub(crate) fn builtin_process_send_eof_impl(
    processes: &mut ProcessManager,
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() > 1 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("process-send-eof"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    if let Some(process) = args.first()
        && !process.is_nil()
    {
        if let Some(id) = process_value_to_id(process)
            && is_stale_process_id_designator_in_manager(processes, process)
        {
            return Err(signal_process_not_running_in_manager(processes, id));
        }
        let id = resolve_get_process_designator_in_state(processes, buffers, process)?;
        process_send_eof_liveness_gate(processes, id)?;
        if let Some(proc) = processes.get_mut(id) {
            send_eof_to_process(proc)?;
        }
        return Ok(*process);
    }
    let id = resolve_optional_process_or_current_buffer_in_state(processes, buffers, args.first())?;
    process_send_eof_liveness_gate(processes, id)?;
    if let Some(proc) = processes.get_mut(id) {
        send_eof_to_process(proc)?;
    }
    Ok(Value::NIL)
}

/// GNU `Fprocess_send_eof`'s "Make sure the process is really alive" gate
/// (src/process.c:7451-7455), which this port did not have on the non-pty
/// path at all -- `process-send-eof` answered `ok` for a child that had
/// exited where GNU raises `Process NAME not running: finished`.
///
/// The datagram exemption above it is GNU's own: `Fprocess_send_eof` returns
/// the process untouched for a datagram connection at :7444-7445, BEFORE the
/// gate, so a datagram is never rejected by it.
fn process_send_eof_liveness_gate(
    processes: &mut ProcessManager,
    id: ProcessId,
) -> Result<(), Flow> {
    if processes
        .get(id)
        .is_some_and(process_has_datagram_semantics)
    {
        return Ok(());
    }
    if processes
        .observe(UpdateStatusSite::ProcessSendEof, id)
        .is_some_and(|observed| !observed.allows_send())
    {
        return Err(signal_process_not_running_in_manager(processes, id));
    }
    Ok(())
}

/// (process-running-child-p &optional PROCESS) -> bool
pub(crate) fn builtin_process_running_child_p(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_process_running_child_p_impl(&eval.processes, &eval.buffers, args)
}

#[cfg(unix)]
fn process_tty_foreground_group(proc: &Process) -> Option<i32> {
    if let Some(gid) = proc
        .live_io
        .pty_master
        .as_ref()
        .and_then(|master| master.as_raw_fd())
        .and_then(sys::fd_foreground_pgrp)
    {
        return Some(gid);
    }

    let tty_name = proc.tty_name.as_lisp_string()?;
    if tty_name.as_bytes().is_empty() {
        return None;
    }
    let tty_path = lisp_string_to_os_string(tty_name);
    sys::tty_path_foreground_pgrp(tty_path.as_os_str())
}

#[cfg(not(unix))]
fn process_tty_foreground_group(_proc: &Process) -> Option<i32> {
    None
}

fn process_running_child_value(proc: &Process) -> Value {
    if !process_has_subprocess_backing(proc) {
        return Value::NIL;
    }
    if let Some(gid) = process_tty_foreground_group(proc) {
        if proc.os_pid.is_some_and(|pid| pid as i64 == gid as i64) {
            Value::NIL
        } else {
            Value::fixnum(gid as i64)
        }
    } else {
        Value::T
    }
}

pub(crate) fn builtin_process_running_child_p_impl(
    processes: &ProcessManager,
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() > 1 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("process-running-child-p"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    // GNU checks `!EQ (p->type, Qreal)` at src/process.c:7042-7044 BEFORE
    // `p->infd < 0` at :7045-7047, so a pipe answers "is not a subprocess"
    // however long it has been dead.
    if let Some(process) = args.first()
        && let Some(id) = process_value_to_id(process)
        && is_stale_real_process_designator_in_manager(processes, process)
    {
        return Err(signal_process_not_active_in_manager(processes, id));
    }
    let id = resolve_optional_process_or_current_buffer_in_state(processes, buffers, args.first())?;
    let proc = processes
        .get_any(id)
        .ok_or_else(|| signal_process_not_active_in_manager(processes, id))?;
    if proc.kind != ProcessKind::Real {
        return Err(signal_process_not_subprocess(proc));
    }
    if processes.get(id).is_none() {
        return Err(signal_process_not_active_in_manager(processes, id));
    }
    Ok(process_running_child_value(proc))
}

/// (accept-process-output &optional PROCESS SECONDS MILLISECS JUST-THIS-ONE) -> bool
pub(crate) fn builtin_accept_process_output(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let Some(request) = parse_accept_process_output_request(&mut eval.processes, &args)? else {
        return Ok(Value::NIL);
    };

    match eval.wait_for_process_output(request.wait)? {
        ProcessOutputWaitOutcome::ProcessActivity => Ok(Value::T),
        ProcessOutputWaitOutcome::NoProcessActivity => Ok(Value::NIL),
    }
}

/// (get-process NAME) -> process-or-nil
pub(crate) fn builtin_get_process(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    builtin_get_process_impl(&eval.processes, args)
}

pub(crate) fn builtin_get_process_impl(processes: &ProcessManager, args: Vec<Value>) -> EvalResult {
    expect_args("get-process", &args, 1)?;
    // GNU `Fget_process`: a process object is returned unchanged; otherwise the
    // argument must be a name string.
    if args[0].is_process() {
        return Ok(args[0]);
    }
    let name = expect_string_strict(&args[0])?;
    match processes.find_by_name(&name) {
        Some(id) => Ok(Value::make_process(id)),
        None => Ok(Value::NIL),
    }
}

/// (get-buffer-process BUFFER-OR-NAME) -> process-or-nil
pub(crate) fn builtin_get_buffer_process(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_get_buffer_process_impl(&eval.buffers, &eval.processes, args)
}

pub(crate) fn builtin_get_buffer_process_impl(
    buffers: &BufferManager,
    processes: &ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("get-buffer-process", &args, 1)?;
    let Some(buffer_id) = resolve_buffer_for_process_lookup_in_state(buffers, &args[0])? else {
        return Ok(Value::NIL);
    };
    match processes.find_by_buffer_id(buffer_id) {
        Some(id) => Ok(Value::make_process(id)),
        None => Ok(Value::NIL),
    }
}

/// (processp OBJECT) -> bool
pub(crate) fn builtin_processp(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    builtin_processp_impl(&eval.processes, args)
}

pub(crate) fn builtin_processp_impl(_processes: &ProcessManager, args: Vec<Value>) -> EvalResult {
    expect_args("processp", &args, 1)?;
    // GNU `Fprocessp` is purely structural: any process object is `t`, even
    // after it has exited (it stays a process object).  A bare integer is not a
    // process.
    Ok(Value::bool_val(args[0].is_process()))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_process_live_p_impl(
    processes: &mut ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-live-p", &args, 1)?;
    let Some(id) = process_value_to_id(&args[0]).and_then(|id| processes.get(id).map(|_| id))
    else {
        return Ok(Value::NIL);
    };
    // Keep this a recorded-state query, like GNU's process predicates.  Wait
    // paths observe child exits and update `status`; `process-live-p` must not
    // perform a fresh no-wait child probe on its own.
    let proc = processes.get(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    Ok(process_live_status_value(proc))
}

/// (process-id PROCESS) -> integer
pub(crate) fn builtin_process_id(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    builtin_process_id_impl(&eval.processes, args)
}

pub(crate) fn builtin_process_id_impl(processes: &ProcessManager, args: Vec<Value>) -> EvalResult {
    expect_args("process-id", &args, 1)?;
    // GNU `Fprocess_id` uses CHECK_PROCESS — it requires a genuine process
    // object (no name-string designator), so resolve structurally only.
    let id = process_value_to_id(&args[0])
        .filter(|id| processes.get_any(*id).is_some())
        .ok_or_else(|| signal_wrong_type_processp(args[0]))?;
    let proc = processes
        .get_any(id)
        .ok_or_else(|| signal_wrong_type_processp(args[0]))?;
    // GNU `Fprocess_id` returns the child's real OS pid as an integer
    // (`XPROCESS (process)->pid`), or nil when there is none (pid == 0), as
    // for network/serial/pipe connections.  The internal `ProcessId` used to
    // key the manager is kept separate and never exposed here.
    match proc.os_pid {
        Some(pid) => Ok(Value::fixnum(i64::from(pid))),
        None => Ok(Value::NIL),
    }
}

/// (process-query-on-exit-flag PROCESS) -> bool
pub(crate) fn builtin_process_query_on_exit_flag(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_process_query_on_exit_flag_impl(&eval.processes, args)
}

pub(crate) fn builtin_process_query_on_exit_flag_impl(
    processes: &ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-query-on-exit-flag", &args, 1)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    let proc = processes.get_any(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    Ok(Value::bool_val(proc.exit_query_policy.queries_on_exit()))
}

/// (set-process-query-on-exit-flag PROCESS FLAG) -> FLAG
pub(crate) fn builtin_set_process_query_on_exit_flag(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_set_process_query_on_exit_flag_impl(&mut eval.processes, args)
}

pub(crate) fn builtin_set_process_query_on_exit_flag_impl(
    processes: &mut ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-process-query-on-exit-flag", &args, 2)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    let policy = ExitQueryPolicy::from_lisp_query_flag(args[1]);
    let proc = processes.get_any_mut(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    proc.exit_query_policy = policy;
    Ok(args[1])
}

/// (process-command PROCESS) -> list
pub(crate) fn builtin_process_command(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_process_command_impl(&eval.processes, args)
}

pub(crate) fn builtin_process_command_impl(
    processes: &ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-command", &args, 1)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    let proc = processes.get_any(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    Ok(proc.command)
}

/// (process-contact PROCESS &optional KEY NO-BLOCK) -> value
pub(crate) fn builtin_process_contact(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if (1..=3).contains(&args.len()) {
        let no_block = args.get(2).is_some_and(|value| value.is_truthy());
        if let Some(id) = pending_network_connect_id(&eval.processes, args[0])? {
            if no_block {
                return Ok(Value::NIL);
            }
            eval.wait_while_network_process_connecting(id)?;
        }
    }
    builtin_process_contact_impl(&eval.processes, args)
}

pub(crate) fn builtin_process_contact_impl(
    processes: &ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("process-contact", &args, 1)?;
    if args.len() > 3 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("process-contact"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    let proc = processes.get_any(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    let key = args.get(1).copied().unwrap_or(Value::NIL);
    let mut contact = proc.childp;
    match proc.proc_type.as_symbol_name() {
        Some("network") => {
            if process_is_datagram_network(proc)
                && (key == Value::T || key == ProcessKeyword::Remote.value())
            {
                contact = process_contact_plist_put(
                    contact,
                    ProcessKeyword::Remote.value(),
                    proc.datagram_address,
                )?;
            }
            if key == Value::T {
                Ok(contact)
            } else if key.is_nil() {
                Ok(Value::list(vec![
                    process_contact_plist_get(contact, ProcessKeyword::Host.value()),
                    process_contact_plist_get(contact, ProcessKeyword::Service.value()),
                ]))
            } else {
                Ok(process_contact_plist_get(contact, key))
            }
        }
        Some("serial") => {
            if key == Value::T {
                Ok(contact)
            } else if key.is_nil() {
                Ok(Value::list(vec![
                    process_contact_plist_get(contact, ProcessKeyword::Port.value()),
                    process_contact_plist_get(contact, ProcessKeyword::Speed.value()),
                ]))
            } else {
                Ok(process_contact_plist_get(contact, key))
            }
        }
        Some("pipe") => {
            if key == Value::T {
                Ok(contact)
            } else if key.is_nil() {
                Ok(Value::T)
            } else {
                Ok(process_contact_plist_get(contact, key))
            }
        }
        _ => Ok(contact),
    }
}

/// (process-filter PROCESS) -> function
pub(crate) fn builtin_process_filter(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_process_filter_impl(&eval.processes, args)
}

pub(crate) fn builtin_process_filter_impl(
    processes: &ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-filter", &args, 1)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    let proc = processes.get_any(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    Ok(proc.filter)
}

/// (set-process-filter PROCESS FILTER) -> FILTER
pub(crate) fn builtin_set_process_filter(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_set_process_filter_impl(&mut eval.processes, args)
}

pub(crate) fn builtin_set_process_filter_impl(
    processes: &mut ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-process-filter", &args, 2)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    let stored = if args[1].is_nil() {
        Value::symbol(DEFAULT_PROCESS_FILTER_SYMBOL)
    } else {
        args[1]
    };
    let (accepted_output_before, accepts_output_after) = {
        let proc = processes.get_any_mut(id).ok_or_else(|| {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("processp"), args[0]],
            )
        })?;
        let accepted_output_before = process_filter_accepts_output(proc);
        proc.filter = stored;
        if process_uses_contact_plist(proc) {
            proc.childp =
                process_contact_plist_put(proc.childp, ProcessKeyword::Filter.value(), stored)?;
        }
        (accepted_output_before, process_filter_accepts_output(proc))
    };
    if accepted_output_before != accepts_output_after {
        processes.set_process_output_read_interest(id, accepts_output_after);
    }
    Ok(stored)
}

/// (process-sentinel PROCESS) -> function
pub(crate) fn builtin_process_sentinel(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_process_sentinel_impl(&eval.processes, args)
}

pub(crate) fn builtin_process_sentinel_impl(
    processes: &ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-sentinel", &args, 1)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    let proc = processes.get_any(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    Ok(proc.sentinel)
}

/// (set-process-sentinel PROCESS SENTINEL) -> SENTINEL
pub(crate) fn builtin_set_process_sentinel(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_set_process_sentinel_impl(&mut eval.processes, args)
}

pub(crate) fn builtin_set_process_sentinel_impl(
    processes: &mut ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-process-sentinel", &args, 2)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    let stored = if args[1].is_nil() {
        Value::symbol(DEFAULT_PROCESS_SENTINEL_SYMBOL)
    } else {
        args[1]
    };
    let proc = processes.get_any_mut(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    proc.sentinel = stored;
    if process_uses_contact_plist(proc) {
        proc.childp =
            process_contact_plist_put(proc.childp, ProcessKeyword::Sentinel.value(), stored)?;
    }
    Ok(stored)
}

/// (process-plist PROCESS) -> plist
pub(crate) fn builtin_process_plist(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_process_plist_impl(&eval.processes, args)
}

pub(crate) fn builtin_process_plist_impl(
    processes: &ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("process-plist", &args, 1)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    let proc = processes.get_any(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    Ok(proc.plist)
}

/// (set-process-plist PROCESS PLIST) -> plist
pub(crate) fn builtin_set_process_plist(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_set_process_plist_impl(&mut eval.processes, args)
}

pub(crate) fn builtin_set_process_plist_impl(
    processes: &mut ProcessManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-process-plist", &args, 2)?;
    let id = resolve_process_object_or_wrong_type_any_in_manager(processes, &args[0])?;
    if !args[1].is_list() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), args[1]],
        ));
    }
    let proc = processes.get_any_mut(id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), args[0]],
        )
    })?;
    proc.plist = args[1];
    Ok(proc.plist)
}

// ---------------------------------------------------------------------------
// Builtins (pure — no evaluator needed)
// ---------------------------------------------------------------------------

/// (getenv-internal VARIABLE &optional ENV) -> string or nil
pub(crate) fn builtin_getenv_internal(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("getenv-internal", &args, 1)?;
    if args.len() > 2 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("getenv-internal"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    // `getenv_internal` takes `&mut Context`, so a borrow of VARNAME's payload
    // would span it. The name is short and this is not a hot path, so copy the
    // bytes out rather than reason about whether the callee can reach a
    // safepoint (DIVERGENCES.md 163).
    let varname = eval.expect_lisp_string(args[0])?.clone();
    super::environment::getenv_internal(eval, &varname, args.get(1).copied().unwrap_or(Value::NIL))
}

pub(crate) fn make_network_process_subfeatures() -> Value {
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

/// (set-binary-mode STREAM MODE) -> t
///
/// Batch/runtime compatibility path. Accepts stdin/stdout/stderr symbols.
pub(crate) fn builtin_set_binary_mode(args: Vec<Value>) -> EvalResult {
    expect_args("set-binary-mode", &args, 2)?;
    let stream = args[0].as_symbol_name().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), args[0]],
        )
    })?;

    match stream {
        "stdin" | "stdout" | "stderr" => Ok(Value::T),
        _ => Err(signal(
            "error",
            vec![Value::string("unsupported stream"), args[0]],
        )),
    }
}

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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
#[path = "tests/raw_bytes.rs"]
mod raw_bytes_tests;
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
