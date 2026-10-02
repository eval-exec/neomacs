//! Bounded automatic startup for the ordinary local Emacs server protocol.
use std::fs;
use std::io::{self, Read};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// A full listener backlog must not make a bounded startup block in connect,
/// or be mistaken for an absent server and start a duplicate daemon.
pub(super) fn try_connect(path: &Path) -> io::Result<UnixStream> {
    let socket = socket2::Socket::new(socket2::Domain::UNIX, socket2::Type::STREAM, None)?;
    socket.set_nonblocking(true)?;
    socket.connect(&socket2::SockAddr::unix(path)?)?;
    socket.set_nonblocking(false)?;
    Ok(socket.into())
}

/// Serialize cold starts for one endpoint. The persistent flock file avoids
/// unlink/recreate races between waiting clients; it carries no server state.
pub(super) fn start_and_connect(
    prog: &str,
    socket: &Path,
    name: Option<&str>,
    timeout: Option<Duration>,
) -> Result<UnixStream, String> {
    let deadline = Instant::now()
        .checked_add(
            timeout
                .filter(|timeout| !timeout.is_zero())
                .unwrap_or(Duration::from_secs(60)),
        )
        .ok_or_else(|| format!("{prog}: startup timeout is too large"))?;
    let mut lock_name = socket.as_os_str().to_os_string();
    lock_name.push(".startup-lock");
    let lock_path = PathBuf::from(lock_name);
    // A live server does not need directory creation rights. Nevertheless,
    // honor an existing starter's lock before submitting to an early listener.
    let mut lock = open_startup_lock(prog, &lock_path, false)?;
    loop {
        if let Some(lock) = &lock {
            wait_for_startup_lock(prog, lock, deadline)?;
        }
        let Some(stream) = connect_existing(prog, socket, deadline)? else {
            break;
        };
        if lock.is_none() {
            // A cold starter may have created its persistent lock between
            // our first lookup and connect. Do not bypass its readiness wait.
            lock = open_startup_lock(prog, &lock_path, false)?;
            if lock.is_some() {
                drop(stream);
                continue;
            }
        }
        return Ok(stream);
    }
    let parent = socket
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)
        .map_err(|error| format!("{prog}: cannot create server directory: {error}"))?;
    let metadata = fs::symlink_metadata(parent).map_err(|error| error.to_string())?;
    // SAFETY: geteuid has no pointer arguments and no preconditions.
    let uid = unsafe { libc::geteuid() };
    if !metadata.is_dir() || metadata.uid() != uid || metadata.mode() & 0o077 != 0 {
        return Err(format!(
            "{prog}: unsafe server directory: {}",
            parent.display()
        ));
    }
    if lock.is_none() {
        lock = open_startup_lock(prog, &lock_path, true)?;
        if let Some(lock) = &lock {
            wait_for_startup_lock(prog, lock, deadline)?;
        }
        // Another client may have completed startup while we waited.
        if let Some(stream) = connect_existing(prog, socket, deadline)? {
            return Ok(stream);
        }
    }
    if Instant::now() >= deadline {
        return Err(format!("{prog}: timed out waiting for daemon startup"));
    }
    let executable = std::env::current_exe()
        .map_err(|error| error.to_string())?
        .with_file_name("neomacs");
    let (mut readiness, notify) = UnixStream::pair().map_err(|error| error.to_string())?;
    readiness
        .set_nonblocking(true)
        .map_err(|error| error.to_string())?;
    let notify_fd = notify.as_raw_fd();
    let mut command = Command::new(executable);
    command
        .env("NEOMACS_DAEMON_NOTIFY_FD", notify_fd.to_string())
        .arg(name.map_or_else(
            || "--fg-daemon".to_owned(),
            |name| format!("--fg-daemon={name}"),
        ))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // SAFETY: setsid and fcntl are async-signal-safe and allocate nothing in
    // the post-fork child. notify remains owned and live through spawn.
    unsafe {
        command.pre_exec(move || {
            if libc::setsid() < 0 || libc::fcntl(notify_fd, libc::F_SETFD, 0) < 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
    }
    neovm_core::emacs_core::callproc::retain_child_exit_status()
        .map_err(|error| format!("{prog}: cannot retain daemon child: {error}"))?;
    let mut child = command
        .spawn()
        .map_err(|error| format!("{prog}: cannot start Neomacs daemon: {error}"))?;
    drop(notify);
    let mut initialized = false;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Err(format!("{prog}: daemon startup failed: {status}")),
            Err(error) => {
                // ECHILD means ownership was lost, not that this PID is safe.
                if error.raw_os_error() != Some(libc::ECHILD) {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                return Err(format!("{prog}: cannot wait for daemon: {error}"));
            }
            Ok(None) => {}
        }
        if !initialized {
            let mut byte = [0];
            match readiness.read(&mut byte) {
                Ok(1) if byte == [b'\n'] => initialized = true,
                Err(error)
                    if error.kind() == io::ErrorKind::WouldBlock
                        || error.kind() == io::ErrorKind::Interrupted => {}
                result => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "{prog}: daemon startup failed before initialization: {result:?}"
                    ));
                }
            }
        }
        if initialized && let Ok(stream) = try_connect(socket) {
            return Ok(stream);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("{prog}: timed out waiting for daemon startup"));
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn open_startup_lock(prog: &str, path: &Path, create: bool) -> Result<Option<fs::File>, String> {
    let lock = match fs::OpenOptions::new()
        .read(true)
        .write(create)
        .create(create)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)
    {
        Ok(lock) => lock,
        Err(error) if !create && error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("{prog}: cannot lock daemon startup: {error}")),
    };
    let metadata = lock.metadata().map_err(|error| error.to_string())?;
    // SAFETY: geteuid has no pointer arguments and no preconditions.
    let uid = unsafe { libc::geteuid() };
    if !metadata.is_file()
        || metadata.uid() != uid
        || metadata.nlink() != 1
        || metadata.mode() & 0o077 != 0
    {
        return Err(format!("{prog}: unsafe daemon startup lock"));
    }
    Ok(Some(lock))
}

fn wait_for_startup_lock(prog: &str, lock: &fs::File, deadline: Instant) -> Result<(), String> {
    loop {
        // SAFETY: the owned File remains open for this entire operation.
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::WouldBlock && error.kind() != io::ErrorKind::Interrupted {
            return Err(format!("{prog}: cannot lock daemon startup: {error}"));
        }
        // A bound socket is not readiness: the competing starter retains its
        // lock until the exact daemon-initialized handshake has completed.
        if Instant::now() >= deadline {
            return Err(format!("{prog}: timed out waiting for daemon startup"));
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn connect_existing(
    prog: &str,
    socket: &Path,
    deadline: Instant,
) -> Result<Option<UnixStream>, String> {
    loop {
        match try_connect(socket) {
            Ok(stream) => return Ok(Some(stream)),
            Err(error)
                if error.kind() == io::ErrorKind::WouldBlock
                    || error.raw_os_error() == Some(libc::EINPROGRESS) => {}
            Err(_) => return Ok(None),
        }
        if Instant::now() >= deadline {
            return Err(format!("{prog}: timed out connecting to existing daemon"));
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}
