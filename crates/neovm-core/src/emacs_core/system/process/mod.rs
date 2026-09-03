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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use crate::host_time::Instant;
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
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
#[path = "tests/raw_bytes.rs"]
mod raw_bytes_tests;

mod builtins;
pub use builtins::*;

mod bootstrap_vars;
pub use bootstrap_vars::*;

mod helpers;
pub use helpers::*;

mod types;
pub use types::*;
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
