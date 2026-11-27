//! Client for the pre-booted Neomacs batch `fork()` zygote server.
//!
//! Routes eligible Neomacs oracle evaluations (default-environment batch runs
//! on Unix) through a shared per-run zygote daemon over a Unix domain socket so
//! the 21 MiB `neomacs.pdump` is loaded once per nextest run instead of once
//! per test case, while every test still executes inside its own OS-isolated
//! `fork()`ed child process.
//!
//! Works on both Linux and macOS (`#[cfg(unix)]`). Falls back transparently to
//! normal `Command::output()` when disabled (`NEOVM_ORACLE_ZYGOTE=0`), outside
//! Unix, or for tests with custom pre-boot environment overrides.

use std::process::Output;
use std::time::Duration;

use neomacs_test_env::PinnedCommand;

use crate::common::oracle_sandbox::OracleSandbox;

#[cfg(not(unix))]
pub(crate) fn try_run_neomacs_via_zygote(
    _cmd: &PinnedCommand,
    _sandbox: &OracleSandbox,
    _rlimit_as: Option<u64>,
    _timeout: Option<Duration>,
) -> Option<Result<Output, String>> {
    None
}

#[cfg(unix)]
pub(crate) fn try_run_neomacs_via_zygote(
    cmd: &PinnedCommand,
    sandbox: &OracleSandbox,
    rlimit_as: Option<u64>,
    timeout: Option<Duration>,
) -> Option<Result<Output, String>> {
    unix_impl::try_run_neomacs_via_zygote(cmd, sandbox, rlimit_as, timeout)
}

