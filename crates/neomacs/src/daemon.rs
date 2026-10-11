//! Display-free daemon process lifecycle. Lisp startup.el remains responsible
//! for loading init files, starting the ordinary server and announcing readiness.

use neovm_core::emacs_core::eval::DaemonNotifier;
use std::ffi::OsString;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Options {
    pub(super) background: bool,
    pub(super) name: Option<String>,
}

pub(super) fn parse_option(arg: &str) -> Option<Options> {
    let (flag, name) = arg
        .split_once('=')
        .map_or((arg, None), |(flag, name)| (flag, Some(name)));
    for (short, long, minimum, background) in [
        ("-daemon", "--daemon", 5, true),
        ("-bg-daemon", "--bg-daemon", 10, true),
        ("-fg-daemon", "--fg-daemon", 10, false),
    ] {
        if flag == short && name.is_none()
            || flag.starts_with("--") && flag.len() >= minimum && long.starts_with(flag)
        {
            return Some(Options {
                background,
                name: name.filter(|name| !name.is_empty()).map(str::to_owned),
            });
        }
    }
    None
}

/// Called before starting any runtime threads. Background startup uses a
/// close-on-exec socketpair, so child programs cannot hold the readiness peer.
#[cfg(unix)]
pub(super) fn prepare(options: Option<&Options>) -> Result<Option<DaemonNotifier>, String> {
    use std::io::{Read, Write};
    use std::os::fd::AsRawFd;
    use std::os::unix::net::UnixStream;

    let Some(options) = options else {
        return Ok(None);
    };
    // Retain the forked daemon until this sole startup owner reaps it.
    neovm_core::emacs_core::callproc::retain_child_exit_status()
        .map_err(|error| error.to_string())?;
    if !options.background {
        // Explicit inherited foreground handshakes remain supported. Ordinary
        // automatic clients use the background launcher instead; server
        // requests and the one-shot notification error contract are unchanged.
        let Some(raw) = std::env::var_os("NEOMACS_DAEMON_NOTIFY_FD") else {
            return Ok(None);
        };
        // SAFETY: main calls prepare before starting any runtime threads.
        unsafe { std::env::remove_var("NEOMACS_DAEMON_NOTIFY_FD") };
        let fd = raw
            .to_str()
            .and_then(|raw| raw.parse::<i32>().ok())
            .filter(|fd| *fd > libc::STDERR_FILENO)
            .ok_or("invalid daemon readiness descriptor")?;
        // Duplicate rather than assuming ownership of a user-provided number;
        // the new descriptor is immediately close-on-exec for Lisp subprocesses.
        // SAFETY: fcntl duplicates an inherited descriptor without taking
        // ownership of it; an invalid descriptor reports EBADF.
        let duplicate = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 3) };
        if duplicate < 0 {
            return Err(format!(
                "cannot duplicate daemon readiness descriptor {fd}: {}",
                std::io::Error::last_os_error()
            ));
        }
        use std::os::fd::FromRawFd;
        // SAFETY: fcntl returned a fresh descriptor owned by this function.
        let mut notify = unsafe { UnixStream::from_raw_fd(duplicate) };
        notify
            .peer_addr()
            .map_err(|error| format!("invalid daemon readiness socket descriptor {fd}: {error}"))?;
        // SAFETY: fd is the dedicated inherited socket, validated above.
        unsafe { libc::close(fd) };
        return Ok(Some(Box::new(move || {
            notify
                .write_all(b"\n")
                .map_err(|error| format!("I/O error during daemon initialization: {error}"))
        })));
    }
    // Automatic startup transfers the endpoint lock to this launcher before
    // the requester can abandon its wait. Only the launcher retains it: the
    // evaluator and all Lisp subprocesses must not own this capability.
    let startup_lock = inherited_startup_lock()?;
    let (mut parent, mut child) = UnixStream::pair().map_err(|error| error.to_string())?;
    // SAFETY: this is the early, single-threaded startup boundary, before
    // logging, evaluator workers, native displays or signal-reader threads.
    let pid = unsafe { libc::fork() };
    if pid < 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    if pid > 0 {
        drop(child);
        let mut ready = [0];
        // read_exact retries EINTR. Ordinary GNU startup has no deadline.
        let result = parent.read_exact(&mut ready);
        if result.is_ok() && ready == [b'\n'] {
            drop(startup_lock);
            std::process::exit(0);
        }
        let status = settle_failed_child(pid);
        return Err(format!(
            "daemon failed to initialize: {result:?}, notification {ready:?}; {status}"
        ));
    }
    drop(startup_lock);
    drop(parent);
    // SAFETY: a newly forked child is not a process group leader.
    if unsafe { libc::setsid() } < 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(Some(Box::new(move || {
        let null = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/null")
            .map_err(|error| error.to_string())?;
        for fd in [libc::STDIN_FILENO, libc::STDOUT_FILENO, libc::STDERR_FILENO] {
            // SAFETY: null is an open descriptor, and fd is a standard fd.
            if unsafe { libc::dup2(null.as_raw_fd(), fd) } < 0 {
                return Err(std::io::Error::last_os_error().to_string());
            }
        }
        child
            .write_all(b"\n")
            .map_err(|error| format!("I/O error during daemon initialization: {error}"))
    })))
}

