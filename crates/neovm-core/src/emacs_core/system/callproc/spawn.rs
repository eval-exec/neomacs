//! Child creation for pipe-stdio children -- GNU `emacs_spawn` (src/callproc.c).
//!
//! GNU prefers `posix_spawn` whenever the child needs no pseudoterminal:
//!
//! ```c
//! bool use_posix_spawn = pty_name == NULL;
//! ...
//! posix_spawnattr_setflags (attributes, POSIX_SPAWN_SETSID
//!                                       | POSIX_SPAWN_SETSIGDEF
//!                                       | POSIX_SPAWN_SETSIGMASK);
//! ```
//!
//! with `adddup2` for the three standard descriptors and `addchdir(_np)` for
//! the working directory.  Only pty children take the `vfork` path.
//!
//! `std::process::Command` cannot express `setsid` on stable Rust without a
//! `pre_exec` closure, and any `pre_exec` closure forces the classic `fork`
//! path: the parent copies its page tables (milliseconds for a large heap),
//! then blocks on the exec handshake while the child tears that copy down
//! again inside `execve`.  On the magit-status workload that was 3.5 ms in
//! `clone` plus 4.2 ms in the handshake per `git` spawn, 22 spawns per
//! refresh, against GNU's 0.9 ms round trip including `git` itself.
//! `posix_spawn` shares the address space until `exec` (`CLONE_VM |
//! CLONE_VFORK`) and costs tens of microseconds regardless of heap size.
//!
//! The platform split is resolved ONCE here.  glibc Linux takes the
//! `posix_spawn` engine; every other platform keeps `std::process::Command`
//! with the same session isolation applied by
//! [`super::isolate_child_command`].  Pseudoterminal children need post-fork
//! setup `posix_spawn` cannot express, so they take
//! [`ChildCommand::into_forking_command`] -- GNU's `vfork` fallback.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Output};

/// One standard descriptor of the child, mirroring `std::process::Stdio`.
pub(crate) enum ChildStdio {
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

/// Environment edits with `std::process::Command` semantics: `env_clear`
/// drops the inherited environment and every earlier edit; later edits win.
#[derive(Default)]
struct ChildEnv {
    clear: bool,
    /// Set by [`ChildCommand::set_exact_env`]: the child starts from exactly
    /// these variables (the Lisp layer's resolved, de-duplicated list) rather
    /// than from nothing or the editor's own, and `edits` apply on top.
    base: Option<std::sync::Arc<[(OsString, OsString)]>>,
    edits: BTreeMap<OsString, Option<OsString>>,
}

impl ChildEnv {
    /// The variables themselves when they are an exact list with nothing
    /// applied on top -- the `call-process` shape -- so a spawn can hand them
    /// to the child as they are, in `process-environment` order.
    fn exact_without_edits(&self) -> Option<&[(OsString, OsString)]> {
        self.base.as_deref().filter(|_| self.edits.is_empty())
    }

    /// The child's complete environment, inherited unless cleared, with the
    /// recorded edits applied.
    fn materialize(&self) -> BTreeMap<OsString, OsString> {
        let mut env: BTreeMap<OsString, OsString> = if self.clear {
            BTreeMap::new()
        } else {
            std::env::vars_os().collect()
        };
        for (name, value) in self.base.iter().flat_map(|base| base.iter()) {
            env.insert(name.clone(), value.clone());
        }
        for (name, value) in &self.edits {
            match value {
                Some(value) => {
                    env.insert(name.clone(), value.clone());
                }
                None => {
                    env.remove(name);
                }
            }
        }
        env
    }

