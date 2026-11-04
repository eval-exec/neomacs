//! Callproc spawn state owned by the evaluator layer (stage-4 W2/E6c-A1
//! TYPE-DOWN).
//!
//! Peer ruling 16:2xZ: the process state unit extends down to the spawn
//! vocabulary so the future `neovm-eval` crate can name every field of the
//! process state.  Only type definitions and their state conversions live
//! here; the spawn logic itself (`ChildCommand`, posix_spawn/fork) and the
//! child wait/kill IO stay in `emacs_core::system::callproc` as the
//! `SpawnedChildIo` extension trait, implemented for this type in the upper
//! crate (orphan-legal: the trait is core-local).
//!
//! `ChildStdio`'s `os_pipe` conversions are the only new external dependency
//! this unit introduces; it is an eval-only entry copied verbatim from
//! core's workspace dependency (no features added), so workspace feature
//! unification is unchanged.

use std::fs::File;
use std::process::ExitStatus;

/// One standard descriptor of the child, mirroring `std::process::Stdio`.
pub enum ChildStdio {
    /// Share the editor's descriptor.
    Inherit,
    /// Connect to `/dev/null` (or the platform's null device).
    Null,
    /// A fresh pipe whose editor end lands on the [`SpawnedChild`].
    Piped,
    /// An open file or pipe end the child receives as its descriptor.
    File(File),
}

impl From<File> for ChildStdio {
    fn from(file: File) -> Self {
        Self::File(file)
    }
}

impl From<os_pipe::PipeWriter> for ChildStdio {
    fn from(writer: os_pipe::PipeWriter) -> Self {
        Self::File(pipe_writer_into_file(writer))
    }
}

impl From<os_pipe::PipeReader> for ChildStdio {
    fn from(reader: os_pipe::PipeReader) -> Self {
        Self::File(pipe_reader_into_file(reader))
    }
}

#[cfg(unix)]
fn pipe_writer_into_file(writer: os_pipe::PipeWriter) -> File {
    File::from(std::os::fd::OwnedFd::from(writer))
}

#[cfg(unix)]
fn pipe_reader_into_file(reader: os_pipe::PipeReader) -> File {
    File::from(std::os::fd::OwnedFd::from(reader))
}

#[cfg(windows)]
fn pipe_writer_into_file(writer: os_pipe::PipeWriter) -> File {
    File::from(std::os::windows::io::OwnedHandle::from(writer))
}

#[cfg(windows)]
fn pipe_reader_into_file(reader: os_pipe::PipeReader) -> File {
    File::from(std::os::windows::io::OwnedHandle::from(reader))
}

/// A spawned pipe-stdio child: its pid, the editor's ends of any piped
/// descriptors, and the handle that waits for or kills it.
pub struct SpawnedChild {
    pub pid: u32,
    pub stdin: Option<File>,
    pub stdout: Option<File>,
    pub stderr: Option<File>,
    pub backend: ChildBackend,
    /// ECHILD revokes numeric signalling; it is not an exit status.
    pub signal_authority: bool,
}

pub enum ChildBackend {
    /// Reaped with `waitpid`; `status` caches the exit once collected so a
    /// later `kill` can never signal a reused pid.
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    Pid {
        status: Option<ExitStatus>,
    },
    Std(std::process::Child),
}

impl SpawnedChild {
    /// Adopt a child spawned through `std::process::Command`, moving its
    /// piped descriptors onto this handle.
    pub fn from_std(mut child: std::process::Child) -> Self {
        let stdin = child.stdin.take().map(std_pipe_into_file);
        let stdout = child.stdout.take().map(std_pipe_into_file);
        let stderr = child.stderr.take().map(std_pipe_into_file);
        Self {
            pid: child.id(),
            stdin,
            stdout,
            stderr,
            backend: ChildBackend::Std(child),
            signal_authority: true,
        }
    }

    pub fn id(&self) -> u32 {
        self.pid
    }
}

#[cfg(unix)]
fn std_pipe_into_file<T: Into<std::os::fd::OwnedFd>>(pipe: T) -> File {
    File::from(pipe.into())
}

#[cfg(windows)]
fn std_pipe_into_file<T: Into<std::os::windows::io::OwnedHandle>>(pipe: T) -> File {
    File::from(pipe.into())
}
