//! Pre-booted batch `fork()` zygote server for fast, process-isolated runs.
//!
//! When launched with `--batch-zygote-socket=<path>`, Neomacs loads its final
//! `pdump` image **once** in a strictly single-threaded parent process, then
//! serves requests over a Unix domain socket by calling `libc::fork()`.
//!
//! Each forked child inherits the already-deserialized Lisp heap via OS
//! Copy-on-Write, applies the request's `stdout`/`stderr` files, `cwd`,
//! `umask`, OS environment, `RLIMIT_AS`, and `argv`, and then runs the exact
//! unmodified post-image activation (`activate_runtime_evaluator`), startup
//! setup (`bootstrap_buffers` + `configure_gnu_startup_state`), and GNU
//! `recursive-edit` command loop before exiting via `libc::_exit`.
//!
//! Works on both Linux and macOS (`#[cfg(unix)]`).

use std::path::Path;

use super::RuntimeMode;

#[cfg(not(unix))]
pub(crate) fn run_batch_zygote(
    _socket_path: &Path,
    _mode: RuntimeMode,
    _dump_file_override: Option<&Path>,
) -> ! {
    eprintln!("neomacs: --batch-zygote-socket is only supported on Unix platforms");
    std::process::exit(1);
}

#[cfg(unix)]
pub(crate) fn run_batch_zygote(
    socket_path: &Path,
    mode: RuntimeMode,
    dump_file_override: Option<&Path>,
) -> ! {
    unix_impl::run_batch_zygote(socket_path, mode, dump_file_override)
}

#[cfg(unix)]
mod unix_impl {
    use std::collections::HashMap;
    use std::ffi::{CString, OsStr, OsString};
    use std::io::{self, Read, Write};
    use std::os::fd::{AsRawFd, RawFd};
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
    use std::time::{Duration, Instant};

    use neovm_core::emacs_core::load::RuntimeImageRole;

    use super::super::{
        BOOTSTRAP_CORE_FEATURES, RuntimeMode, initialize_system_locale,
        jemalloc_release_startup_slack, parse_startup_options, render_startup_image_error,
        run_tty_batch_evaluator,
    };

    const WIRE_MAGIC: &[u8; 4] = b"NVZ1";
    const MAX_REQUEST_BYTES: usize = 16 * 1024 * 1024;
    const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 15;
    const POLL_TICK_MS: i32 = 250;

    const RESP_EXITED: u8 = 0;
    const RESP_SIGNALED: u8 = 1;
    const RESP_ERROR: u8 = 2;

    static SIGCHLD_WRITE_FD: AtomicI32 = AtomicI32::new(-1);
    static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

    extern "C" fn sigchld_handler(_sig: libc::c_int) {
        let fd = SIGCHLD_WRITE_FD.load(Ordering::Relaxed);
        if fd >= 0 {
            let byte: u8 = 1;
            // SAFETY: `write` on a non-blocking pipe fd is async-signal-safe.
            let _ = unsafe { libc::write(fd, (&byte as *const u8).cast::<libc::c_void>(), 1) };
        }
    }

    extern "C" fn shutdown_signal_handler(_sig: libc::c_int) {
        SHUTDOWN_REQUESTED.store(true, Ordering::Relaxed);
        let fd = SIGCHLD_WRITE_FD.load(Ordering::Relaxed);
        if fd >= 0 {
            let byte: u8 = 2;
            // SAFETY: `write` on a non-blocking pipe fd is async-signal-safe.
            let _ = unsafe { libc::write(fd, (&byte as *const u8).cast::<libc::c_void>(), 1) };
        }
    }

    struct ZygoteRequest {
        argv: Vec<String>,
        env: Vec<(OsString, OsString)>,
        cwd: PathBuf,
        stdout_path: PathBuf,
        stderr_path: PathBuf,
        umask: u32,
        rlimit_as: u64,
    }