    fn apply_to_command(&self, command: &mut std::process::Command) {
        if self.clear {
            command.env_clear();
        }
        for (name, value) in self.base.iter().flat_map(|base| base.iter()) {
            command.env(name, value);
        }
        for (name, value) in &self.edits {
            match value {
                Some(value) => {
                    command.env(name, value);
                }
                None => {
                    command.env_remove(name);
                }
            }
        }
    }
}

/// A pipe-stdio child to be spawned in its own session, built with the
/// subset of the `std::process::Command` API the process layer uses.
///
/// A command spawns once: [`ChildCommand::spawn`] moves the configured
/// descriptors into the child.
pub(crate) struct ChildCommand {
    program: OsString,
    args: Vec<OsString>,
    env: ChildEnv,
    cwd: Option<PathBuf>,
    stdin: Option<ChildStdio>,
    stdout: Option<ChildStdio>,
    stderr: Option<ChildStdio>,
}

impl ChildCommand {
    pub(crate) fn new<S: AsRef<OsStr>>(program: S) -> Self {
        Self {
            program: program.as_ref().to_os_string(),
            args: Vec::new(),
            env: ChildEnv::default(),
            cwd: None,
            stdin: None,
            stdout: None,
            stderr: None,
        }
    }

    pub(crate) fn arg<S: AsRef<OsStr>>(&mut self, arg: S) -> &mut Self {
        self.args.push(arg.as_ref().to_os_string());
        self
    }

    pub(crate) fn args<I, S>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        for arg in args {
            self.arg(arg);
        }
        self
    }

    pub(crate) fn env<K: AsRef<OsStr>, V: AsRef<OsStr>>(&mut self, key: K, value: V) -> &mut Self {
        self.env.edits.insert(
            key.as_ref().to_os_string(),
            Some(value.as_ref().to_os_string()),
        );
        self
    }

    pub(crate) fn env_remove<K: AsRef<OsStr>>(&mut self, key: K) -> &mut Self {
        self.env.edits.insert(key.as_ref().to_os_string(), None);
        self
    }

    #[cfg(test)]
    pub(crate) fn env_clear(&mut self) -> &mut Self {
        self.env.clear = true;
        self.env.base = None;
        self.env.edits.clear();
        self
    }

    /// Give the child exactly `vars` (unique names), replacing the inherited
    /// environment and every earlier edit -- `env_clear` plus `envs`, without
    /// copying each variable into the edit map.
    pub(crate) fn set_exact_env(
        &mut self,
        vars: std::sync::Arc<[(OsString, OsString)]>,
    ) -> &mut Self {
        self.env.clear = true;
        self.env.base = Some(vars);
        self.env.edits.clear();
        self
    }

    pub(crate) fn current_dir<P: AsRef<Path>>(&mut self, dir: P) -> &mut Self {
        self.cwd = Some(dir.as_ref().to_path_buf());
        self
    }

    pub(crate) fn stdin<T: Into<ChildStdio>>(&mut self, stdio: T) -> &mut Self {
        self.stdin = Some(stdio.into());
        self
    }

    pub(crate) fn stdout<T: Into<ChildStdio>>(&mut self, stdio: T) -> &mut Self {
        self.stdout = Some(stdio.into());
        self
    }

    pub(crate) fn stderr<T: Into<ChildStdio>>(&mut self, stdio: T) -> &mut Self {
        self.stderr = Some(stdio.into());
        self
    }

    /// Spawn the child in its own session.  Unset descriptors are inherited,
    /// as with `std::process::Command::spawn`.
    pub(crate) fn spawn(&mut self) -> io::Result<SpawnedChild> {
        let stdin = self.stdin.take().unwrap_or(ChildStdio::Inherit);
        let stdout = self.stdout.take().unwrap_or(ChildStdio::Inherit);
        let stderr = self.stderr.take().unwrap_or(ChildStdio::Inherit);
        self.spawn_with(stdin, stdout, stderr)
    }

    /// Spawn, wait, and collect the output.  Unset descriptors default the
    /// way `std::process::Command::output` defaults them: stdin closed,
    /// stdout and stderr captured.
    pub(crate) fn output(&mut self) -> io::Result<Output> {
        let stdin = self.stdin.take().unwrap_or(ChildStdio::Null);
        let stdout = self.stdout.take().unwrap_or(ChildStdio::Piped);
        let stderr = self.stderr.take().unwrap_or(ChildStdio::Piped);
        self.spawn_with(stdin, stdout, stderr)?.wait_with_output()
    }