#[cfg(unix)]
fn inherited_startup_lock() -> Result<Option<std::fs::File>, String> {
    use std::os::fd::FromRawFd;
    use std::os::unix::fs::MetadataExt;
    let Some(raw) = std::env::var_os("NEOMACS_DAEMON_LOCK_FD") else {
        return Ok(None);
    };
    // SAFETY: prepare runs before runtime threads or Lisp subprocesses.
    unsafe { std::env::remove_var("NEOMACS_DAEMON_LOCK_FD") };
    let fd = raw
        .to_str()
        .and_then(|raw| raw.parse::<i32>().ok())
        .filter(|fd| *fd > libc::STDERR_FILENO)
        .ok_or("invalid daemon startup lock descriptor")?;
    // SAFETY: duplicate the inherited capability without taking ownership of
    // an unvalidated descriptor number. Only the duplicate is RAII-owned.
    let duplicate = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 3) };
    if duplicate < 0 {
        return Err(format!(
            "cannot duplicate daemon startup lock descriptor {fd}: {}",
            std::io::Error::last_os_error()
        ));
    }
    let lock = unsafe { std::fs::File::from_raw_fd(duplicate) };
    let metadata = lock.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.nlink() != 1
        || metadata.mode() & 0o077 != 0
    {
        return Err("unsafe inherited daemon startup lock".into());
    }
    // SAFETY: the validated dedicated inherited capability is now duplicated.
    unsafe { libc::close(fd) };
    Ok(Some(lock))
}

#[cfg(unix)]
fn settle_failed_child(pid: libc::pid_t) -> String {
    let mut status = 0;
    // Retained child status prevents PID reuse between this ownership probe,
    // signalling and reaping. ECHILD denies signalling rather than guessing.
    loop {
        let result = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
        if result == pid {
            return format!("child wait status {status}");
        }
        if result == 0 {
            unsafe { libc::kill(pid, libc::SIGKILL) };
            break;
        }
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::Interrupted {
            return format!("cannot retain failed child: {error}");
        }
    }
    loop {
        if unsafe { libc::waitpid(pid, &mut status, 0) } == pid {
            return format!("child wait status {status}");
        }
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::Interrupted {
            return format!("cannot reap failed child: {error}");
        }
    }
}

#[cfg(not(unix))]
pub(super) fn prepare(options: Option<&Options>) -> Result<Option<DaemonNotifier>, String> {
    if options.is_some() {
        Err("daemon mode is not supported on this platform".into())
    } else {
        Ok(None)
    }
}

/// Re-exec a daemon in place, retaining its foreground/background identity.
/// A background daemon is already detached: do not fork a second time.
pub(super) fn restart(args: &[OsString], bypass_finalizers: bool) -> ! {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let executable = std::env::current_exe().unwrap_or_else(|error| {
            eprintln!("neomacs: cannot restart: {error}");
            if bypass_finalizers {
                super::exit_cancelled_gui_startup(1);
            }
            std::process::exit(1);
        });
        let mut command = std::process::Command::new(executable);
        if let Some(arg0) = args.first() {
            command.arg0(arg0);
        }
        let mut index = 1;
        while index < args.len() {
            let arg = &args[index];
            if arg == "--" {
                command.args(&args[index..]);
                break;
            }
            if let Some(options) = arg.to_str().and_then(parse_option) {
                command.arg(options.name.map_or_else(
                    || "--fg-daemon".into(),
                    |name| format!("--fg-daemon={name}"),
                ));
                index += 1;
            } else {
                let operands = arg
                    .to_str()
                    .and_then(super::args::classify_standard_arg)
                    .map_or(0, |matched| matched.operands);
                let end = (index + 1 + operands).min(args.len());
                command.args(&args[index..end]);
                index = end;
            }
        }
        let error = command.exec();
        eprintln!("neomacs: cannot restart: {error}");
    }
    if bypass_finalizers {
        super::exit_cancelled_gui_startup(1);
    }
    std::process::exit(1);
}

#[cfg(test)]
#[path = "daemon/tests/daemon_test.rs"]
mod tests;
