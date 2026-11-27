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
    use std::os::unix::fs::PermissionsExt;
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
    const REQUEST_READ_DEADLINE: Duration = Duration::from_secs(5);
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

        fn remaining(&self) -> usize {
            self.buf.len().saturating_sub(self.pos)
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

    fn parse_request_payload(payload: &[u8]) -> Result<ZygoteRequest, String> {
        let mut cur = Cursor::new(payload);

        // Each argv element requires at least a 4-byte length prefix; validate
        // before reserving so a malformed frame cannot trigger huge allocations.
        let argc = cur.read_u32()? as usize;
        if argc > cur.remaining() / 4 {
            return Err(format!(
                "invalid argc {argc} for remaining payload ({} bytes)",
                cur.remaining()
            ));
        }
        let mut argv = Vec::new();
        argv.try_reserve_exact(argc)
            .map_err(|e| format!("failed to reserve argv ({argc}): {e}"))?;
        for _ in 0..argc {
            argv.push(cur.read_string()?);
        }

        // Each env entry requires at least two 4-byte length prefixes (8 bytes).
        let envc = cur.read_u32()? as usize;
        if envc > cur.remaining() / 8 {
            return Err(format!(
                "invalid envc {envc} for remaining payload ({} bytes)",
                cur.remaining()
            ));
        }
        let mut env = Vec::new();
        env.try_reserve_exact(envc)
            .map_err(|e| format!("failed to reserve env ({envc}): {e}"))?;
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

    enum ReadProgress {
        Incomplete,
        Complete(ZygoteRequest),
        Failed(i32),
    }

    /// Incremental, non-blocking request reader so a stalled or slow client
    /// never blocks the single-threaded parent's child supervision loop.
    struct PendingClient {
        stream: UnixStream,
        header: [u8; 8],
        header_read: usize,
        payload: Vec<u8>,
        payload_len: usize,
        payload_read: usize,
        deadline: Instant,
    }

    impl PendingClient {
        fn new(stream: UnixStream) -> Self {
            Self {
                stream,
                header: [0u8; 8],
                header_read: 0,
                payload: Vec::new(),
                payload_len: 0,
                payload_read: 0,
                deadline: Instant::now() + REQUEST_READ_DEADLINE,
            }
        }

        fn try_advance(&mut self) -> ReadProgress {
            while self.header_read < self.header.len() {
                match self.stream.read(&mut self.header[self.header_read..]) {
                    Ok(0) => return ReadProgress::Failed(libc::EINVAL),
                    Ok(n) => self.header_read += n,
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        return ReadProgress::Incomplete;
                    }
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(e) => {
                        return ReadProgress::Failed(e.raw_os_error().unwrap_or(libc::EIO));
                    }
                }
            }

            if self.payload.is_empty() && self.payload_len == 0 {
                if &self.header[0..4] != WIRE_MAGIC {
                    return ReadProgress::Failed(libc::EINVAL);
                }
                let len = u32::from_le_bytes(self.header[4..8].try_into().unwrap()) as usize;
                if len == 0 || len > MAX_REQUEST_BYTES {
                    return ReadProgress::Failed(libc::EINVAL);
                }
                if self.payload.try_reserve_exact(len).is_err() {
                    return ReadProgress::Failed(libc::ENOMEM);
                }
                self.payload.resize(len, 0);
                self.payload_len = len;
            }

            while self.payload_read < self.payload_len {
                match self.stream.read(&mut self.payload[self.payload_read..]) {
                    Ok(0) => return ReadProgress::Failed(libc::EINVAL),
                    Ok(n) => self.payload_read += n,
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        return ReadProgress::Incomplete;
                    }
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(e) => {
                        return ReadProgress::Failed(e.raw_os_error().unwrap_or(libc::EIO));
                    }
                }
            }

            match parse_request_payload(&self.payload) {
                Ok(req) => ReadProgress::Complete(req),
                Err(_) => ReadProgress::Failed(libc::EINVAL),
            }
        }
    }

    fn verify_peer_same_uid(stream: &UnixStream) -> bool {
        // SAFETY: `geteuid` has no preconditions.
        let expected_uid = unsafe { libc::geteuid() };
        let fd = stream.as_raw_fd();
        #[cfg(target_os = "linux")]
        {
            let mut cred = libc::ucred {
                pid: 0,
                uid: u32::MAX,
                gid: u32::MAX,
            };
            let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
            // SAFETY: valid connected Unix socket fd and properly sized `ucred`.
            let rc = unsafe {
                libc::getsockopt(
                    fd,
                    libc::SOL_SOCKET,
                    libc::SO_PEERCRED,
                    (&mut cred as *mut libc::ucred).cast::<libc::c_void>(),
                    &mut len,
                )
            };
            rc == 0 && cred.uid == expected_uid
        }
        #[cfg(target_os = "macos")]
        {
            let mut euid: libc::uid_t = u32::MAX;
            let mut egid: libc::gid_t = u32::MAX;
            // SAFETY: valid connected Unix socket fd and output pointers.
            let rc = unsafe { libc::getpeereid(fd, &mut euid, &mut egid) };
            rc == 0 && euid == expected_uid
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            let _ = (expected_uid, fd);
            true
        }
    }

    fn write_response(stream: &mut UnixStream, kind: u8, code: i32) {
        let mut resp = [0u8; 9];
        resp[0..4].copy_from_slice(WIRE_MAGIC);
        resp[4] = kind;
        resp[5..9].copy_from_slice(&code.to_le_bytes());
        let _ = stream.set_nonblocking(false);
        let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
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
        // SAFETY: resetting parent-installed signal handlers in newly forked
        // single-threaded child. Keep SIGPIPE as SIG_IGN to match Rust's
        // normal `main()` runtime initialization (`std::sys::pal::unix::init`).
        unsafe {
            libc::signal(libc::SIGCHLD, libc::SIG_DFL);
            libc::signal(libc::SIGPIPE, libc::SIG_IGN);
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

        // Reset C locale back to "C" first so `initialize_system_locale()`'s
        // `LC_ALL == "C"` fast path starts from the same baseline as a fresh
        // `execve()` process even when the zygote parent booted in a non-C locale.
        // SAFETY: single-threaded child process.
        unsafe {
            libc::setlocale(libc::LC_ALL, c"C".as_ptr());
        }
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

    fn advance_pending_clients(
        pending: &mut Vec<PendingClient>,
        revents: &[libc::c_short],
        now: Instant,
        ready_requests: &mut Vec<(UnixStream, ZygoteRequest)>,
    ) {
        debug_assert_eq!(pending.len(), revents.len());
        // Traverse original pending slots in reverse so `swap_remove(idx)`
        // never shifts lower indices and `pending[0..idx]` stays 1:1 paired
        // with `revents[0..idx]`.
        for idx in (0..revents.len()).rev() {
            if now >= pending[idx].deadline {
                let mut client = pending.swap_remove(idx);
                write_response(&mut client.stream, RESP_ERROR, libc::ETIMEDOUT);
                continue;
            }
            if revents[idx] == 0 {
                continue;
            }
            match pending[idx].try_advance() {
                ReadProgress::Incomplete => {
                    if Instant::now() >= pending[idx].deadline {
                        let mut client = pending.swap_remove(idx);
                        write_response(&mut client.stream, RESP_ERROR, libc::ETIMEDOUT);
                    }
                }
                ReadProgress::Complete(req) => {
                    let mut client = pending.swap_remove(idx);
                    if Instant::now() >= client.deadline {
                        write_response(&mut client.stream, RESP_ERROR, libc::ETIMEDOUT);
                    } else {
                        ready_requests.push((client.stream, req));
                    }
                }
                ReadProgress::Failed(errno) => {
                    let mut client = pending.swap_remove(idx);
                    write_response(&mut client.stream, RESP_ERROR, errno);
                }
            }
        }
    }

    const MAX_SOCKET_WITHDRAW_RETRIES: u8 = 3;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum ServerLifecycle {
        /// Actively polling the listener and accepting new connections.
        Accepting { withdraw_failures: u8 },
        /// Published socket path has been removed; draining already-accepted
        /// clients and exiting once none remain.
        Draining,
        /// Unlinking the socket failed persistently after bounded retries;
        /// stop polling the listener and exit once active work finishes.
        ExitingWithoutUnlink,
    }

    impl ServerLifecycle {
        fn is_accepting(self) -> bool {
            matches!(self, Self::Accepting { .. })
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum IdleShutdownDecision {
        /// Queued work arrived or socket withdrawal failed transiently; keep looping.
        Continue,
        /// Socket withdrawn (or retries exhausted) with no remaining work; exit now.
        ExitNow,
    }

    fn accept_queued_clients(
        listener: &UnixListener,
        pending: &mut Vec<PendingClient>,
        ready_requests: &mut Vec<(UnixStream, ZygoteRequest)>,
        last_activity: &mut Instant,
    ) {
        loop {
            let mut stream = match listener.accept() {
                Ok((s, _)) => s,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(_) => break,
            };
            if !verify_peer_same_uid(&stream) {
                write_response(&mut stream, RESP_ERROR, libc::EACCES);
                continue;
            }
            let _ = stream.set_nonblocking(true);
            *last_activity = Instant::now();
            let mut client = PendingClient::new(stream);
            match client.try_advance() {
                ReadProgress::Incomplete => {
                    if Instant::now() >= client.deadline {
                        write_response(&mut client.stream, RESP_ERROR, libc::ETIMEDOUT);
                    } else {
                        pending.push(client);
                    }
                }
                ReadProgress::Complete(req) => {
                    if Instant::now() >= client.deadline {
                        write_response(&mut client.stream, RESP_ERROR, libc::ETIMEDOUT);
                    } else {
                        ready_requests.push((client.stream, req));
                    }
                }
                ReadProgress::Failed(errno) => {
                    write_response(&mut client.stream, RESP_ERROR, errno);
                }
            }
        }
    }

    fn try_withdraw_for_idle_shutdown(
        socket_path: &Path,
        listener: &UnixListener,
        pending: &mut Vec<PendingClient>,
        ready_requests: &mut Vec<(UnixStream, ZygoteRequest)>,
        last_activity: &mut Instant,
        lifecycle: &mut ServerLifecycle,
    ) -> IdleShutdownDecision {
        let ServerLifecycle::Accepting { withdraw_failures } = *lifecycle else {
            return if pending.is_empty() && ready_requests.is_empty() {
                IdleShutdownDecision::ExitNow
            } else {
                IdleShutdownDecision::Continue
            };
        };

        // 1. Check for connections queued after `poll` timed out before
        // deciding to withdraw the socket or exit.
        accept_queued_clients(listener, pending, ready_requests, last_activity);
        if !pending.is_empty() || !ready_requests.is_empty() {
            return IdleShutdownDecision::Continue;
        }

        // 2. Withdraw the published socket path so no new clients can connect.
        // Only enter `Draining` if removal succeeded or the path is already
        // absent; otherwise report the error, keep accepting, and retry with a
        // bounded policy.
        match std::fs::remove_file(socket_path) {
            Ok(()) => {
                *lifecycle = ServerLifecycle::Draining;
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                *lifecycle = ServerLifecycle::Draining;
            }
            Err(err) => {
                eprintln!(
                    "neomacs zygote: failed to unlink {}: {err}",
                    socket_path.display()
                );
                let next_failures = withdraw_failures.saturating_add(1);
                if next_failures >= MAX_SOCKET_WITHDRAW_RETRIES {
                    *lifecycle = ServerLifecycle::ExitingWithoutUnlink;
                    accept_queued_clients(listener, pending, ready_requests, last_activity);
                    return if pending.is_empty() && ready_requests.is_empty() {
                        IdleShutdownDecision::ExitNow
                    } else {
                        IdleShutdownDecision::Continue
                    };
                }
                *lifecycle = ServerLifecycle::Accepting {
                    withdraw_failures: next_failures,
                };
                return IdleShutdownDecision::Continue;
            }
        }

        // 3. Drain any client that connected just before unlink.
        accept_queued_clients(listener, pending, ready_requests, last_activity);
        if pending.is_empty() && ready_requests.is_empty() {
            IdleShutdownDecision::ExitNow
        } else {
            IdleShutdownDecision::Continue
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

        // Bind to a temporary sibling path with owner-only mode 0600, then
        // atomically publish with `hard_link` (which fails with `EEXIST` if
        // `socket_path` is already occupied instead of replacing an existing
        // file). Always remove `tmp_socket` afterward.
        let mut tmp_os = socket_path.as_os_str().to_os_string();
        tmp_os.push(format!(".t{}", std::process::id()));
        let tmp_socket = PathBuf::from(tmp_os);
        let _ = std::fs::remove_file(&tmp_socket);
        let listener = UnixListener::bind(&tmp_socket).unwrap_or_else(|err| {
            eprintln!(
                "neomacs zygote: failed to bind {}: {err}",
                tmp_socket.display()
            );
            std::process::exit(1);
        });
        let _ = std::fs::set_permissions(&tmp_socket, std::fs::Permissions::from_mode(0o600));
        let publish_res = std::fs::hard_link(&tmp_socket, socket_path);
        let _ = std::fs::remove_file(&tmp_socket);
        if let Err(err) = publish_res {
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

        let mut pending: Vec<PendingClient> = Vec::new();
        let mut ready_requests: Vec<(UnixStream, ZygoteRequest)> = Vec::new();
        let mut children: HashMap<libc::pid_t, UnixStream> = HashMap::new();
        let mut last_activity = Instant::now();
        let mut lifecycle = ServerLifecycle::Accepting {
            withdraw_failures: 0,
        };

        loop {
            if ready_requests.is_empty()
                && children.is_empty()
                && pending.is_empty()
                && (!lifecycle.is_accepting() || SHUTDOWN_REQUESTED.load(Ordering::Relaxed))
            {
                break;
            }

            if ready_requests.is_empty() {
                let accepting = lifecycle.is_accepting();
                let mut pollfds = Vec::with_capacity(2 + pending.len() + children.len());
                if accepting {
                    pollfds.push(libc::pollfd {
                        fd: listener_fd,
                        events: libc::POLLIN,
                        revents: 0,
                    });
                }
                let sigchld_idx = pollfds.len();
                pollfds.push(libc::pollfd {
                    fd: sigchld_r,
                    events: libc::POLLIN,
                    revents: 0,
                });
                let pending_start = pollfds.len();
                for client in &pending {
                    pollfds.push(libc::pollfd {
                        fd: client.stream.as_raw_fd(),
                        events: libc::POLLIN,
                        revents: 0,
                    });
                }
                let children_start = pollfds.len();
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
                if (pollfds[sigchld_idx].revents & libc::POLLIN) != 0 || rc == 0 {
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
                    let revents = pollfds[children_start + idx].revents;
                    if revents != 0
                        && let Some(stream) = children.get(&pid)
                        && client_disconnected(stream.as_raw_fd())
                    {
                        // SAFETY: `pid` is our live unreaped child.
                        unsafe { libc::kill(pid, libc::SIGKILL) };
                    }
                }

                // 3. Advance non-blocking pending request readers without blocking supervision.
                let pending_revents: Vec<libc::c_short> = pollfds[pending_start..children_start]
                    .iter()
                    .map(|pfd| pfd.revents)
                    .collect();
                advance_pending_clients(
                    &mut pending,
                    &pending_revents,
                    Instant::now(),
                    &mut ready_requests,
                );

                // 4. Accept newly queued connections (non-blocking) and try an immediate read.
                if accepting && (pollfds[0].revents & libc::POLLIN) != 0 {
                    accept_queued_clients(
                        &listener,
                        &mut pending,
                        &mut ready_requests,
                        &mut last_activity,
                    );
                }
            }

            // 5. Fork one isolated child per completed request.
            let mut ready = std::mem::take(&mut ready_requests).into_iter();
            while let Some((mut stream, req)) = ready.next() {
                last_activity = Instant::now();
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
                    // Child: close all server/peer FDs (including remaining
                    // queued requests) before running Lisp.
                    drop(stream);
                    drop(ready);
                    drop(pending);
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

            // 6. Clean idle / watched-parent shutdown when no work is in flight.
            if lifecycle.is_accepting() && children.is_empty() && pending.is_empty() {
                let watch_dead = watch_pid.is_some_and(|wpid| !watch_pid_alive(wpid));
                let idle_expired = last_activity.elapsed() >= idle_timeout;
                if (watch_dead || idle_expired)
                    && try_withdraw_for_idle_shutdown(
                        socket_path,
                        &listener,
                        &mut pending,
                        &mut ready_requests,
                        &mut last_activity,
                        &mut lifecycle,
                    ) == IdleShutdownDecision::ExitNow
                {
                    break;
                }
            }
        }

        // Reap any remaining children on signal shutdown.
        for &pid in children.keys() {
            unsafe { libc::kill(pid, libc::SIGKILL) };
        }
        if lifecycle.is_accepting() {
            let _ = std::fs::remove_file(socket_path);
        }
        std::mem::forget(evaluator);
        unsafe { libc::_exit(0) };
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn encode_minimal_frame() -> Vec<u8> {
            let mut payload = Vec::new();
            // argc = 1 ("neomacs")
            payload.extend_from_slice(&1u32.to_le_bytes());
            payload.extend_from_slice(&7u32.to_le_bytes());
            payload.extend_from_slice(b"neomacs");
            // envc = 0
            payload.extend_from_slice(&0u32.to_le_bytes());
            // cwd = "/tmp"
            payload.extend_from_slice(&4u32.to_le_bytes());
            payload.extend_from_slice(b"/tmp");
            // stdout = "/tmp/o"
            payload.extend_from_slice(&6u32.to_le_bytes());
            payload.extend_from_slice(b"/tmp/o");
            // stderr = "/tmp/e"
            payload.extend_from_slice(&6u32.to_le_bytes());
            payload.extend_from_slice(b"/tmp/e");
            // umask + rlimit_as
            payload.extend_from_slice(&0o022u32.to_le_bytes());
            payload.extend_from_slice(&0u64.to_le_bytes());

            let mut frame = Vec::with_capacity(8 + payload.len());
            frame.extend_from_slice(WIRE_MAGIC);
            frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            frame.extend_from_slice(&payload);
            frame
        }

        #[test]
        fn parse_request_payload_rejects_oversized_counts() {
            // Claim argc = u32::MAX in a tiny 16-byte buffer.
            let mut bad_argc = Vec::new();
            bad_argc.extend_from_slice(&u32::MAX.to_le_bytes());
            bad_argc.extend_from_slice(&[0u8; 12]);
            assert!(parse_request_payload(&bad_argc).is_err());

            // Valid argc = 0, then oversized envc = u32::MAX.
            let mut bad_envc = Vec::new();
            bad_envc.extend_from_slice(&0u32.to_le_bytes());
            bad_envc.extend_from_slice(&u32::MAX.to_le_bytes());
            bad_envc.extend_from_slice(&[0u8; 16]);
            assert!(parse_request_payload(&bad_envc).is_err());
        }

        #[test]
        fn advance_pending_clients_preserves_revents_pairing_and_rejects_late_frames() {
            let now = Instant::now();
            let frame = encode_minimal_frame();

            // Client A: expired and not readable.
            let (server_a, mut peer_a) = UnixStream::pair().expect("pair a");
            server_a.set_nonblocking(true).unwrap();
            let mut client_a = PendingClient::new(server_a);
            client_a.deadline = now - Duration::from_secs(1);

            // Client B: valid deadline and readable with a complete frame.
            let (server_b, mut peer_b) = UnixStream::pair().expect("pair b");
            server_b.set_nonblocking(true).unwrap();
            peer_b.write_all(&frame).unwrap();
            let mut client_b = PendingClient::new(server_b);
            client_b.deadline = now + Duration::from_secs(5);

            // Client C: expired, yet also readable with a complete frame (late completion).
            let (server_c, mut peer_c) = UnixStream::pair().expect("pair c");
            server_c.set_nonblocking(true).unwrap();
            peer_c.write_all(&frame).unwrap();
            let mut client_c = PendingClient::new(server_c);
            client_c.deadline = now - Duration::from_millis(1);

            let mut pending = vec![client_a, client_b, client_c];
            let revents = [0, libc::POLLIN, libc::POLLIN];
            let mut ready = Vec::new();
            advance_pending_clients(&mut pending, &revents, now, &mut ready);

            assert!(pending.is_empty(), "all three clients should be resolved");
            assert_eq!(ready.len(), 1, "only on-time client B should be queued");
            assert_eq!(ready[0].1.argv, vec!["neomacs".to_string()]);

            // Both expired client A and late-completing client C must receive ETIMEDOUT.
            for (name, peer) in [("A", &mut peer_a), ("C", &mut peer_c)] {
                let mut resp = [0u8; 9];
                peer.read_exact(&mut resp)
                    .unwrap_or_else(|e| panic!("client {name} missing error response: {e}"));
                assert_eq!(&resp[0..4], WIRE_MAGIC);
                assert_eq!(resp[4], RESP_ERROR);
                assert_eq!(
                    i32::from_le_bytes(resp[5..9].try_into().unwrap()),
                    libc::ETIMEDOUT
                );
            }
        }

        #[test]
        fn idle_shutdown_drains_queued_connection_instead_of_exiting() {
            let dir = std::env::temp_dir().join(format!("nvz-idle-test-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            let sock = dir.join("z.sock");

            let listener = UnixListener::bind(&sock).expect("bind listener");
            listener.set_nonblocking(true).unwrap();

            // Queue a client in the kernel listen backlog without calling `poll`.
            let mut client = UnixStream::connect(&sock).expect("connect queued client");
            client.write_all(&encode_minimal_frame()).unwrap();

            let mut pending = Vec::new();
            let mut ready = Vec::new();
            let mut last_activity = Instant::now() - Duration::from_secs(30);
            let mut lifecycle = ServerLifecycle::Accepting {
                withdraw_failures: 0,
            };

            // Idle shutdown must accept the queued client and stay alive rather
            // than exiting and dropping the connection.
            let decision = try_withdraw_for_idle_shutdown(
                &sock,
                &listener,
                &mut pending,
                &mut ready,
                &mut last_activity,
                &mut lifecycle,
            );
            assert_eq!(decision, IdleShutdownDecision::Continue);
            assert_eq!(
                lifecycle,
                ServerLifecycle::Accepting {
                    withdraw_failures: 0
                }
            );
            assert_eq!(ready.len(), 1);
            assert!(sock.exists(), "socket should stay published while active");

            // Once all queued work is drained and no more clients are pending,
            // idle shutdown withdraws the socket and transitions to Draining.
            ready.clear();
            drop(client);
            let decision2 = try_withdraw_for_idle_shutdown(
                &sock,
                &listener,
                &mut pending,
                &mut ready,
                &mut last_activity,
                &mut lifecycle,
            );
            assert_eq!(decision2, IdleShutdownDecision::ExitNow);
            assert_eq!(lifecycle, ServerLifecycle::Draining);
            assert!(!sock.exists(), "socket should be unlinked on idle exit");

            let _ = std::fs::remove_dir_all(&dir);
        }

        #[test]
        fn idle_shutdown_keeps_accepting_when_unlink_fails_until_retry_limit() {
            let dir = std::env::temp_dir().join(format!("nvz-unlk-test-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            let sock = dir.join("z.sock");
            // Passing a directory path to `remove_file` reliably fails with EISDIR/EPERM.
            let unremovable = dir.join("occupied-dir");
            std::fs::create_dir_all(&unremovable).unwrap();

            let listener = UnixListener::bind(&sock).expect("bind listener");
            listener.set_nonblocking(true).unwrap();

            let mut pending = Vec::new();
            let mut ready = Vec::new();
            let mut last_activity = Instant::now() - Duration::from_secs(30);
            let mut lifecycle = ServerLifecycle::Accepting {
                withdraw_failures: 0,
            };

            for expected_failures in 1..MAX_SOCKET_WITHDRAW_RETRIES {
                let decision = try_withdraw_for_idle_shutdown(
                    &unremovable,
                    &listener,
                    &mut pending,
                    &mut ready,
                    &mut last_activity,
                    &mut lifecycle,
                );
                assert_eq!(decision, IdleShutdownDecision::Continue);
                assert_eq!(
                    lifecycle,
                    ServerLifecycle::Accepting {
                        withdraw_failures: expected_failures,
                    }
                );
            }

            let final_decision = try_withdraw_for_idle_shutdown(
                &unremovable,
                &listener,
                &mut pending,
                &mut ready,
                &mut last_activity,
                &mut lifecycle,
            );
            assert_eq!(final_decision, IdleShutdownDecision::ExitNow);
            assert_eq!(lifecycle, ServerLifecycle::ExitingWithoutUnlink);

            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}