    /// The `posix_spawn` engine needs `POSIX_SPAWN_SETSID` (glibc 2.26) and
    /// `posix_spawn_file_actions_addchdir_np` (glibc 2.29).  GNU gates the
    /// same way (`USABLE_POSIX_SPAWN`).
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    fn spawn_with(
        &self,
        stdin: ChildStdio,
        stdout: ChildStdio,
        stderr: ChildStdio,
    ) -> io::Result<SpawnedChild> {
        posix::spawn(self, stdin, stdout, stderr)
    }

    /// Elsewhere the child forks through `std::process::Command`, with the
    /// same session isolation applied in the forked child.
    #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
    fn spawn_with(
        &self,
        stdin: ChildStdio,
        stdout: ChildStdio,
        stderr: ChildStdio,
    ) -> io::Result<SpawnedChild> {
        let mut command = self.forking_command();
        command.stdin(std_stdio(stdin));
        command.stdout(std_stdio(stdout));
        command.stderr(std_stdio(stderr));
        command.spawn().map(SpawnedChild::from_std)
    }

    /// The same child as a `std::process::Command` that will `fork`, with
    /// the session isolation already installed.  For children that need
    /// post-fork setup `posix_spawn` cannot express (a pty as controlling
    /// terminal); GNU likewise falls back to `vfork` for pty children.
    pub(crate) fn into_forking_command(mut self) -> std::process::Command {
        let mut command = self.forking_command();
        if let Some(stdin) = self.stdin.take() {
            command.stdin(std_stdio(stdin));
        }
        if let Some(stdout) = self.stdout.take() {
            command.stdout(std_stdio(stdout));
        }
        if let Some(stderr) = self.stderr.take() {
            command.stderr(std_stdio(stderr));
        }
        command
    }

    fn forking_command(&self) -> std::process::Command {
        let mut command = std::process::Command::new(&self.program);
        command.args(&self.args);
        self.env.apply_to_command(&mut command);
        if let Some(dir) = &self.cwd {
            command.current_dir(dir);
        }
        super::isolate_child_command(&mut command);
        command
    }

    /// Whether spawning goes through `posix_spawn` on this platform.
    #[cfg(test)]
    pub(crate) fn uses_posix_spawn() -> bool {
        cfg!(all(target_os = "linux", target_env = "gnu"))
    }
}

fn std_stdio(stdio: ChildStdio) -> std::process::Stdio {
    match stdio {
        ChildStdio::Inherit => std::process::Stdio::inherit(),
        ChildStdio::Null => std::process::Stdio::null(),
        ChildStdio::Piped => std::process::Stdio::piped(),
        ChildStdio::File(file) => std::process::Stdio::from(file),
    }
}

/// A spawned pipe-stdio child: its pid, the editor's ends of any piped
/// descriptors, and the handle that waits for or kills it.
pub(crate) struct SpawnedChild {
    pid: u32,
    pub(crate) stdin: Option<File>,
    pub(crate) stdout: Option<File>,
    pub(crate) stderr: Option<File>,
    backend: ChildBackend,
}

enum ChildBackend {
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
    pub(crate) fn from_std(mut child: std::process::Child) -> Self {
        let stdin = child.stdin.take().map(std_pipe_into_file);
        let stdout = child.stdout.take().map(std_pipe_into_file);
        let stderr = child.stderr.take().map(std_pipe_into_file);
        Self {
            pid: child.id(),
            stdin,
            stdout,
            stderr,
            backend: ChildBackend::Std(child),
        }
    }

    pub(crate) fn id(&self) -> u32 {
        self.pid
    }