    struct Cursor<'a> {
        buf: &'a [u8],
        pos: usize,
    }

    impl<'a> Cursor<'a> {
        fn new(buf: &'a [u8]) -> Self {
            Self { buf, pos: 0 }
        }

        fn read_u32(&mut self) -> Result<u32, String> {
            let end = self
                .pos
                .checked_add(4)
                .filter(|&e| e <= self.buf.len())
                .ok_or_else(|| "truncated u32 in zygote request".to_string())?;
            let val = u32::from_le_bytes(self.buf[self.pos..end].try_into().unwrap());
            self.pos = end;
            Ok(val)
        }

        fn read_u64(&mut self) -> Result<u64, String> {
            let end = self
                .pos
                .checked_add(8)
                .filter(|&e| e <= self.buf.len())
                .ok_or_else(|| "truncated u64 in zygote request".to_string())?;
            let val = u64::from_le_bytes(self.buf[self.pos..end].try_into().unwrap());
            self.pos = end;
            Ok(val)
        }

        fn read_bytes(&mut self) -> Result<&'a [u8], String> {
            let len = self.read_u32()? as usize;
            let end = self
                .pos
                .checked_add(len)
                .filter(|&e| e <= self.buf.len())
                .ok_or_else(|| "truncated byte slice in zygote request".to_string())?;
            let out = &self.buf[self.pos..end];
            self.pos = end;
            Ok(out)
        }

        fn read_string(&mut self) -> Result<String, String> {
            let bytes = self.read_bytes()?;
            String::from_utf8(bytes.to_vec())
                .map_err(|e| format!("non-UTF-8 string in zygote request: {e}"))
        }

        fn read_os_string(&mut self) -> Result<OsString, String> {
            let bytes = self.read_bytes()?;
            Ok(OsStr::from_bytes(bytes).to_os_string())
        }
    }

    fn read_request(stream: &mut UnixStream) -> Result<ZygoteRequest, String> {
        let mut header = [0u8; 8];
        stream
            .read_exact(&mut header)
            .map_err(|e| format!("failed reading zygote request header: {e}"))?;
        if &header[0..4] != WIRE_MAGIC {
            return Err(format!("bad zygote request magic: {:?}", &header[0..4]));
        }
        let payload_len = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;
        if payload_len > MAX_REQUEST_BYTES {
            return Err(format!("zygote request too large: {payload_len} bytes"));
        }
        let mut payload = vec![0u8; payload_len];
        stream
            .read_exact(&mut payload)
            .map_err(|e| format!("failed reading zygote request payload: {e}"))?;

        let mut cur = Cursor::new(&payload);
        let argc = cur.read_u32()? as usize;
        let mut argv = Vec::with_capacity(argc);
        for _ in 0..argc {
            argv.push(cur.read_string()?);
        }
        let envc = cur.read_u32()? as usize;
        let mut env = Vec::with_capacity(envc);
        for _ in 0..envc {
            let key = cur.read_os_string()?;
            let val = cur.read_os_string()?;
            env.push((key, val));
        }
        let cwd = PathBuf::from(cur.read_os_string()?);
        let stdout_path = PathBuf::from(cur.read_os_string()?);
        let stderr_path = PathBuf::from(cur.read_os_string()?);
        let umask = cur.read_u32()?;
        let rlimit_as = cur.read_u64()?;

        Ok(ZygoteRequest {
            argv,
            env,
            cwd,
            stdout_path,
            stderr_path,
            umask,
            rlimit_as,
        })
    }

    fn write_response(stream: &mut UnixStream, kind: u8, code: i32) {
        let mut resp = [0u8; 9];
        resp[0..4].copy_from_slice(WIRE_MAGIC);
        resp[4] = kind;
        resp[5..9].copy_from_slice(&code.to_le_bytes());
        let _ = stream.write_all(&resp);
    }

    fn create_signal_pipe() -> io::Result<(RawFd, RawFd)> {
        let mut fds = [0 as RawFd; 2];
        // SAFETY: `fds` is a valid 2-element array.
        if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        for fd in fds {
            // SAFETY: valid freshly created pipe fds.
            unsafe {
                let flags = libc::fcntl(fd, libc::F_GETFL);
                if flags >= 0 {
                    libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
                }
                let fd_flags = libc::fcntl(fd, libc::F_GETFD);
                if fd_flags >= 0 {
                    libc::fcntl(fd, libc::F_SETFD, fd_flags | libc::FD_CLOEXEC);
                }
            }
        }
        Ok((fds[0], fds[1]))
    }

    fn drain_pipe(fd: RawFd) {
        let mut buf = [0u8; 64];
        loop {
            // SAFETY: valid non-blocking pipe read fd.
            let n = unsafe { libc::read(fd, buf.as_mut_ptr().cast::<libc::c_void>(), buf.len()) };
            if n <= 0 {
                break;
            }
        }
    }

    fn install_parent_signals(sigchld_w: RawFd) -> io::Result<()> {
        SIGCHLD_WRITE_FD.store(sigchld_w, Ordering::SeqCst);
        // SAFETY: installing simple async-signal-safe handlers in single-threaded parent.
        unsafe {
            let mut sa: libc::sigaction = std::mem::zeroed();
            sa.sa_sigaction = sigchld_handler as *const () as usize;
            libc::sigemptyset(&mut sa.sa_mask);
            sa.sa_flags = libc::SA_NOCLDSTOP;
            if libc::sigaction(libc::SIGCHLD, &sa, std::ptr::null_mut()) != 0 {
                return Err(io::Error::last_os_error());
            }

            let mut sa_term: libc::sigaction = std::mem::zeroed();
            sa_term.sa_sigaction = shutdown_signal_handler as *const () as usize;
            libc::sigemptyset(&mut sa_term.sa_mask);
            sa_term.sa_flags = 0;
            let _ = libc::sigaction(libc::SIGTERM, &sa_term, std::ptr::null_mut());
            let _ = libc::sigaction(libc::SIGINT, &sa_term, std::ptr::null_mut());

            libc::signal(libc::SIGPIPE, libc::SIG_IGN);
        }
        Ok(())
    }

    fn reset_child_signals() {
        // SAFETY: resetting process signals in newly forked single-threaded child.
        unsafe {
            libc::signal(libc::SIGCHLD, libc::SIG_DFL);
            libc::signal(libc::SIGPIPE, libc::SIG_DFL);
            libc::signal(libc::SIGTERM, libc::SIG_DFL);
            libc::signal(libc::SIGINT, libc::SIG_DFL);
        }
    }

    fn assert_single_threaded_before_fork() {
        #[cfg(target_os = "linux")]
        {
            if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
                for line in status.lines() {
                    if let Some(rest) = line.strip_prefix("Threads:") {
                        let count: usize = rest.trim().parse().unwrap_or(0);
                        assert_eq!(
                            count, 1,
                            "batch zygote must be single-threaded before fork(), found {count} threads"
                        );
                        return;
                    }
                }
            }
        }
    }

    fn watch_pid_alive(pid: libc::pid_t) -> bool {
        // SAFETY: kill with signal 0 only checks process existence.
        let rc = unsafe { libc::kill(pid, 0) };
        if rc == 0 {
            return true;
        }
        let err = io::Error::last_os_error();
        err.raw_os_error() != Some(libc::ESRCH)
    }

    fn client_disconnected(fd: RawFd) -> bool {
        let mut byte = 0u8;
        // SAFETY: non-blocking peek on valid connected Unix socket fd.
        let n = unsafe {
            libc::recv(
                fd,
                (&mut byte as *mut u8).cast::<libc::c_void>(),
                1,
                libc::MSG_PEEK | libc::MSG_DONTWAIT,
            )
        };
        if n == 0 {
            return true;
        }
        if n < 0 {
            let err = io::Error::last_os_error().raw_os_error();
            return err != Some(libc::EAGAIN)
                && err != Some(libc::EWOULDBLOCK)
                && err != Some(libc::EINTR);
        }
        false
    }

    fn redirect_fd_to_path(path: &Path, target_fd: RawFd, flags: libc::c_int) -> Result<(), i32> {
        let cpath = CString::new(path.as_os_str().as_bytes()).map_err(|_| libc::EINVAL)?;
        // SAFETY: `cpath` is a valid NUL-terminated path.
        let fd = unsafe { libc::open(cpath.as_ptr(), flags, 0o600 as libc::c_uint) };
        if fd < 0 {
            return Err(io::Error::last_os_error()
                .raw_os_error()
                .unwrap_or(libc::EIO));
        }
        if fd != target_fd {
            // SAFETY: valid open fd and standard target_fd.
            let rc = unsafe { libc::dup2(fd, target_fd) };
            let err = io::Error::last_os_error()
                .raw_os_error()
                .unwrap_or(libc::EIO);
            // SAFETY: closing temporary fd != target_fd.
            unsafe { libc::close(fd) };
            if rc < 0 {
                return Err(err);
            }
        }
        Ok(())
    }

    fn run_child(
        mut evaluator: neovm_core::emacs_core::Context,
        mode: RuntimeMode,
        req: ZygoteRequest,
    ) -> ! {
        let started_at = Instant::now();
        reset_child_signals();

        if redirect_fd_to_path(Path::new("/dev/null"), libc::STDIN_FILENO, libc::O_RDONLY).is_err()
            || redirect_fd_to_path(
                &req.stdout_path,
                libc::STDOUT_FILENO,
                libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC,
            )
            .is_err()
            || redirect_fd_to_path(
                &req.stderr_path,
                libc::STDERR_FILENO,
                libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC,
            )
            .is_err()
        {
            // SAFETY: immediately terminate child on stdio setup failure.
            unsafe { libc::_exit(126) };
        }

        // SAFETY: single-threaded freshly forked child.
        unsafe { libc::umask(req.umask as libc::mode_t) };

        if let Err(err) = std::env::set_current_dir(&req.cwd) {
            eprintln!("neomacs zygote: chdir({}) failed: {err}", req.cwd.display());
            unsafe { libc::_exit(126) };
        }

        // Replace the OS environment completely. Iterating `vars_os()` +
        // `remove_var` is portable across both Linux and macOS (macOS lacks
        // `clearenv`).
        let existing_keys: Vec<OsString> = std::env::vars_os().map(|(k, _)| k).collect();
        for key in existing_keys {
            // SAFETY: strictly single-threaded child process.
            unsafe { std::env::remove_var(&key) };
        }
        for (key, value) in &req.env {
            // SAFETY: strictly single-threaded child process.
            unsafe { std::env::set_var(key, value) };
        }

        // Refresh C locale and timezone tables from the child's environment.
        initialize_system_locale();
        unsafe extern "C" {
            fn tzset();
        }
        // SAFETY: single-threaded child process after setting TZ.
        unsafe { tzset() };

        #[cfg(target_os = "linux")]
        if req.rlimit_as > 0 {
            // SAFETY: standard POSIX setrlimit in child before evaluation.
            unsafe {
                let mut current = libc::rlimit {
                    rlim_cur: 0,
                    rlim_max: 0,
                };
                if libc::getrlimit(libc::RLIMIT_AS, &mut current) == 0 {
                    let target = req.rlimit_as as libc::rlim_t;
                    current.rlim_cur = current.rlim_cur.min(target);
                    let _ = libc::setrlimit(libc::RLIMIT_AS, &current);
                }
            }
        }

        let process_args: Vec<OsString> = req.argv.iter().map(OsString::from).collect();
        let child_startup = match parse_startup_options(req.argv) {
            Ok(s) => s,
            Err(message) => {
                eprintln!("{message}");
                unsafe { libc::_exit(1) };
            }
        };

        neovm_core::emacs_core::load::set_no_site_lisp(child_startup.no_site_lisp);
        let _logging_guard = neovm_core::logging::init(neovm_core::logging::LogTarget::File);
        neovm_core::emacs_core::keyboard::initialize_termination_signals(
            child_startup.noninteractive,
        );

        if let Err(err) = neovm_core::emacs_core::load::activate_runtime_evaluator(
            &mut evaluator,
            RuntimeImageRole::Final,
        ) {
            eprintln!(
                "neomacs zygote: failed to activate final image: {}",
                render_startup_image_error(&err)
            );
            unsafe { libc::_exit(1) };
        }

        let exit_code =
            run_tty_batch_evaluator(evaluator, mode, &child_startup, started_at, &process_args);

        let _ = io::stdout().flush();
        let _ = io::stderr().flush();
        // SAFETY: flush C stdio buffers before `_exit`.
        unsafe {
            libc::fflush(std::ptr::null_mut());
            libc::_exit(exit_code);
        }
    }

    pub(crate) fn run_batch_zygote(
        socket_path: &Path,
        mode: RuntimeMode,
        dump_file_override: Option<&Path>,
    ) -> ! {
        if mode != RuntimeMode::FinalRun {
            eprintln!("neomacs: --batch-zygote-socket requires RuntimeMode::FinalRun");
            std::process::exit(1);
        }

        let watch_pid: Option<libc::pid_t> = std::env::var("NEOMACS_ZYGOTE_WATCH_PID")
            .ok()
            .and_then(|v| v.parse::<libc::pid_t>().ok())
            .filter(|&p| p > 1);
        let idle_timeout = Duration::from_secs(
            std::env::var("NEOMACS_ZYGOTE_IDLE_SECS")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(DEFAULT_IDLE_TIMEOUT_SECS),
        );

        // Load the final pdump image ONCE before binding the socket so any
        // connecting client is guaranteed to hit a warm, ready zygote.
        let evaluator =
            neovm_core::emacs_core::load_boot::load_unactivated_runtime_image_with_features(
                RuntimeImageRole::Final,
                BOOTSTRAP_CORE_FEATURES,
                dump_file_override,
            )
            .unwrap_or_else(|err| {
                eprintln!(
                    "neomacs zygote: failed to load final image: {}",
                    render_startup_image_error(&err)
                );
                std::process::exit(1);
            });
        jemalloc_release_startup_slack();

        assert_single_threaded_before_fork();

        let (sigchld_r, sigchld_w) = create_signal_pipe().unwrap_or_else(|err| {
            eprintln!("neomacs zygote: failed to create signal pipe: {err}");
            std::process::exit(1);
        });
        if let Err(err) = install_parent_signals(sigchld_w) {
            eprintln!("neomacs zygote: failed to install signal handlers: {err}");
            std::process::exit(1);
        }

        // Bind to a temporary sibling path then atomically rename into place
        // so clients never observe an unbound or half-bound socket.
        let tmp_socket = socket_path.with_extension(format!("tmp.{}", std::process::id()));
        let _ = std::fs::remove_file(&tmp_socket);
        let listener = UnixListener::bind(&tmp_socket).unwrap_or_else(|err| {
            eprintln!(
                "neomacs zygote: failed to bind {}: {err}",
                tmp_socket.display()
            );
            std::process::exit(1);
        });
        if let Err(err) = std::fs::rename(&tmp_socket, socket_path) {
            let _ = std::fs::remove_file(&tmp_socket);
            eprintln!(
                "neomacs zygote: failed to publish socket {}: {err}",
                socket_path.display()
            );
            std::process::exit(1);
        }
        let _ = listener.set_nonblocking(true);

        let listener_fd = listener.as_raw_fd();
        // SAFETY: prevent children from accidentally inheriting the listener if exec occurs.
        unsafe {
            let flags = libc::fcntl(listener_fd, libc::F_GETFD);
            if flags >= 0 {
                libc::fcntl(listener_fd, libc::F_SETFD, flags | libc::FD_CLOEXEC);
            }
        }

        let mut children: HashMap<libc::pid_t, UnixStream> = HashMap::new();
        let mut last_activity = Instant::now();

        loop {
            if SHUTDOWN_REQUESTED.load(Ordering::Relaxed) && children.is_empty() {
                break;
            }

            let mut pollfds = Vec::with_capacity(2 + children.len());
            pollfds.push(libc::pollfd {
                fd: listener_fd,
                events: libc::POLLIN,
                revents: 0,
            });
            pollfds.push(libc::pollfd {
                fd: sigchld_r,
                events: libc::POLLIN,
                revents: 0,
            });
            let active_pids: Vec<libc::pid_t> = children.keys().copied().collect();
            for &pid in &active_pids {
                pollfds.push(libc::pollfd {
                    fd: children[&pid].as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                });
            }

            // SAFETY: `pollfds` is a valid contiguous slice of `pollfd`.
            let rc = unsafe {
                libc::poll(
                    pollfds.as_mut_ptr(),
                    pollfds.len() as libc::nfds_t,
                    POLL_TICK_MS,
                )
            };
            if rc < 0 {
                let err = io::Error::last_os_error();
                if err.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                break;
            }

            // 1. Reap exited children.
            if (pollfds[1].revents & libc::POLLIN) != 0 || rc == 0 {
                drain_pipe(sigchld_r);
                loop {
                    let mut status: libc::c_int = 0;
                    // SAFETY: non-blocking waitpid for any child.
                    let pid = unsafe { libc::waitpid(-1, &mut status, libc::WNOHANG) };
                    if pid <= 0 {
                        break;
                    }
                    last_activity = Instant::now();
                    if let Some(mut stream) = children.remove(&pid) {
                        if libc::WIFEXITED(status) {
                            write_response(&mut stream, RESP_EXITED, libc::WEXITSTATUS(status));
                        } else if libc::WIFSIGNALED(status) {
                            write_response(&mut stream, RESP_SIGNALED, libc::WTERMSIG(status));
                        } else {
                            write_response(&mut stream, RESP_ERROR, status);
                        }
                    }
                }
            }

            // 2. Kill any child whose client disconnected (e.g. nextest timeout).
            for (idx, &pid) in active_pids.iter().enumerate() {
                let revents = pollfds[2 + idx].revents;
                if revents != 0
                    && let Some(stream) = children.get(&pid)
                    && client_disconnected(stream.as_raw_fd())
                {
                    // SAFETY: `pid` is our live unreaped child.
                    unsafe { libc::kill(pid, libc::SIGKILL) };
                }
            }

            // 3. Accept all queued new connections.
            if (pollfds[0].revents & libc::POLLIN) != 0 {
                loop {
                    let mut stream = match listener.accept() {
                        Ok((s, _)) => s,
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                        Err(_) => break,
                    };
                    let _ = stream.set_nonblocking(false);
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                    last_activity = Instant::now();
                    let req = match read_request(&mut stream) {
                        Ok(req) => req,
                        Err(_) => {
                            write_response(&mut stream, RESP_ERROR, libc::EINVAL);
                            continue;
                        }
                    };
                    let _ = stream.set_read_timeout(None);

                    assert_single_threaded_before_fork();
                    // SAFETY: parent is strictly single-threaded; child never returns here.
                    let pid = unsafe { libc::fork() };
                    if pid < 0 {
                        let errno = io::Error::last_os_error()
                            .raw_os_error()
                            .unwrap_or(libc::EAGAIN);
                        write_response(&mut stream, RESP_ERROR, errno);
                        continue;
                    }
                    if pid == 0 {
                        // Child: close all server/peer FDs before running Lisp.
                        drop(stream);
                        drop(children);
                        drop(listener);
                        unsafe {
                            libc::close(sigchld_r);
                            libc::close(sigchld_w);
                        }
                        run_child(evaluator, mode, req);
                    }
                    children.insert(pid, stream);
                }
            }

            // 4. Clean idle / watched-parent shutdown when no children are running.
            if children.is_empty() {
                if let Some(wpid) = watch_pid
                    && !watch_pid_alive(wpid)
                {
                    break;
                }
                if last_activity.elapsed() >= idle_timeout {
                    break;
                }
            }
        }

        // Reap any remaining children on signal shutdown.
        for &pid in children.keys() {
            unsafe { libc::kill(pid, libc::SIGKILL) };
        }
        let _ = std::fs::remove_file(socket_path);
        std::mem::forget(evaluator);
        unsafe { libc::_exit(0) };
    }
}
