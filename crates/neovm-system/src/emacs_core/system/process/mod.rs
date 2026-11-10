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

use crate::emacs_core::callproc::{ChildStdio, SpawnedChild};
use neovm_value::error::LispCondition;
use neovm_value::error::{expect_args, expect_min_args};
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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use strum::{EnumString, IntoStaticStr};

use super::tls::{
    RustlsBackend, TlsBackendError, TlsClientBackend, TlsClientParameters, TlsHandshakeInterest,
    TlsHandshakeProgress, TlsStream, TlsStreamIo, gnutls_close_notify_result_value,
    gnutls_peer_status_to_value, parse_gnutls_boot_parameters,
};
use super::wait::ProcessOutputWaitOutcome;

/// Platform abstraction layer for OS-specific subprocess facilities (currently
/// the child-status wait source). See `process/sys/mod.rs`.
#[cfg(not(feature = "test-support"))]
pub(crate) mod sys;
/// Platform abstraction layer for OS-specific subprocess facilities (currently
/// the child-status wait source). See `process/sys/mod.rs`.
#[cfg(feature = "test-support")]
pub mod sys;
use sys::ChildStatusSource;

#[doc(hidden)]
pub mod green_select;
pub use green_select::{GreenSelectHandle, MainNativePreparation};

mod executable;
pub(super) use executable::{ExecutableLookupMode, ExecutableSearch};

pub(crate) mod pty_output;

/// GNU `status_notify`'s "retire, then run the sentinel" ordering, as a type.
/// See `process/status_notify.rs`.
mod status_notify;
pub(crate) use reap::{ChildOwnershipIo, ReapableChildIo};
use status_notify::ProcessStatusNotification;

/// GNU `handle_child_signal`'s ASYNCHRONOUS child-status recording
/// (src/process.c:7691), and the type that keeps a Lisp-visible status from
/// being read before the recording has been made.  See
/// `process/child_status.rs`.
#[cfg(not(feature = "test-support"))]
pub(crate) mod child_status;
/// GNU `handle_child_signal`'s ASYNCHRONOUS child-status recording
/// (src/process.c:7691), and the type that keeps a Lisp-visible status from
/// being read before the recording has been made.  See
/// `process/child_status.rs`.
#[cfg(feature = "test-support")]
pub mod child_status;
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) use child_status::{UnrecordedStatusRead, UpdateStatusSite};

/// The single owner of `waitpid`, and GNU's `p->alive` as a type: a child that
/// has been reaped has no pid to hand to `kill(2)` or to a second `waitpid`.
/// See `process/reap.rs`.
#[doc(hidden)]
pub mod reap;
pub(crate) use reap::ChildStatusChange;

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
    pub fn select(proc: &Process) -> Option<Self> {
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

pub(crate) trait NetworkSocketIo {
    fn register_readable(&self, poller: &polling::Poller, id: ProcessId) -> Result<(), String>;
    fn register_writable(&self, poller: &polling::Poller, id: ProcessId) -> Result<(), String>;
    fn unregister_readable(&self, poller: &polling::Poller);
    fn read_stream_output(&mut self, buf: &mut [u8]) -> Option<std::io::Result<usize>>;
    fn write_input_once(
        &mut self,
        bytes: &[u8],
        datagram_addr: Option<SocketAddr>,
        #[cfg(unix)] datagram_unix_path: Option<std::path::PathBuf>,
    ) -> Option<std::io::Result<usize>>;
    fn modify_interest(
        &self,
        poller: &polling::Poller,
        id: ProcessId,
        event: polling::Event,
    ) -> Result<(), String>;
    fn shutdown_write(&self) -> Option<std::io::Result<()>>;
    fn take_pending_connect_error(&self) -> Option<std::io::Result<Option<std::io::Error>>>;
}

impl NetworkSocketIo for NetworkSocket {
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

use neovm_edit::buffer::{
    BufferId, BufferManager, EmacsByteLen, EmacsBytePos, EmacsByteRange, LispCharPos1,
};
use neovm_eval::emacs_core::threads::ThreadManager;
use neovm_value::error::{EvalResult, Flow, signal};
use neovm_value::gc_trace::GcTrace;
use neovm_value::heap_types::LispString;
use neovm_value::intern::{intern, resolve_sym};
use neovm_value::value::{Value, ValueKind, VecLikeType, equal_value, list_to_vec};

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[doc(hidden)]
pub mod builtins;
pub use builtins::*;

mod bootstrap_vars;
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub use bootstrap_vars::*;

mod helpers;
#[cfg(not(feature = "test-support"))]
pub(crate) use helpers::*;
#[cfg(feature = "test-support")]
pub use helpers::*;

#[doc(hidden)]
pub mod types;
// The glob is already plain `pub`; a `visibility::make(pub)` attribute here
// was a no-op that the VIS coverage scan still attributed to the next named
// re-export, keeping a false ledger row alive (lead #1052 retirement).
pub use types::*;
pub(crate) use types::{swap_world_state, world_state_placeholder};

/// Test-support entry points for the option-A re-homed core process tests.
#[cfg(any(test, feature = "test-support"))]
pub mod test_support {
    use super::{ProcessId, ProcessManager};

    /// Owns the deferred-watcher lifecycle for a test scope.
    pub struct DeferredWatcherTestScopeForTest(super::reap::DeferredWatcherTestScope);

    impl DeferredWatcherTestScopeForTest {
        pub fn new() -> Self {
            Self(super::reap::DeferredWatcherTestScope::new())
        }

        pub fn worker_count_for_test(&self) -> usize {
            self.0.worker_count_for_test()
        }

        pub fn finish(self) {
            self.0.finish();
        }
    }

    /// Test-support facade for the option-A re-homed deferred-reap unit tests
    /// (mock ownership transfer, interrupted probe, ordinary-wait non-probe,
    /// and the native deleted/dropped-PTY paths).
    ///
    /// The registry's native publication source, the native-exit sweep and the
    /// deferred-ownership probe stay private to `process::reap`; this facade is
    /// their only cross-crate route. The type row covers the three associated
    /// functions (fam-15 precedent).
    pub struct DeferredReapFixtureForTest;

    impl DeferredReapFixtureForTest {
        /// Publish an owned child on the native-exit driver without a real
        /// native watcher; the re-homed test forces the completion state.
        pub fn publish_native(
            child: neovm_eval::emacs_core::eval::data::process_state::ReapableChild,
        ) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
            super::reap::publish_native_deferred_child_for_test(child)
        }

        /// Run the native-exit sweep once.
        pub fn collect_native() {
            super::reap::collect_native_deferred_children_for_test();
        }

        /// Whether the deferred registry still owns PID.
        pub fn is_owned(pid: u32) -> bool {
            super::reap::deferred_child_is_owned_for_test(pid)
        }
    }

    pub fn order_pending_status_notifications_newest_first_for_test(
        processes: &ProcessManager,
        proc_ids: &mut [ProcessId],
    ) {
        super::types::order_pending_status_notifications_newest_first(processes, proc_ids);
    }
}

#[cfg(all(test, unix))]
#[path = "tests/native_deferred_reap.rs"]
mod native_deferred_reap_tests;

#[cfg(test)]
#[path = "tests/status_change_sites_test.rs"]
mod status_change_sites_tests;