    pub(crate) fn wait(&mut self) -> io::Result<ExitStatus> {
        match &mut self.backend {
            #[cfg(all(target_os = "linux", target_env = "gnu"))]
            ChildBackend::Pid { status } => {
                if let Some(status) = status {
                    return Ok(*status);
                }
                let collected = posix::wait_blocking(self.pid)?;
                *status = Some(collected);
                Ok(collected)
            }
            ChildBackend::Std(child) => child.wait(),
        }
    }

    /// Non-blocking status probe.  The unix reaper probes by pid through
    /// `waitpid` directly (it needs the stop/continue detail), so only the
    /// non-unix reaper and tests reach this method there.
    #[cfg_attr(unix, allow(dead_code))]
    pub(crate) fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        match &mut self.backend {
            #[cfg(all(target_os = "linux", target_env = "gnu"))]
            ChildBackend::Pid { status } => {
                if status.is_some() {
                    return Ok(*status);
                }
                let collected = posix::wait_nonblocking(self.pid)?;
                if collected.is_some() {
                    *status = collected;
                }
                Ok(collected)
            }
            ChildBackend::Std(child) => child.try_wait(),
        }
    }

    /// Force the child to exit.  Like `std::process::Child::kill`, a child
    /// that has already been reaped is left alone and `Ok(())` is returned.
    pub(crate) fn kill(&mut self) -> io::Result<()> {
        match &mut self.backend {
            #[cfg(all(target_os = "linux", target_env = "gnu"))]
            ChildBackend::Pid { status } => {
                if status.is_some() {
                    return Ok(());
                }
                posix::kill(self.pid)
            }
            ChildBackend::Std(child) => child.kill(),
        }
    }

    /// Close stdin, drain stdout and stderr to completion, then wait.
    pub(crate) fn wait_with_output(mut self) -> io::Result<Output> {
        drop(self.stdin.take());
        let (stdout, stderr) = drain_pipes(self.stdout.take(), self.stderr.take())?;
        let status = self.wait()?;
        Ok(Output {
            status,
            stdout,
            stderr,
        })
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

/// Read both output pipes to EOF without letting either fill and block the
/// child.  Unix multiplexes with `poll`; elsewhere a helper thread drains
/// stderr while this thread drains stdout.
#[cfg(unix)]
fn drain_pipes(stdout: Option<File>, stderr: Option<File>) -> io::Result<(Vec<u8>, Vec<u8>)> {
    use std::io::Read;
    use std::os::fd::AsRawFd;

    let mut out = Vec::new();
    let mut err = Vec::new();
    match (stdout, stderr) {
        (None, None) => {}
        (Some(mut stdout), None) => {
            stdout.read_to_end(&mut out)?;
        }
        (None, Some(mut stderr)) => {
            stderr.read_to_end(&mut err)?;
        }
        (Some(stdout), Some(stderr)) => {
            for file in [&stdout, &stderr] {
                set_nonblocking(file.as_raw_fd())?;
            }
            let mut sources = [Some((stdout, &mut out)), Some((stderr, &mut err))];
            loop {
                let mut fds: Vec<libc::pollfd> = sources
                    .iter()
                    .flatten()
                    .map(|(file, _)| libc::pollfd {
                        fd: file.as_raw_fd(),
                        events: libc::POLLIN,
                        revents: 0,
                    })
                    .collect();
                if fds.is_empty() {
                    break;
                }
                // SAFETY: `fds` is a valid, initialized array of `pollfd`s and
                // the length passed matches it.
                let ready = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, -1) };
                if ready < 0 {
                    let error = io::Error::last_os_error();
                    if error.kind() == io::ErrorKind::Interrupted {
                        continue;
                    }
                    return Err(error);
                }
                for source in sources.iter_mut() {
                    let Some((file, buffer)) = source else {
                        continue;
                    };
                    let fd = file.as_raw_fd();
                    let Some(pollfd) = fds.iter().find(|pollfd| pollfd.fd == fd) else {
                        continue;
                    };
                    if pollfd.revents == 0 {
                        continue;
                    }
                    match read_available(file, buffer) {
                        Ok(true) => *source = None,
                        Ok(false) => {}
                        Err(error) => return Err(error),
                    }
                }
            }
        }
    }
    Ok((out, err))
}