#[cfg(unix)]
mod unix_impl {
    use std::collections::BTreeMap;
    use std::collections::hash_map::DefaultHasher;
    use std::ffi::{OsStr, OsString};
    use std::fs::{self, OpenOptions};
    use std::hash::{Hash, Hasher};
    use std::io::{Read, Write};
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::net::UnixStream;
    use std::os::unix::process::ExitStatusExt;
    use std::path::{Path, PathBuf};
    use std::process::{ExitStatus, Output, Stdio};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, Instant};

    use neomacs_test_env::{CANONICAL_UMASK, PinnedCommand, bounded_temp_base};

    use crate::common::oracle_sandbox::OracleSandbox;

    const WIRE_MAGIC: &[u8; 4] = b"NVZ1";
    const RESP_EXITED: u8 = 0;
    const RESP_SIGNALED: u8 = 1;
    const STARTUP_WAIT_TIMEOUT: Duration = Duration::from_secs(10);

    static IO_SEQ: AtomicU64 = AtomicU64::new(0);

    fn zygote_enabled() -> bool {
        match std::env::var("NEOVM_ORACLE_ZYGOTE").ok().as_deref() {
            Some("0" | "off" | "false" | "no") => return false,
            Some("1" | "on" | "true" | "yes") => return true,
            _ => {}
        }
        // Enable automatically under `cargo nextest`, where thousands of test
        // processes share one `NEXTEST_RUN_ID`. Outside nextest (single ad-hoc
        // test invocation), opt in via `NEOVM_ORACLE_ZYGOTE=1`.
        std::env::var_os("NEXTEST_RUN_ID").is_some_and(|v| !v.is_empty())
    }

    fn socket_path_for(program: &OsStr) -> PathBuf {
        if let Some(explicit) = std::env::var_os("NEOVM_ORACLE_ZYGOTE_SOCK")
            && !explicit.is_empty()
        {
            return PathBuf::from(explicit);
        }
        let run_id = std::env::var("NEXTEST_RUN_ID").unwrap_or_else(|_| "manual".to_string());
        let mut hasher = DefaultHasher::new();
        program.as_bytes().hash(&mut hasher);
        run_id.hash(&mut hasher);
        // Include UID so multi-user /tmp directories never collide.
        // SAFETY: getuid() has no preconditions.
        let uid = unsafe { libc::getuid() };
        uid.hash(&mut hasher);
        let digest = hasher.finish();
        // Keep socket path short (< 45 bytes) so it fits well inside macOS's
        // 104-byte `sun_path` limit.
        bounded_temp_base().join(format!("nvz-{digest:016x}.sock"))
    }

    fn lock_path_for(sock_path: &Path) -> PathBuf {
        sock_path.with_extension("lock")
    }

    /// Identify the owning `cargo-nextest` PID when running under nextest so
    /// the zygote exits promptly (~250ms) once the test run finishes.
    fn detect_nextest_watch_pid() -> Option<libc::pid_t> {
        if std::env::var_os("NEXTEST_RUN_ID").is_none_or(|v| v.is_empty()) {
            return None;
        }
        // SAFETY: `getppid()` has no preconditions.
        let ppid = unsafe { libc::getppid() };
        if ppid <= 1 {
            return None;
        }
        #[cfg(target_os = "linux")]
        {
            let comm = fs::read_to_string(format!("/proc/{ppid}/comm")).ok()?;
            if !comm.trim().contains("nextest") {
                return None;
            }
        }
        Some(ppid)
    }

    fn acquire_flock(lock_path: &Path) -> Result<fs::File, String> {
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(lock_path)
            .map_err(|e| format!("failed to open zygote lock {}: {e}", lock_path.display()))?;
        loop {
            // SAFETY: `file` is a valid open file descriptor.
            let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
            if rc == 0 {
                return Ok(file);
            }
            let err = std::io::Error::last_os_error();
            if err.kind() != std::io::ErrorKind::Interrupted {
                return Err(format!(
                    "failed to flock zygote lock {}: {err}",
                    lock_path.display()
                ));
            }
        }
    }

    fn connect_or_spawn_zygote(
        program: &OsStr,
        sock_path: &Path,
        sandbox: &OracleSandbox,
    ) -> Result<UnixStream, String> {
        if let Ok(stream) = UnixStream::connect(sock_path) {
            return Ok(stream);
        }

        let lock_path = lock_path_for(sock_path);
        let _lock = acquire_flock(&lock_path)?;

        // Another parallel test process may have started the zygote while we
        // waited on the lock.
        if let Ok(stream) = UnixStream::connect(sock_path) {
            return Ok(stream);
        }

        // Remove any stale socket file left by an earlier crashed server.
        let _ = fs::remove_file(sock_path);

        let mut cmd = sandbox.pinned_command(program);
        cmd.arg("--batch-zygote-socket").arg(sock_path);
        // Never inherit test stdout/stderr pipes: nextest treats inherited open
        // stdio pipes as a leaked-handle error.
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // Detach into a fresh session/process group so nextest's per-test
        // process-group teardown does not kill the shared zygote.
        cmd.detach_session();
        // Clear any inherited `NEOMACS_LOG_FILE` so the zygote parent never
        // starts a background tracing file-appender thread before `fork()`.
        cmd.env_remove("NEOMACS_LOG_FILE");
        if std::env::var_os("NEOMACS_ZYGOTE_WATCH_PID").is_none()
            && let Some(watch_pid) = detect_nextest_watch_pid()
        {
            cmd.env("NEOMACS_ZYGOTE_WATCH_PID", watch_pid.to_string());
        }
        if std::env::var_os("NEOMACS_ZYGOTE_IDLE_SECS").is_none() {
            cmd.env("NEOMACS_ZYGOTE_IDLE_SECS", "15");
        }

        let mut child = cmd.spawn().map_err(|e| {
            format!(
                "failed to spawn neomacs zygote {}: {e}",
                Path::new(program).display()
            )
        })?;

        let deadline = Instant::now() + STARTUP_WAIT_TIMEOUT;
        while Instant::now() < deadline {
            if let Ok(stream) = UnixStream::connect(sock_path) {
                return Ok(stream);
            }
            if let Ok(Some(status)) = child.try_wait() {
                return Err(format!(
                    "neomacs zygote exited early with {status} before publishing {}",
                    sock_path.display()
                ));
            }
            std::thread::sleep(Duration::from_millis(5));
        }

        let _ = child.kill();
        let _ = child.wait();
        Err(format!(
            "timed out waiting for neomacs zygote socket {}",
            sock_path.display()
        ))
    }

    fn push_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
        out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(bytes);
    }

    fn encode_request(
        cmd: &PinnedCommand,
        cwd: &Path,
        stdout_path: &Path,
        stderr_path: &Path,
        rlimit_as: Option<u64>,
    ) -> Vec<u8> {
        let mut payload = Vec::with_capacity(4096);

        let mut argv: Vec<&OsStr> = Vec::new();
        argv.push(cmd.get_program());
        argv.extend(cmd.get_args());
        payload.extend_from_slice(&(argv.len() as u32).to_le_bytes());
        for arg in argv {
            push_bytes(&mut payload, arg.as_bytes());
        }

        // Build the exact effective environment `Command::spawn` would pass:
        // inherited process env overlaid with explicit `env()` / `env_remove()`.
        let mut env_map: BTreeMap<OsString, OsString> = std::env::vars_os().collect();
        for (key, value) in cmd.get_envs() {
            match value {
                Some(v) => {
                    env_map.insert(key.to_os_string(), v.to_os_string());
                }
                None => {
                    env_map.remove(key);
                }
            }
        }
        payload.extend_from_slice(&(env_map.len() as u32).to_le_bytes());
        for (key, value) in &env_map {
            push_bytes(&mut payload, key.as_bytes());
            push_bytes(&mut payload, value.as_bytes());
        }

        push_bytes(&mut payload, cwd.as_os_str().as_bytes());
        push_bytes(&mut payload, stdout_path.as_os_str().as_bytes());
        push_bytes(&mut payload, stderr_path.as_os_str().as_bytes());
        payload.extend_from_slice(&CANONICAL_UMASK.to_le_bytes());
        payload.extend_from_slice(&rlimit_as.unwrap_or(0).to_le_bytes());

        let mut frame = Vec::with_capacity(8 + payload.len());
        frame.extend_from_slice(WIRE_MAGIC);
        frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        frame.extend_from_slice(&payload);
        frame
    }

    fn decode_exit_status(resp: &[u8; 9]) -> Result<ExitStatus, String> {
        if &resp[0..4] != WIRE_MAGIC {
            return Err(format!("invalid zygote response magic: {:?}", &resp[0..4]));
        }
        let kind = resp[4];
        let code = i32::from_le_bytes(resp[5..9].try_into().unwrap());
        match kind {
            RESP_EXITED => Ok(ExitStatus::from_raw((code & 0xff) << 8)),
            RESP_SIGNALED => Ok(ExitStatus::from_raw(code & 0x7f)),
            other => Err(format!("zygote server error (kind={other}, code={code})")),
        }
    }

    fn round_trip(
        stream: &mut UnixStream,
        frame: &[u8],
        timeout: Option<Duration>,
    ) -> Result<ExitStatus, String> {
        stream
            .write_all(frame)
            .map_err(|e| format!("failed to send request to neomacs zygote: {e}"))?;
        let _ = stream.set_read_timeout(timeout);
        let mut resp = [0u8; 9];
        stream
            .read_exact(&mut resp)
            .map_err(|e| format!("failed to read response from neomacs zygote: {e}"))?;
        decode_exit_status(&resp)
    }

    pub(crate) fn try_run_neomacs_via_zygote(
        cmd: &PinnedCommand,
        sandbox: &OracleSandbox,
        rlimit_as: Option<u64>,
        timeout: Option<Duration>,
    ) -> Option<Result<Output, String>> {
        if !zygote_enabled() || !sandbox.extra_env_is_empty() {
            return None;
        }

        let program = cmd.get_program();
        let sock_path = socket_path_for(program);
        let cwd = match cmd.get_current_dir() {
            Some(dir) => dir.to_path_buf(),
            None => match std::env::current_dir() {
                Ok(dir) => dir,
                Err(e) => return Some(Err(format!("failed to determine cwd: {e}"))),
            },
        };

        let seq = IO_SEQ.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let stdout_path = sandbox
            .case_root_path()
            .join(format!("zygote-{pid}-{seq}.stdout"));
        let stderr_path = sandbox
            .case_root_path()
            .join(format!("zygote-{pid}-{seq}.stderr"));
        let frame = encode_request(cmd, &cwd, &stdout_path, &stderr_path, rlimit_as);

        // Retry once in case an idle zygote shut down right as we connected.
        let mut last_err = None;
        for attempt in 0..2 {
            let mut stream = match connect_or_spawn_zygote(program, &sock_path, sandbox) {
                Ok(s) => s,
                Err(e) => {
                    last_err = Some(e);
                    if attempt == 0 {
                        continue;
                    }
                    break;
                }
            };
            match round_trip(&mut stream, &frame, timeout) {
                Ok(status) => {
                    let stdout = fs::read(&stdout_path).unwrap_or_default();
                    let stderr = fs::read(&stderr_path).unwrap_or_default();
                    let _ = fs::remove_file(&stdout_path);
                    let _ = fs::remove_file(&stderr_path);
                    return Some(Ok(Output {
                        status,
                        stdout,
                        stderr,
                    }));
                }
                Err(e) => {
                    last_err = Some(e);
                    // Only retry if the server dropped the connection before
                    // replying (not on explicit caller timeout).
                    if attempt == 0 && timeout.is_none() {
                        continue;
                    }
                    break;
                }
            }
        }

        let _ = fs::remove_file(&stdout_path);
        let _ = fs::remove_file(&stderr_path);
        Some(Err(last_err.unwrap_or_else(|| {
            "unknown neomacs zygote transport failure".to_string()
        })))
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::thread;

        fn run_direct_on_socket(
            sock_path: &Path,
            program: &Path,
            cwd: &Path,
            io_dir: &Path,
            tag: &str,
            extra_env: &[(&str, &str)],
            eval_form: &str,
        ) -> Output {
            let mut cmd = neomacs_test_env::CanonicalEnvironment::new(
                neomacs_test_env::CanonicalProfile::Oracle,
                io_dir,
                io_dir,
                io_dir,
            )
            .expect("canonical env")
            .command(program);
            cmd.current_dir(cwd);
            for (k, v) in extra_env {
                cmd.env(k, v);
            }
            cmd.args(["--batch", "-Q", "--eval", eval_form]);

            let stdout_path = io_dir.join(format!("{tag}.stdout"));
            let stderr_path = io_dir.join(format!("{tag}.stderr"));
            let frame = encode_request(&cmd, cwd, &stdout_path, &stderr_path, None);
            let mut stream = UnixStream::connect(sock_path).expect("connect to private zygote");
            let status =
                round_trip(&mut stream, &frame, Some(Duration::from_secs(15))).expect("round trip");
            let stdout = fs::read(&stdout_path).unwrap_or_default();
            let stderr = fs::read(&stderr_path).unwrap_or_default();
            Output {
                status,
                stdout,
                stderr,
            }
        }

        fn spawn_private_zygote(
            program: &Path,
            sock_path: &Path,
            work_dir: &Path,
        ) -> std::process::Child {
            let environment = neomacs_test_env::CanonicalEnvironment::new(
                neomacs_test_env::CanonicalProfile::Oracle,
                work_dir,
                work_dir,
                work_dir,
            )
            .expect("canonical env");
            let mut cmd = environment.command(program);
            cmd.arg("--batch-zygote-socket").arg(sock_path);
            cmd.stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            cmd.env_remove("NEOMACS_LOG_FILE");
            cmd.env("NEOMACS_ZYGOTE_IDLE_SECS", "10");
            let child = cmd.spawn().expect("spawn private zygote");
            let deadline = Instant::now() + STARTUP_WAIT_TIMEOUT;
            while Instant::now() < deadline {
                if sock_path.exists() {
                    return child;
                }
                thread::sleep(Duration::from_millis(5));
            }
            panic!("private zygote did not publish {}", sock_path.display());
        }

        #[test]
        fn decode_exit_status_handles_exit_signal_and_errors() {
            let mut exited = [0u8; 9];
            exited[0..4].copy_from_slice(WIRE_MAGIC);
            exited[4] = RESP_EXITED;
            exited[5..9].copy_from_slice(&42i32.to_le_bytes());
            assert_eq!(decode_exit_status(&exited).unwrap().code(), Some(42));

            let mut signaled = [0u8; 9];
            signaled[0..4].copy_from_slice(WIRE_MAGIC);
            signaled[4] = RESP_SIGNALED;
            signaled[5..9].copy_from_slice(&libc::SIGKILL.to_le_bytes());
            assert_eq!(
                std::os::unix::process::ExitStatusExt::signal(
                    &decode_exit_status(&signaled).unwrap()
                ),
                Some(libc::SIGKILL)
            );

            let mut bad_magic = [0u8; 9];
            bad_magic[0..4].copy_from_slice(b"XXXX");
            assert!(decode_exit_status(&bad_magic).is_err());
        }

        #[test]
        fn zygote_isolates_lisp_state_env_cwd_stdio_and_exit_code() {
            let program = PathBuf::from(crate::common::neomacs_binary_path());
            let dir = neomacs_test_env::bounded_temp_dir("nvz-iso-").expect("tempdir");
            let sock = dir.path().join("z.sock");
            let cwd_a = dir.path().join("cwd-a");
            let cwd_b = dir.path().join("cwd-b");
            fs::create_dir_all(&cwd_a).unwrap();
            fs::create_dir_all(&cwd_b).unwrap();

            let mut server = spawn_private_zygote(&program, &sock, dir.path());

            // Request 1 mutates obarray, creates a buffer, writes stdout+stderr.
            let out1 = run_direct_on_socket(
                &sock,
                &program,
                &cwd_a,
                dir.path(),
                "req1",
                &[("NEOVM_ZYGOTE_TEST_TAG", "alpha")],
                r#"(progn
                     (defvar neovm--zygote-leak 'dirty)
                     (fset 'neovm--zygote-fn (lambda () 'dirty))
                     (get-buffer-create "*zygote-leak*")
                     (princ (format "%d:%s" (emacs-pid) (getenv "NEOVM_ZYGOTE_TEST_TAG")))
                     (princ "STDERR-ALPHA" 'external-debugging-output))"#,
            );
            assert!(out1.status.success(), "req1 failed: {out1:?}");
            let s1 = String::from_utf8_lossy(&out1.stdout);
            assert!(s1.ends_with(":alpha"), "unexpected stdout1: {s1:?}");
            assert!(
                String::from_utf8_lossy(&out1.stderr).contains("STDERR-ALPHA"),
                "missing stderr1: {:?}",
                String::from_utf8_lossy(&out1.stderr)
            );
            let pid1: i64 = s1.split(':').next().unwrap().parse().unwrap();

            // Request 2 on the SAME zygote must see zero leaked state and its own env/cwd/pid.
            let out2 = run_direct_on_socket(
                &sock,
                &program,
                &cwd_b,
                dir.path(),
                "req2",
                &[("NEOVM_ZYGOTE_TEST_TAG", "beta")],
                r#"(princ (format "%S"
                     (list (boundp 'neovm--zygote-leak)
                           (fboundp 'neovm--zygote-fn)
                           (not (null (get-buffer "*zygote-leak*")))
                           (emacs-pid)
                           (getenv "NEOVM_ZYGOTE_TEST_TAG")
                           (file-name-nondirectory (directory-file-name default-directory)))))"#,
            );
            assert!(out2.status.success(), "req2 failed: {out2:?}");
            let s2 = String::from_utf8_lossy(&out2.stdout);
            assert!(
                s2.starts_with("(nil nil nil ") && s2.ends_with(" \"beta\" \"cwd-b\")"),
                "state leaked or env/cwd mismatch: {s2:?}"
            );
            assert!(
                !String::from_utf8_lossy(&out2.stderr).contains("STDERR-ALPHA"),
                "stderr leaked into req2"
            );
            let pid2: i64 = s2.split_whitespace().nth(3).unwrap().parse().unwrap();
            assert_ne!(pid1, pid2, "forked children must have distinct PIDs");

            // Non-zero exit code via `kill-emacs` must propagate verbatim.
            let out3 = run_direct_on_socket(
                &sock,
                &program,
                &cwd_a,
                dir.path(),
                "req3",
                &[],
                "(kill-emacs 42)",
            );
            assert_eq!(out3.status.code(), Some(42));

            let _ = server.kill();
            let _ = server.wait();
        }

        #[test]
        fn zygote_concurrent_sleep_for_children_do_not_share_poller() {
            // Regression test: before deferring `ProcessManager::new()` until
            // post-image activation, forked children inherited one shared
            // `epoll`/`timerfd`/`kqueue` and deadlocked when sleeping together.
            let program = PathBuf::from(crate::common::neomacs_binary_path());
            let dir = neomacs_test_env::bounded_temp_dir("nvz-slp-").expect("tempdir");
            let sock = dir.path().join("z.sock");

            let mut server = spawn_private_zygote(&program, &sock, dir.path());

            let t0 = Instant::now();
            let handles: Vec<_> = (0..4)
                .map(|i| {
                    let sock = sock.clone();
                    let program = program.clone();
                    let work = dir.path().to_path_buf();
                    thread::spawn(move || {
                        run_direct_on_socket(
                            &sock,
                            &program,
                            &work,
                            &work,
                            &format!("sleep-{i}"),
                            &[],
                            "(progn (sleep-for 0.15) (princ \"DONE\"))",
                        )
                    })
                })
                .collect();

            for (i, handle) in handles.into_iter().enumerate() {
                let out = handle.join().expect("worker thread panicked");
                assert!(out.status.success(), "sleep worker {i} failed: {out:?}");
                assert_eq!(String::from_utf8_lossy(&out.stdout), "DONE");
            }
            assert!(
                t0.elapsed() < Duration::from_secs(5),
                "concurrent sleep-for children took too long: {:?}",
                t0.elapsed()
            );

            let _ = server.kill();
            let _ = server.wait();
        }
    }
}
