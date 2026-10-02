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
mod tests {
    use super::*;
    use neovm_core::emacs_core::{Context, Value};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    #[test]
    fn daemon_options_use_optional_inline_names_and_gnu_prefixes() {
        for flag in [
            "--daemon",
            "--dae",
            "-daemon",
            "--bg-daemon",
            "-bg-daemon",
            "--daemon=",
        ] {
            let options = parse_option(flag).unwrap();
            assert!(options.background, "{flag}");
            assert_eq!(options.name, None);
        }
        assert_eq!(
            parse_option("--fg-daemon=work"),
            Some(Options {
                background: false,
                name: Some("work".into())
            })
        );
        assert_eq!(parse_option("--da"), None);
        assert_eq!(parse_option("--fg-dae"), None);
        assert_eq!(parse_option("--daemon-other"), None);
        assert_eq!(parse_option("-daemon=work"), None);
    }

    #[test]
    fn daemon_startup_keeps_action_operands_and_normal_modes_distinct() {
        let startup = super::super::parse_startup_options(
            [
                "neomacs",
                "--eval",
                "\"--daemon=operand\"",
                "--fg-daemon=work",
            ]
            .map(str::to_owned),
        )
        .unwrap();
        assert_eq!(
            startup.daemon,
            Some(Options {
                background: false,
                name: Some("work".into())
            })
        );
        assert!(!startup.noninteractive);
        assert_eq!(startup.frontend, super::super::FrontendKind::Tty);
        assert_eq!(
            startup.forwarded_args,
            vec!["neomacs", "--eval", "\"--daemon=operand\""]
        );
        let batch =
            super::super::parse_startup_options(["neomacs", "--batch"].map(str::to_owned)).unwrap();
        assert!(batch.noninteractive && batch.daemon.is_none());
        let ordinary = super::super::parse_startup_options(["neomacs"].map(str::to_owned)).unwrap();
        assert_eq!(ordinary.frontend, super::super::FrontendKind::Gui);
        assert!(ordinary.daemon.is_none());
        let literal =
            super::super::parse_startup_options(["neomacs", "--", "--daemon"].map(str::to_owned))
                .unwrap();
        assert!(literal.daemon.is_none());
        assert_eq!(literal.forwarded_args, vec!["neomacs", "--", "--daemon"]);
    }

    #[test]
    fn daemon_initialized_consumes_a_failing_notifier_and_rejects_retry() {
        let mut eval = Context::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let notified = Arc::clone(&calls);
        eval.configure_daemon(
            None,
            Some(Box::new(move || {
                if notified.fetch_add(1, Ordering::SeqCst) == 0 {
                    Err("I/O error during daemon initialization: fail-once".into())
                } else {
                    Ok(())
                }
            })),
        );
        eval.set_variable("after-init-time", Value::T);
        // GNU src/emacs.c marks initialization consumed before reporting I/O
        // failure. A notifier that could succeed on retry must still run once.
        let result = "(condition-case err (daemon-initialized) (error (car (cdr err))))";
        assert_eq!(
            eval.eval_str(result).unwrap().as_utf8_str(),
            Some("I/O error during daemon initialization: fail-once")
        );
        assert_eq!(
            eval.eval_str(result).unwrap().as_utf8_str(),
            Some("The daemon has already been initialized")
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(eval.eval_str("(daemonp)").unwrap(), Value::T);
    }

    #[test]
    fn daemon_identity_and_initialization_are_host_owned_and_one_shot() {
        let mut eval = Context::new();
        assert_eq!(eval.eval_str("(daemonp)").unwrap(), Value::NIL);
        assert!(eval.eval_str("(daemon-initialized)").is_err());
        let calls = Arc::new(AtomicUsize::new(0));
        let notified = Arc::clone(&calls);
        eval.configure_daemon(
            Some("work".into()),
            Some(Box::new(move || {
                notified.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })),
        );
        eval.set_variable("after-init-time", Value::NIL);
        assert_eq!(
            eval.eval_str("(daemonp)").unwrap().as_utf8_str(),
            Some("work")
        );
        assert!(eval.eval_str("(daemon-initialized)").is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        eval.set_variable("after-init-time", Value::T);
        assert_eq!(eval.eval_str("(daemon-initialized)").unwrap(), Value::T);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(eval.eval_str("(daemon-initialized)").is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            eval.eval_str("(daemonp)").unwrap().as_utf8_str(),
            Some("work")
        );
        let mut unnamed = Context::new();
        unnamed.configure_daemon(None, None);
        assert_eq!(unnamed.eval_str("(daemonp)").unwrap(), Value::T);
    }
}