#[cfg(unix)]
fn set_nonblocking(fd: std::os::fd::RawFd) -> io::Result<()> {
    // SAFETY: plain `fcntl` calls on a descriptor this function's caller owns.
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL);
        if flags == -1 {
            return Err(io::Error::last_os_error());
        }
        if libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) == -1 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

/// Read what a non-blocking pipe has ready; `Ok(true)` at EOF.
#[cfg(unix)]
fn read_available(file: &mut File, buffer: &mut Vec<u8>) -> io::Result<bool> {
    use std::io::Read;

    let mut chunk = [0u8; 8192];
    loop {
        match file.read(&mut chunk) {
            Ok(0) => return Ok(true),
            Ok(n) => buffer.extend_from_slice(&chunk[..n]),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(false),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
}

#[cfg(not(unix))]
fn drain_pipes(stdout: Option<File>, stderr: Option<File>) -> io::Result<(Vec<u8>, Vec<u8>)> {
    use std::io::Read;

    let stderr_reader = stderr.map(|mut stderr| {
        std::thread::spawn(move || {
            let mut err = Vec::new();
            stderr.read_to_end(&mut err).map(|_| err)
        })
    });
    let mut out = Vec::new();
    if let Some(mut stdout) = stdout {
        stdout.read_to_end(&mut out)?;
    }
    let err = match stderr_reader {
        Some(reader) => reader
            .join()
            .map_err(|_| io::Error::other("stderr reader thread panicked"))??,
        None => Vec::new(),
    };
    Ok((out, err))
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
mod posix {
    //! The `posix_spawn` engine and the pid-based waiting its children use.

    use super::*;
    use std::os::unix::process::ExitStatusExt;

    pub(super) fn wait_blocking(pid: u32) -> io::Result<ExitStatus> {
        loop {
            let mut status: libc::c_int = 0;
            // SAFETY: `status` is a valid out-pointer for the duration of the call.
            let reaped = unsafe { libc::waitpid(pid as libc::pid_t, &mut status, 0) };
            if reaped == -1 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error);
            }
            return Ok(ExitStatus::from_raw(status));
        }
    }

    pub(super) fn wait_nonblocking(pid: u32) -> io::Result<Option<ExitStatus>> {
        loop {
            let mut status: libc::c_int = 0;
            // SAFETY: as above; `WNOHANG` makes the call non-blocking.
            let reaped = unsafe { libc::waitpid(pid as libc::pid_t, &mut status, libc::WNOHANG) };
            if reaped == -1 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error);
            }
            if reaped == 0 {
                return Ok(None);
            }
            return Ok(Some(ExitStatus::from_raw(status)));
        }
    }

    pub(super) fn kill(pid: u32) -> io::Result<()> {
        // SAFETY: plain syscall on a pid this handle owns and has not reaped.
        if unsafe { libc::kill(pid as libc::pid_t, libc::SIGKILL) } == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    pub(super) fn spawn(
        command: &ChildCommand,
        stdin: ChildStdio,
        stdout: ChildStdio,
        stderr: ChildStdio,
    ) -> io::Result<SpawnedChild> {
        engine::spawn(command, stdin, stdout, stderr)
    }

    mod engine {
        use super::*;
        use std::ffi::CString;
        use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
        use std::os::unix::ffi::OsStrExt;

        /// What the child's descriptor becomes, plus the editor's end of a
        /// pipe when one was requested.
        struct Slot {
            action: SlotAction,
            /// Keeps a dup source alive until `posix_spawn` has consumed it.
            _child_end: Option<OwnedFd>,
            parent_end: Option<File>,
        }

        enum SlotAction {
            Inherit,
            Null,
            Dup(RawFd),
        }

        fn slot(stdio: ChildStdio, child_reads: bool) -> io::Result<Slot> {
            Ok(match stdio {
                ChildStdio::Inherit => Slot {
                    action: SlotAction::Inherit,
                    _child_end: None,
                    parent_end: None,
                },
                ChildStdio::Null => Slot {
                    action: SlotAction::Null,
                    _child_end: None,
                    parent_end: None,
                },
                ChildStdio::File(file) => Slot {
                    action: SlotAction::Dup(file.as_raw_fd()),
                    _child_end: Some(OwnedFd::from(file)),
                    parent_end: None,
                },
                ChildStdio::Piped => {
                    let (read_end, write_end) = cloexec_pipe()?;
                    let (child_end, parent_end) = if child_reads {
                        (read_end, write_end)
                    } else {
                        (write_end, read_end)
                    };
                    Slot {
                        action: SlotAction::Dup(child_end.as_raw_fd()),
                        _child_end: Some(child_end),
                        parent_end: Some(File::from(parent_end)),
                    }
                }
            })
        }

        fn cloexec_pipe() -> io::Result<(OwnedFd, OwnedFd)> {
            let mut fds: [libc::c_int; 2] = [-1, -1];
            // SAFETY: `fds` is a valid two-element out array.
            if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: `pipe2` succeeded, so both descriptors are open and owned
            // by nothing else.
            Ok(unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) })
        }

        fn c_string(value: &OsStr, what: &str) -> io::Result<CString> {
            CString::new(value.as_bytes()).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("{what} contains an interior NUL byte"),
                )
            })
        }

        /// Owner of a `posix_spawn_file_actions_t`, destroyed on every exit.
        struct FileActions(libc::posix_spawn_file_actions_t);

        impl FileActions {
            fn new() -> io::Result<Self> {
                let mut actions =
                    std::mem::MaybeUninit::<libc::posix_spawn_file_actions_t>::uninit();
                // SAFETY: `init` fully initializes the out-pointer on success.
                check(unsafe { libc::posix_spawn_file_actions_init(actions.as_mut_ptr()) })?;
                // SAFETY: initialized just above.
                Ok(Self(unsafe { actions.assume_init() }))
            }

            fn apply(&mut self, target: RawFd, action: &SlotAction) -> io::Result<()> {
                match action {
                    SlotAction::Inherit => Ok(()),
                    SlotAction::Null => {
                        let flags = if target == libc::STDIN_FILENO {
                            libc::O_RDONLY
                        } else {
                            libc::O_WRONLY
                        };
                        // SAFETY: the path is a valid NUL-terminated C string.
                        check(unsafe {
                            libc::posix_spawn_file_actions_addopen(
                                &mut self.0,
                                target,
                                c"/dev/null".as_ptr(),
                                flags,
                                0,
                            )
                        })
                    }
                    // SAFETY: plain FFI on an initialized actions object.
                    SlotAction::Dup(source) => check(unsafe {
                        libc::posix_spawn_file_actions_adddup2(&mut self.0, *source, target)
                    }),
                }
            }

            fn chdir(&mut self, dir: &CString) -> io::Result<()> {
                // SAFETY: `dir` is a valid NUL-terminated C string that outlives
                // the spawn.
                check(unsafe {
                    libc::posix_spawn_file_actions_addchdir_np(&mut self.0, dir.as_ptr())
                })
            }
        }

        impl Drop for FileActions {
            fn drop(&mut self) {
                // SAFETY: initialized in `new`; destroyed exactly once here.
                unsafe {
                    libc::posix_spawn_file_actions_destroy(&mut self.0);
                }
            }
        }

        /// Owner of a `posix_spawnattr_t` carrying GNU's attribute set.
        struct Attributes(libc::posix_spawnattr_t);

        impl Attributes {
            /// GNU `emacs_posix_spawn_init_attributes`: own session, default
            /// dispositions for the signals the editor ignores (SIGINT, SIGQUIT,
            /// SIGPIPE, SIGPROF), and an empty signal mask in the child.
            fn gnu() -> io::Result<Self> {
                let mut attributes = std::mem::MaybeUninit::<libc::posix_spawnattr_t>::uninit();
                // SAFETY: `init` fully initializes the out-pointer on success.
                check(unsafe { libc::posix_spawnattr_init(attributes.as_mut_ptr()) })?;
                // SAFETY: initialized just above.
                let mut this = Self(unsafe { attributes.assume_init() });
                let flags = libc::POSIX_SPAWN_SETSID as libc::c_int
                    | libc::POSIX_SPAWN_SETSIGDEF
                    | libc::POSIX_SPAWN_SETSIGMASK;
                // SAFETY: FFI on an initialized attributes object.
                check(unsafe {
                    libc::posix_spawnattr_setflags(&mut this.0, flags as libc::c_short)
                })?;

                let mut defaults = std::mem::MaybeUninit::<libc::sigset_t>::uninit();
                let mut empty = std::mem::MaybeUninit::<libc::sigset_t>::uninit();
                // SAFETY: `sigemptyset`/`sigaddset` initialize and edit the sets
                // through valid pointers; both sets are then fully initialized.
                unsafe {
                    libc::sigemptyset(defaults.as_mut_ptr());
                    for signal in [libc::SIGINT, libc::SIGQUIT, libc::SIGPIPE, libc::SIGPROF] {
                        libc::sigaddset(defaults.as_mut_ptr(), signal);
                    }
                    libc::sigemptyset(empty.as_mut_ptr());
                    check(libc::posix_spawnattr_setsigdefault(
                        &mut this.0,
                        defaults.as_ptr(),
                    ))?;
                    check(libc::posix_spawnattr_setsigmask(
                        &mut this.0,
                        empty.as_ptr(),
                    ))?;
                }
                Ok(this)
            }
        }

        impl Drop for Attributes {
            fn drop(&mut self) {
                // SAFETY: initialized in `gnu`; destroyed exactly once here.
                unsafe {
                    libc::posix_spawnattr_destroy(&mut self.0);
                }
            }
        }

        fn check(code: libc::c_int) -> io::Result<()> {
            if code == 0 {
                Ok(())
            } else {
                Err(io::Error::from_raw_os_error(code))
            }
        }

        /// `std::process::Command` resolves a bare program name against a
        /// `PATH` set *for the child* before falling back to the editor's own
        /// `PATH` (`execvp`).  Mirror that: search the child's `PATH` here;
        /// otherwise let `posix_spawnp` search the editor's.
        fn resolve_program(program: &OsStr, path: Option<&OsStr>) -> Option<PathBuf> {
            if program.as_bytes().contains(&b'/') {
                return None;
            }
            let path = path?;
            std::env::split_paths(path)
                .map(|dir| dir.join(program))
                .find(|candidate| is_executable_file(candidate))
        }

        fn is_executable_file(candidate: &Path) -> bool {
            let Ok(c_path) = CString::new(candidate.as_os_str().as_bytes()) else {
                return false;
            };
            // SAFETY: `c_path` is a valid NUL-terminated C string.
            candidate.is_file() && unsafe { libc::access(c_path.as_ptr(), libc::X_OK) } == 0
        }

        pub(super) fn spawn(
            command: &ChildCommand,
            stdin: ChildStdio,
            stdout: ChildStdio,
            stderr: ChildStdio,
        ) -> io::Result<SpawnedChild> {
            // The child's variables: the exact list itself when nothing was
            // edited on top of it (every `call-process`), else the
            // materialized map.
            let materialized;
            let vars: Vec<(&OsStr, &OsStr)> = match command.env.exact_without_edits() {
                Some(list) => list
                    .iter()
                    .map(|(name, value)| (name.as_os_str(), value.as_os_str()))
                    .collect(),
                None => {
                    materialized = command.env.materialize();
                    materialized
                        .iter()
                        .map(|(name, value)| (name.as_os_str(), value.as_os_str()))
                        .collect()
                }
            };
            let child_path = vars
                .iter()
                .find(|(name, _)| *name == OsStr::new("PATH"))
                .map(|(_, value)| *value);
            let resolved = resolve_program(&command.program, child_path);
            let search_path = resolved.is_none();
            let file = c_string(
                resolved
                    .as_deref()
                    .map_or(command.program.as_os_str(), Path::as_os_str),
                "program",
            )?;
            let argv: Vec<CString> = std::iter::once(c_string(&command.program, "program"))
                .chain(command.args.iter().map(|arg| c_string(arg, "argument")))
                .collect::<io::Result<_>>()?;
            let envp: Vec<CString> = vars
                .iter()
                .map(|(name, value)| {
                    // `NAME=VALUE` plus the NUL `CString` appends, sized once:
                    // growing an exact `to_vec` for `=`, the value and the
                    // NUL reallocated up to three times per variable, per
                    // spawn.
                    let (name, value) = (name.as_bytes(), value.as_bytes());
                    let mut entry = Vec::with_capacity(name.len() + value.len() + 2);
                    entry.extend_from_slice(name);
                    entry.push(b'=');
                    entry.extend_from_slice(value);
                    CString::new(entry).map_err(|_| {
                        io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "environment entry contains an interior NUL byte",
                        )
                    })
                })
                .collect::<io::Result<_>>()?;
            let cwd = command
                .cwd
                .as_deref()
                .map(|dir| c_string(dir.as_os_str(), "working directory"))
                .transpose()?;

            let mut stdin = slot(stdin, true)?;
            let mut stdout = slot(stdout, false)?;
            let mut stderr = slot(stderr, false)?;

            let mut actions = FileActions::new()?;
            actions.apply(libc::STDIN_FILENO, &stdin.action)?;
            actions.apply(libc::STDOUT_FILENO, &stdout.action)?;
            actions.apply(libc::STDERR_FILENO, &stderr.action)?;
            if let Some(cwd) = &cwd {
                actions.chdir(cwd)?;
            }
            let attributes = Attributes::gnu()?;

            let mut argv_ptrs: Vec<*mut libc::c_char> =
                argv.iter().map(|arg| arg.as_ptr().cast_mut()).collect();
            argv_ptrs.push(std::ptr::null_mut());
            let mut envp_ptrs: Vec<*mut libc::c_char> =
                envp.iter().map(|entry| entry.as_ptr().cast_mut()).collect();
            envp_ptrs.push(std::ptr::null_mut());

            let mut pid: libc::pid_t = 0;
            // SAFETY: every pointer refers to a NUL-terminated string or a
            // NULL-terminated array that outlives the call; the actions and
            // attributes are initialized; `pid` is a valid out-pointer.
            let code = unsafe {
                if search_path {
                    libc::posix_spawnp(
                        &mut pid,
                        file.as_ptr(),
                        &actions.0,
                        &attributes.0,
                        argv_ptrs.as_ptr(),
                        envp_ptrs.as_ptr(),
                    )
                } else {
                    libc::posix_spawn(
                        &mut pid,
                        file.as_ptr(),
                        &actions.0,
                        &attributes.0,
                        argv_ptrs.as_ptr(),
                        envp_ptrs.as_ptr(),
                    )
                }
            };
            check(code)?;

            Ok(SpawnedChild {
                pid: pid as u32,
                stdin: stdin.parent_end.take(),
                stdout: stdout.parent_end.take(),
                stderr: stderr.parent_end.take(),
                backend: ChildBackend::Pid { status: None },
            })
        }
    }
}

#[cfg(all(test, unix))]
#[path = "tests/spawn.rs"]
mod tests;
