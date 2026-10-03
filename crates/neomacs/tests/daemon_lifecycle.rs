//! Real display-free editor/client lifecycle regressions. No mock server and
//! no installed runtime: Cargo's matching binaries run on disposable HOME/XDG
//! data, and the bootstrap runtime image they boot from is provisioned beside
//! the editor binary on first use (see [`ensure_bootstrap_runtime_image`]).
#![cfg(target_os = "linux")]

use std::fs;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

/// Provision the bootstrap runtime image the daemon boots from, once per
/// test process.
///
/// The editor loads `RuntimeImageRole::Bootstrap` from beside its own binary
/// and never builds that image itself.  Without it the daemon bootstraps from
/// Lisp sources instead — minutes of loading, then a hard failure — which the
/// fixture's readiness deadline reports as the unhelpful "daemon never became
/// ready".  The image must land where the loader searches, so every name here
/// comes from the editor's own loader API rather than a literal that could
/// drift from the layout it looks in.
fn ensure_bootstrap_runtime_image() {
    use neovm_core::emacs_core::load::{
        RuntimeImageRole, TEMACS_ROLE_BINARY_NAME, fingerprinted_runtime_image_path_for_executable,
        runtime_image_path_for_executable,
    };

    static PROVISIONED: OnceLock<Result<(), String>> = OnceLock::new();
    let provisioned = PROVISIONED.get_or_init(|| {
        let editor = PathBuf::from(env!("CARGO_BIN_EXE_neomacs"));
        // A final image beside the binaries wins over the bootstrap image at
        // load time (the daemon prefers it), so a stale one would run old
        // code against these tests.  Repairing it is a full build, not
        // something a test may do -- but it must not be silent.
        for stale_candidate in [
            runtime_image_path_for_executable(&editor, RuntimeImageRole::Final),
            fingerprinted_runtime_image_path_for_executable(&editor, RuntimeImageRole::Final),
        ] {
            if stale_candidate.is_file()
                && neomacs_infra::runtime_image::image_is_older_than(&stale_candidate, &editor)
            {
                return Err(format!(
                    "{} is older than {}; the daemon would load that final image in \
                     preference to the bootstrap image and run stale code. Remove it or \
                     point CARGO_TARGET_DIR at a clean directory",
                    stale_candidate.display(),
                    editor.display(),
                ));
            }
        }
        let plan = neomacs_infra::runtime_image::BootstrapImagePlan {
            loader_image: fingerprinted_runtime_image_path_for_executable(
                &editor,
                RuntimeImageRole::Bootstrap,
            ),
            canonical_image_name: RuntimeImageRole::Bootstrap.image_file_name().to_string(),
            editor,
            runtime_root: neomacs_infra::workspace_root(),
            role_binary_name: TEMACS_ROLE_BINARY_NAME.to_string(),
        };
        neomacs_infra::runtime_image::provision_bootstrap_image(&plan).map(|_| ())
    });
    if let Err(error) = provisioned {
        panic!("preparing the daemon test runtime image failed: {error}");
    }
}

// Background daemons are not children of this runner. Capture kernel pidfds
// while their isolated HOME is still verifiable; never signal a PID from a
// stale marker, and retain every candidate rather than just the last writer.
struct BackgroundOwner {
    stop: Arc<AtomicBool>,
    watcher: Option<std::thread::JoinHandle<Vec<OwnedFd>>>,
}

impl BackgroundOwner {
    fn new(root: PathBuf) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = Arc::clone(&stop);
        let watcher = std::thread::spawn(move || {
            let expected = format!("HOME={}", root.join("home").display()).into_bytes();
            let mut seen = std::collections::HashSet::new();
            let mut owned = Vec::new();
            while !stopping.load(Ordering::SeqCst) {
                if let Ok(pid) = fs::read_to_string(root.join("owned-pid"))
                    .unwrap_or_default()
                    .trim()
                    .parse::<i32>()
                    && pid > 0
                    && !seen.contains(&pid)
                {
                    // SAFETY: pidfd_open has no pointer arguments. Its
                    // returned descriptor pins one process across PID reuse.
                    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) };
                    if fd >= 0 {
                        let fd = unsafe { OwnedFd::from_raw_fd(fd as i32) };
                        let environment =
                            fs::read(format!("/proc/{pid}/environ")).unwrap_or_default();
                        if environment
                            .split(|byte| *byte == 0)
                            .any(|entry| entry == expected)
                        {
                            seen.insert(pid);
                            owned.push(fd);
                        }
                    }
                }
                std::thread::sleep(Duration::from_millis(2));
            }
            owned
        });
        Self {
            stop,
            watcher: Some(watcher),
        }
    }
}

impl Drop for BackgroundOwner {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let owned = self.watcher.take().unwrap().join().unwrap();
        for fd in owned {
            // SAFETY: the owned pidfd pins the fixture process, never a reused
            // numeric PID. A null siginfo asks the kernel for standard SIGKILL.
            unsafe {
                libc::syscall(
                    libc::SYS_pidfd_send_signal,
                    fd.as_raw_fd(),
                    libc::SIGKILL,
                    std::ptr::null::<libc::siginfo_t>(),
                    0,
                );
            }
            let mut poll = libc::pollfd {
                fd: fd.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            // A readable pidfd proves process termination, not socket removal.
            assert_eq!(
                unsafe { libc::poll(&mut poll, 1, 5000) },
                1,
                "owned daemon did not exit"
            );
        }
    }
}

struct Fixture {
    // Drop the process owners before deleting their isolated files.
    _background: BackgroundOwner,
    root: tempfile::TempDir,
    daemon: Option<Child>,
}

impl Fixture {
    fn new() -> Self {
        ensure_bootstrap_runtime_image();
        let root = tempfile::Builder::new()
            .prefix("neomacs-daemon-")
            .tempdir()
            .unwrap();
        for dir in ["home", "runtime", "config", "data", "cache", "tmp"] {
            fs::create_dir(root.path().join(dir)).unwrap();
            fs::set_permissions(root.path().join(dir), fs::Permissions::from_mode(0o700)).unwrap();
        }
        let background = BackgroundOwner::new(root.path().to_owned());
        Self {
            _background: background,
            root,
            daemon: None,
        }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.root.path().join(name)
    }
    fn socket(&self, name: &str) -> PathBuf {
        self.path("runtime/emacs").join(name)
    }

    fn command(&self, binary: &str) -> Command {
        let mut command = Command::new(binary);
        command
            .env("HOME", self.path("home"))
            .env("XDG_RUNTIME_DIR", self.path("runtime"))
            .env("XDG_CONFIG_HOME", self.path("config"))
            .env("XDG_DATA_HOME", self.path("data"))
            .env("XDG_CACHE_HOME", self.path("cache"))
            .env("TMPDIR", self.path("tmp"))
            .env("NEOMACS_LOG_FILE", self.path("runtime.log"))
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("EMACS_SERVER_FILE")
            .env_remove("EMACS_SOCKET_NAME")
            .env_remove("ALTERNATE_EDITOR")
            .stdin(Stdio::null());
        command
    }

    fn editor(&self) -> Command {
        self.command(env!("CARGO_BIN_EXE_neomacs"))
    }
    fn client(&self, name: &str, expr: &str) -> Command {
        let mut command = self.command(env!("CARGO_BIN_EXE_neomacsclient"));
        command.args(["-s", name, "-w", "5", "-e", expr]);
        command
    }

    fn eval(&self, name: &str, expr: &str) -> String {
        let output = bounded(self.client(name, expr), Duration::from_secs(10));
        assert!(output.status.success(), "{output:?}");
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    fn foreground(&mut self, name: &str, extra: &[&str]) {
        let mut command = self.editor();
        command
            .arg(format!("--fg-daemon={name}"))
            .args(extra)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        self.daemon = Some(command.spawn().unwrap());
        let deadline = Instant::now() + Duration::from_secs(60);
        while !self.socket(name).exists() {
            if let Some(status) = self.daemon.as_mut().unwrap().try_wait().unwrap() {
                panic!(
                    "daemon exited before readiness: {status}; {}",
                    fs::read_to_string(self.path("runtime.log")).unwrap_or_default()
                );
            }
            assert!(Instant::now() < deadline, "daemon never became ready");
            std::thread::sleep(Duration::from_millis(25));
        }
        // An endpoint alone is not readiness: prove an ordinary eval roundtrip.
        assert_eq!(self.eval(name, "(+ 20 22)"), "42");
    }

    fn write_init(&self, body: &str) {
        let marker = self.path("owned-pid");
        fs::write(
            self.path("home/.emacs"),
            format!(
                "(with-temp-file {:?} (insert (number-to-string (emacs-pid))))\n{body}\n",
                marker.to_str().unwrap()
            ),
        )
        .unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(mut child) = self.daemon.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn bounded(mut command: Command, timeout: Duration) -> Output {
    let stdout = tempfile::NamedTempFile::new().unwrap();
    let stderr = tempfile::NamedTempFile::new().unwrap();
    command
        .stdout(stdout.reopen().unwrap())
        .stderr(stderr.reopen().unwrap());
    let mut child = command.spawn().unwrap();
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "command timed out: {command:?}; stderr={}",
                fs::read_to_string(stderr.path()).unwrap_or_default()
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    Output {
        status,
        stdout: fs::read(stdout.path()).unwrap(),
        stderr: fs::read(stderr.path()).unwrap(),
    }
}

fn wait_for_path(path: &std::path::Path, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while !path.exists() {
        assert!(
            Instant::now() < deadline,
            "missing marker: {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

// Pin identity and settle owned processes even if a later assertion fails.
struct PinnedProcess(OwnedFd);
impl PinnedProcess {
    fn new(pid: i32) -> Self {
        assert!(pid > 0);
        // SAFETY: pidfd_open returns a fresh descriptor pinning one process.
        let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) };
        assert!(fd >= 0, "pidfd_open({pid}) failed");
        Self(unsafe { OwnedFd::from_raw_fd(fd as i32) })
    }
    fn signal(&self, sig: i32) {
        // SAFETY: the pidfd pins this owned process, never a recycled PID.
        assert_eq!(
            unsafe {
                libc::syscall(
                    libc::SYS_pidfd_send_signal,
                    self.0.as_raw_fd(),
                    sig,
                    std::ptr::null::<libc::siginfo_t>(),
                    0,
                )
            },
            0
        );
    }
    fn assert_exited(&self) {
        let mut event = libc::pollfd {
            fd: self.0.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: one initialized pollfd for a live owned descriptor.
        assert_eq!(
            unsafe { libc::poll(&mut event, 1, 5000) },
            1,
            "owned process did not terminate"
        );
    }
}
impl Drop for PinnedProcess {
    fn drop(&mut self) {
        // SAFETY: pins the task-owned process; ESRCH is normal after exit.
        unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                self.0.as_raw_fd(),
                libc::SIGKILL,
                std::ptr::null::<libc::siginfo_t>(),
                0,
            );
        }
        self.assert_exited();
    }
}

struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn wait_child(child: &mut Child, timeout: Duration) -> std::process::ExitStatus {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        assert!(Instant::now() < deadline, "owned child did not exit");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn prepare_signal_exit(fixture: &Fixture, name: &str) -> PinnedProcess {
    fixture.eval(name, &format!(
        "(progn (setq daemon-signal-hook-count 0) \
         (add-hook 'kill-emacs-hook (lambda () \
         (with-temp-file {:?} (insert (number-to-string (setq daemon-signal-hook-count (1+ daemon-signal-hook-count))))) \
         (signal-process (emacs-pid) 'SIGTERM) (sleep-for 0.05))) \
         (setq daemon-signal-process (start-process \"signal-child\" nil \"sleep\" \"60\")) \
         (set-process-query-on-exit-flag daemon-signal-process nil))",
        fixture.path("shutdown-hook"),
    ));
    let pid = fixture
        .eval(name, "(process-id daemon-signal-process)")
        .parse()
        .unwrap();
    PinnedProcess::new(pid)
}

#[test]
fn termination_signals_run_exit_hooks_and_cleanup_idle_and_busy_foreground_daemons() {
    for sig in [libc::SIGTERM, libc::SIGHUP] {
        for busy in [false, true] {
            let mut fixture = Fixture::new();
            fixture.foreground("signal", &["-Q"]);
            let subprocess = prepare_signal_exit(&fixture, "signal");
            let mut busy_client = if busy {
                let expr = format!(
                    "(progn (with-temp-file {:?} (insert \"busy\")) (let ((inhibit-quit t)) (while t)))",
                    fixture.path("busy")
                );
                Some(OwnedChild(
                    fixture
                        .client("signal", &expr)
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .spawn()
                        .unwrap(),
                ))
            } else {
                None
            };
            if busy {
                wait_for_path(&fixture.path("busy"), Duration::from_secs(5));
            }
            // Retained foreground Child owns the PID until wait/reap.
            assert_eq!(
                unsafe { libc::kill(fixture.daemon.as_ref().unwrap().id() as i32, sig) },
                0
            );
            assert_eq!(
                wait_child(fixture.daemon.as_mut().unwrap(), Duration::from_secs(5)).code(),
                Some(sig)
            );
            assert_eq!(
                fs::read_to_string(fixture.path("shutdown-hook")).unwrap(),
                "1"
            );
            assert!(!fixture.socket("signal").exists());
            subprocess.assert_exited();
            if let Some(client) = busy_client.as_mut() {
                wait_child(&mut client.0, Duration::from_secs(5));
            }
        }
    }
}

#[test]
fn termination_signals_interrupt_synchronous_call_process_and_reap_its_child() {
    for sig in [libc::SIGTERM, libc::SIGHUP] {
        let mut fixture = Fixture::new();
        fixture.foreground("sync-signal", &["-Q"]);
        fixture.eval(
            "sync-signal",
            &format!(
                "(add-hook 'kill-emacs-hook (lambda () (with-temp-file {:?} (insert \"hook\"))))",
                fixture.path("shutdown-hook")
            ),
        );
        // The shell writes its identity, then execs the sleeper: one exact
        // synchronous child, not an unowned shell descendant.
        let script = format!(
            "echo $$ > {}; exec sleep 3600",
            fixture.path("sync-pid").display()
        );
        let expr = format!("(call-process \"sh\" nil nil nil \"-c\" {:?})", script);
        let mut client = OwnedChild(
            fixture
                .client("sync-signal", &expr)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        wait_for_path(&fixture.path("sync-pid"), Duration::from_secs(5));
        let pid = fs::read_to_string(fixture.path("sync-pid"))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let subprocess = PinnedProcess::new(pid);
        assert_eq!(
            unsafe { libc::kill(fixture.daemon.as_ref().unwrap().id() as i32, sig) },
            0
        );
        assert_eq!(
            wait_child(fixture.daemon.as_mut().unwrap(), Duration::from_secs(5)).code(),
            Some(sig)
        );
        subprocess.assert_exited();
        assert_eq!(
            fs::read_to_string(fixture.path("shutdown-hook")).unwrap(),
            "hook"
        );
        assert!(!fixture.socket("sync-signal").exists());
        wait_child(&mut client.0, Duration::from_secs(5));
    }
}

#[test]
fn termination_signals_interrupt_call_process_region_and_reap_its_child() {
    for sig in [libc::SIGTERM, libc::SIGHUP] {
        let mut fixture = Fixture::new();
        fixture.foreground("region-signal", &["-Q"]);
        fixture.eval(
            "region-signal",
            &format!(
                "(add-hook 'kill-emacs-hook (lambda () (with-temp-file {:?} (insert \"hook\"))))",
                fixture.path("shutdown-hook")
            ),
        );
        let script = format!(
            "echo $$ > {}; exec sleep 3600",
            fixture.path("region-pid").display()
        );
        let expr = format!(
            "(call-process-region \"input\" nil \"sh\" nil nil nil \"-c\" {:?})",
            script
        );
        let mut client = OwnedChild(
            fixture
                .client("region-signal", &expr)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        wait_for_path(&fixture.path("region-pid"), Duration::from_secs(5));
        let pid = fs::read_to_string(fixture.path("region-pid"))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let subprocess = PinnedProcess::new(pid);
        assert_eq!(
            unsafe { libc::kill(fixture.daemon.as_ref().unwrap().id() as i32, sig) },
            0
        );
        assert_eq!(
            wait_child(fixture.daemon.as_mut().unwrap(), Duration::from_secs(5)).code(),
            Some(sig)
        );
        subprocess.assert_exited();
        assert!(
            !std::path::Path::new(&format!("/proc/{pid}")).exists(),
            "region child was not reaped"
        );
        assert_eq!(
            fs::read_to_string(fixture.path("shutdown-hook")).unwrap(),
            "hook"
        );
        assert!(!fixture.socket("region-signal").exists());
        wait_child(&mut client.0, Duration::from_secs(5));
    }
}

#[test]
fn no_wait_region_does_not_block_on_a_large_input_to_a_nonreading_child() {
    for sig in [libc::SIGTERM, libc::SIGHUP] {
        let mut fixture = Fixture::new();
        fixture.foreground("region-nowait", &["-Q"]);
        fixture.eval(
            "region-nowait",
            &format!(
                "(add-hook 'kill-emacs-hook (lambda () (with-temp-file {:?} (insert \"hook\"))))",
                fixture.path("shutdown-hook")
            ),
        );
        let script = format!(
            "echo $$ > {}; exec sleep 3600",
            fixture.path("region-pid").display()
        );
        let expr = format!(
            "(call-process-region (make-string 1048576 ?x) nil \"sh\" nil 0 nil \"-c\" {:?})",
            script
        );
        let stdout = tempfile::NamedTempFile::new().unwrap();
        let mut client = OwnedChild(
            fixture
                .client("region-nowait", &expr)
                .stdout(stdout.reopen().unwrap())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        wait_for_path(&fixture.path("region-pid"), Duration::from_secs(5));
        let pid = fs::read_to_string(fixture.path("region-pid"))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let _detached_child = PinnedProcess::new(pid);
        assert!(wait_child(&mut client.0, Duration::from_secs(5)).success());
        assert_eq!(fs::read_to_string(stdout.path()).unwrap().trim(), "nil");
        assert_eq!(fixture.eval("region-nowait", "(+ 20 22)"), "42");
        assert_eq!(
            unsafe { libc::kill(fixture.daemon.as_ref().unwrap().id() as i32, sig) },
            0
        );
        assert_eq!(
            wait_child(fixture.daemon.as_mut().unwrap(), Duration::from_secs(5)).code(),
            Some(sig)
        );
        assert_eq!(
            fs::read_to_string(fixture.path("shutdown-hook")).unwrap(),
            "hook"
        );
        assert!(!fixture.socket("region-nowait").exists());
        // Integer destination deliberately transfers the child to a detached
        // waiter. The fixture's pinned owner cleans it only AFTER these checks;
        // releasing it cannot be what made the evaluator return or exit.
    }
}

#[test]
fn batch_error_shutdown_hooks_finish_once_and_preserve_fatal_status() {
    for sig in [libc::SIGTERM, libc::SIGHUP] {
        let fixture = Fixture::new();
        let expr = format!(
            "(progn (setq batch-exit-count 0 debug-on-error nil kill-emacs-hook \
             (list (lambda () (setq batch-exit-count (1+ batch-exit-count)) \
             (with-temp-file {:?} (insert (number-to-string batch-exit-count))) \
             (signal-process (emacs-pid) {}) (sleep-for 0.05) \
             (with-temp-file {:?} (insert (number-to-string batch-exit-count)))) \
             (lambda () (with-temp-file {:?} (insert (number-to-string batch-exit-count)))))) \
             (error \"batch-fatal-probe\"))",
            fixture.path("entered"),
            sig,
            fixture.path("continued"),
            fixture.path("later")
        );
        let mut command = fixture.editor();
        command.args(["-Q", "--batch", "--eval", &expr]);
        let output = bounded(command, Duration::from_secs(60));
        assert_eq!(output.status.code(), Some(255), "{output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("batch-fatal-probe"),
            "{output:?}"
        );
        for marker in ["entered", "continued", "later"] {
            assert_eq!(
                fs::read_to_string(fixture.path(marker)).unwrap(),
                "1",
                "{marker}, signal {sig}"
            );
        }
    }
}

#[test]
fn signals_inside_explicit_exit_hooks_finish_once_and_preserve_original_request() {
    for sig in [libc::SIGTERM, libc::SIGHUP] {
        for restart in [false, true] {
            let mut fixture = Fixture::new();
            fixture.foreground("hook-signal", &["-Q"]);
            let subprocess = PinnedProcess::new(fixture.eval("hook-signal",
                "(progn (setq daemon-signal-hook-count 0 daemon-hook-child (start-process \"hook-child\" nil \"sleep\" \"60\")) (set-process-query-on-exit-flag daemon-hook-child nil) (process-id daemon-hook-child))").parse().unwrap());
            fixture.eval("hook-signal", &format!(
                "(setq kill-emacs-hook (append (list (lambda () \
                 (with-temp-file {:?} (insert (number-to-string (setq daemon-signal-hook-count (1+ daemon-signal-hook-count))))) \
                 (signal-process (emacs-pid) {}) (sleep-for 0.05)) \
                 (lambda () (with-temp-file {:?} (insert \"later\")))) kill-emacs-hook)) \
                 (setq daemon-old-state 'must-disappear)",
                fixture.path("shutdown-hook"), sig, fixture.path("later-hook")
            ));
            let pid = fixture.daemon.as_ref().unwrap().id();
            fixture.eval(
                "hook-signal",
                if restart {
                    "(kill-emacs nil t)"
                } else {
                    "(kill-emacs)"
                },
            );
            subprocess.assert_exited();
            assert_eq!(
                fs::read_to_string(fixture.path("shutdown-hook")).unwrap(),
                "1"
            );
            assert_eq!(
                fs::read_to_string(fixture.path("later-hook")).unwrap(),
                "later"
            );
            if restart {
                let deadline = Instant::now() + Duration::from_secs(60);
                loop {
                    let output = bounded(
                        fixture.client(
                            "hook-signal",
                            "(list (emacs-pid) (boundp 'daemon-old-state))",
                        ),
                        Duration::from_secs(10),
                    );
                    if output.status.success() {
                        assert_eq!(
                            String::from_utf8_lossy(&output.stdout).trim(),
                            format!("({pid} nil)")
                        );
                        break;
                    }
                    assert!(Instant::now() < deadline, "restart did not become ready");
                }
                fixture.eval("hook-signal", "(kill-emacs 8)");
            }
            assert_eq!(
                wait_child(fixture.daemon.as_mut().unwrap(), Duration::from_secs(5)).code(),
                Some(if restart { 8 } else { 0 })
            );
            assert!(!fixture.socket("hook-signal").exists());
        }
    }
}

#[test]
fn batch_preserves_inherited_ignored_hup_and_completes_its_action() {
    use std::os::unix::process::CommandExt;
    let fixture = Fixture::new();
    let mut command = fixture.editor();
    let expr = format!(
        "(progn (with-temp-file {:?} (insert \"ready\")) (while (not (file-exists-p {:?})) (sleep-for 0.01)) (with-temp-file {:?} (insert \"completed\")))",
        fixture.path("batch-ready"),
        fixture.path("release-batch"),
        fixture.path("batch-completed")
    );
    command
        .args(["-Q", "--batch", "--eval", &expr])
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // SAFETY: sigaction is async-signal-safe; this closure only installs an
    // inherited disposition, before exec in our owned single child.
    unsafe {
        command.pre_exec(|| {
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = libc::SIG_IGN;
            libc::sigemptyset(&mut action.sa_mask);
            if libc::sigaction(libc::SIGHUP, &action, std::ptr::null_mut()) == 0 {
                Ok(())
            } else {
                Err(std::io::Error::last_os_error())
            }
        });
    }
    let mut child = OwnedChild(command.spawn().unwrap());
    wait_for_path(&fixture.path("batch-ready"), Duration::from_secs(60));
    assert_eq!(unsafe { libc::kill(child.0.id() as i32, libc::SIGHUP) }, 0);
    fs::write(fixture.path("release-batch"), "release").unwrap();
    assert!(wait_child(&mut child.0, Duration::from_secs(5)).success());
    assert_eq!(
        fs::read_to_string(fixture.path("batch-completed")).unwrap(),
        "completed"
    );
}

#[test]
fn termination_signals_cleanup_background_daemons_through_exact_pidfd_owner() {
    for sig in [libc::SIGTERM, libc::SIGHUP] {
        let fixture = Fixture::new();
        fixture.write_init("");
        let mut command = fixture.editor();
        command.arg("--bg-daemon=signal");
        assert!(bounded(command, Duration::from_secs(70)).status.success());
        let pid = fixture.eval("signal", "(emacs-pid)").parse().unwrap();
        let daemon = PinnedProcess::new(pid);
        let subprocess = prepare_signal_exit(&fixture, "signal");
        daemon.signal(sig);
        daemon.assert_exited();
        subprocess.assert_exited();
        assert_eq!(
            fs::read_to_string(fixture.path("shutdown-hook")).unwrap(),
            "1"
        );
        assert!(!fixture.socket("signal").exists());
    }
}

#[test]
fn termination_signals_restore_attached_client_terminal_modes() {
    signal_terminal_cleanup(false);
}

#[test]
fn sigchld_retention_and_lost_authority_never_signal_an_auto_reaped_child() {
    // SIGCHLD/subreaper policy is process-wide: never mutate the parallel
    // Cargo harness. The child runs only this test, including on plain cargo test.
    if std::env::var_os("NEOMACS_SIGCHLD_PROBE_CHILD").is_none() {
        let mut probe = Command::new(std::env::current_exe().unwrap());
        probe
            .args([
                "--exact",
                "sigchld_retention_and_lost_authority_never_signal_an_auto_reaped_child",
                "--test-threads=1",
            ])
            .env("NEOMACS_SIGCHLD_PROBE_CHILD", "1");
        let output = bounded(probe, Duration::from_secs(180));
        assert!(output.status.success(), "{output:?}");
        return;
    }
    use std::os::unix::process::CommandExt;
    // This process-isolated target runs serially. Adopt only the probe's
    // orphaned output holders so their exits are reaped, not left with PID 1.
    struct Adoption(i32);
    impl Drop for Adoption {
        fn drop(&mut self) {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                let mut status = 0;
                let pid = unsafe { libc::waitpid(-1, &mut status, libc::WNOHANG) };
                if pid < 0 {
                    break;
                }
                if pid == 0 {
                    assert!(Instant::now() < deadline, "unsettled probe descendant");
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
            unsafe {
                libc::prctl(libc::PR_SET_CHILD_SUBREAPER, self.0);
            }
        }
    }
    let mut previous = 0;
    assert_eq!(
        unsafe { libc::prctl(libc::PR_GET_CHILD_SUBREAPER, &mut previous) },
        0
    );
    assert_eq!(unsafe { libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1) }, 0);
    let _adoption = Adoption(previous);
    let shim_dir = tempfile::tempdir().unwrap();
    let source = shim_dir.path().join("sigchld.c");
    let shim = shim_dir.path().join("sigchld.so");
    fs::write(&source, include_str!("fixtures/daemon_sigchld.c")).unwrap();
    let mut cc = Command::new("cc");
    cc.args(["-Wall", "-Wextra", "-Werror", "-shared", "-fPIC"])
        .arg(&source)
        .arg("-o")
        .arg(&shim)
        .arg("-ldl");
    assert!(bounded(cc, Duration::from_secs(30)).status.success());
    for policy in ["ignored", "no-cldwait", "lost"] {
        for region in [false, true] {
            let mut fixture = Fixture::new();
            let configure = |command: &mut Command| {
                command
                    .env("LD_PRELOAD", &shim)
                    .env("DAEMON_CHILD_TRACE", fixture.path("trace"));
                if policy == "ignored" {
                    // SAFETY: async-signal-safe, only in this disposable exec child.
                    unsafe {
                        command.pre_exec(|| {
                            libc::signal(libc::SIGCHLD, libc::SIG_IGN);
                            Ok(())
                        });
                    }
                } else {
                    command.env(
                        if policy == "lost" {
                            "DAEMON_LOSE_CHILD_RETENTION"
                        } else {
                            "DAEMON_INITIAL_NOCLDWAIT"
                        },
                        "1",
                    );
                }
            };
            let call = if region {
                "(with-temp-buffer (call-process-region (point-min) (point-max) \"sh\" nil nil nil \"-c\" \"exit 37\"))"
            } else {
                "(call-process \"sh\" nil nil nil \"-c\" \"exit 37\")"
            };
            let mut natural = fixture.editor();
            configure(&mut natural);
            natural.args([
                "-Q",
                "--batch",
                "--eval",
                &format!("(prin1 (condition-case nil {call} (file-error 'lost)))"),
            ]);
            let output = bounded(natural, Duration::from_secs(30));
            assert!(output.status.success(), "{output:?}");
            let trace = fs::read_to_string(fixture.path("trace")).unwrap_or_default();
            if policy == "lost" {
                assert!(
                    trace.contains("LOSS") && trace.contains("ECHILD"),
                    "{trace}"
                );
            }
            assert_eq!(
                String::from_utf8_lossy(&output.stdout),
                if policy == "lost" { "lost" } else { "37" },
                "policy={policy}, region={region}, trace={trace}"
            );
            assert!(!trace.contains("SUPPRESSED_KILL"));

            let mut daemon = fixture.editor();
            configure(&mut daemon);
            daemon
                .args(["--fg-daemon=retention", "-Q"])
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            fixture.daemon = Some(daemon.spawn().unwrap());
            wait_for_path(&fixture.socket("retention"), Duration::from_secs(60));
            assert_eq!(fixture.eval("retention", "(+ 20 22)"), "42");
            fixture.eval("retention", &format!("(setq kill-emacs-hook (list (lambda () (with-temp-file {:?} (insert \"1\")))))", fixture.path("hook")));
            // A descendant holds output open after the immediate leader exits.
            // Pin that owned holder while live, never an auto-reaped leader.
            let script = format!(
                "echo $$ > {}; sh -c 'echo $$ > {}; exec sleep 3600' & exit 0",
                fixture.path("leader").display(),
                fixture.path("holder").display()
            );
            let expr = if region {
                format!(
                    "(with-temp-buffer (call-process-region (point-min) (point-max) \"sh\" nil t nil \"-c\" {:?}))",
                    script
                )
            } else {
                format!("(call-process \"sh\" nil t nil \"-c\" {:?})", script)
            };
            let mut client = OwnedChild(
                fixture
                    .client("retention", &expr)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .unwrap(),
            );
            wait_for_path(&fixture.path("holder"), Duration::from_secs(5));
            let holder = PinnedProcess::new(
                fs::read_to_string(fixture.path("holder"))
                    .unwrap()
                    .trim()
                    .parse()
                    .unwrap(),
            );
            let leader: i32 = fs::read_to_string(fixture.path("leader"))
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                let stat = fs::read_to_string(format!("/proc/{leader}/stat"));
                let ready = if policy == "lost" {
                    stat.is_err()
                } else {
                    stat.is_ok_and(|s| s.split(") ").nth(1).is_some_and(|s| s.starts_with('Z')))
                };
                if ready {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "leader neither retained nor auto-reaped: {policy}"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            // Real TERM before any status probe; no external holder release.
            assert_eq!(
                unsafe { libc::kill(fixture.daemon.as_ref().unwrap().id() as i32, libc::SIGTERM) },
                0
            );
            assert_eq!(
                wait_child(fixture.daemon.as_mut().unwrap(), Duration::from_secs(5)).code(),
                Some(libc::SIGTERM)
            );
            wait_child(&mut client.0, Duration::from_secs(5));
            assert_eq!(fs::read_to_string(fixture.path("hook")).unwrap(), "1");
            assert!(!fixture.socket("retention").exists());
            assert!(
                !fs::read_to_string(fixture.path("trace"))
                    .unwrap_or_default()
                    .contains("SUPPRESSED_KILL"),
                "requested numeric SIGKILL after leader exit: {policy}, region={region}"
            );
            drop(holder);
        }
    }
}

#[test]
fn daemon_shutdown_preserves_a_replacement_socket_path() {
    use std::os::unix::net::UnixListener;
    let mut fixture = Fixture::new();
    fixture.foreground("replacement", &["-Q"]);
    fixture.eval("replacement", "(setq kill-emacs-hook nil)");
    let path = fixture.socket("replacement");
    let retired = fixture.path("retired-socket");
    fs::rename(&path, &retired).unwrap();
    let _replacement = UnixListener::bind(&path).unwrap();
    assert_eq!(
        unsafe { libc::kill(fixture.daemon.as_ref().unwrap().id() as i32, libc::SIGTERM) },
        0
    );
    assert_eq!(
        wait_child(fixture.daemon.as_mut().unwrap(), Duration::from_secs(5)).code(),
        Some(libc::SIGTERM)
    );
    assert!(
        path.exists(),
        "shutdown removed a different listener's node"
    );
    assert!(retired.exists(), "shutdown guessed a renamed socket path");
}

#[test]
fn alternate_editor_runs_before_tty_validation_or_reading_stdin() {
    // The public native gate needs only the matching product binaries. An
    // explicitly selected GNU oracle exercises the same strict assertions.
    let mut binaries = vec![env!("CARGO_BIN_EXE_neomacsclient").to_owned()];
    if let Some(gnu) = std::env::var_os("NEOMACS_GNU_EMACSCLIENT") {
        binaries.push(gnu.into_string().expect("GNU client path must be UTF-8"));
    }
    for binary in binaries {
        for tty in [true, false] {
            for explicit in [true, false] {
                let fixture = Fixture::new();
                let alternate = fixture.path("alternate");
                fs::write(
                    &alternate,
                    format!(
                        "#!/bin/sh\nprintf 'called' > {:?}\nexit 0\n",
                        fixture.path("alternate-called")
                    ),
                )
                .unwrap();
                fs::set_permissions(&alternate, fs::Permissions::from_mode(0o700)).unwrap();
                let mut command = fixture.command(&binary);
                command.args([
                    "-s",
                    fixture.socket("missing").to_str().unwrap(),
                    "-w",
                    "1",
                    if tty { "-t" } else { "-e" },
                ]);
                if explicit {
                    // CLI must override the inherited fallback, not combine it.
                    command
                        .env("ALTERNATE_EDITOR", "/does/not/exist")
                        .arg("-a")
                        .arg(&alternate);
                } else {
                    command.env("ALTERNATE_EDITOR", &alternate);
                }
                command
                    .stdin(Stdio::piped())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                let mut child = OwnedChild(command.spawn().unwrap());
                // Retain an open, empty stdin. EOF would hide the ordering bug.
                let _stdin = child.0.stdin.take().unwrap();
                assert!(
                    wait_child(&mut child.0, Duration::from_secs(3)).success(),
                    "fallback failed: binary={binary}, tty={tty}, explicit={explicit}"
                );
                assert_eq!(
                    fs::read_to_string(fixture.path("alternate-called")).unwrap(),
                    "called"
                );
            }
        }
    }
}

#[test]
fn alternate_editor_preserves_filename_argv_and_literal_command_tokens() {
    let mut binaries = vec![env!("CARGO_BIN_EXE_neomacsclient").to_owned()];
    if let Some(gnu) = std::env::var_os("NEOMACS_GNU_EMACSCLIENT") {
        binaries.push(gnu.into_string().expect("GNU client path must be UTF-8"));
    }
    for binary in binaries {
        for explicit in [true, false] {
            for tokens in [false, true] {
                let fixture = Fixture::new();
                let alternate = fixture.path("alternate with space");
                fs::write(&alternate, format!(
                    "#!/bin/sh\nprintf 'called\\n' >> {:?}\nprintf 'argc=%s\\n' \"$#\"\nfor arg do printf 'arg=<%s>\\n' \"$arg\"; done\n",
                    fixture.path("alternate-called"),
                )).unwrap();
                fs::set_permissions(&alternate, fs::Permissions::from_mode(0o700)).unwrap();
                let alternate = if tokens {
                    format!(
                        "\"{}\" \"fixed with space\" 'literal' $HOME ; two&x",
                        alternate.display()
                    )
                } else {
                    format!("\"{}\"", alternate.display())
                };
                let mut command = fixture.command(&binary);
                command.args(["-s", fixture.socket("missing").to_str().unwrap(), "-w", "1"]);
                if explicit {
                    command
                        .env("ALTERNATE_EDITOR", "/does/not/exist")
                        .arg("-a")
                        .arg(&alternate);
                } else {
                    command.env("ALTERNATE_EDITOR", &alternate);
                }
                command.args(["--", "file with space", "--daemon=literal", "two&x"]);
                let output = bounded(command, Duration::from_secs(3));
                assert!(
                    output.status.success(),
                    "binary={binary}, explicit={explicit}: {output:?}"
                );
                let expected = if tokens {
                    "argc=8\narg=<fixed with space>\narg=<'literal'>\narg=<$HOME>\narg=<;>\narg=<two&x>\n"
                } else {
                    "argc=3\n"
                };
                assert_eq!(
                    String::from_utf8(output.stdout).unwrap(),
                    format!(
                        "{expected}arg=<file with space>\narg=<--daemon=literal>\narg=<two&x>\n"
                    )
                );
                assert_eq!(
                    fs::read_to_string(fixture.path("alternate-called")).unwrap(),
                    "called\n"
                );
            }
        }
    }
}

#[test]
fn selected_gnu_client_services_state_errors_restart_and_shutdown() {
    // Unselected means native-only, not an implicit dependency on PATH.
    let Some(gnu) = std::env::var_os("NEOMACS_GNU_EMACSCLIENT") else {
        return;
    };
    let gnu = gnu.into_string().expect("GNU client path must be UTF-8");
    let mut fixture = Fixture::new();
    fixture.foreground("gnu-interop", &["-Q"]);
    let evaluate = |expr: &str| {
        let mut command = fixture.command(&gnu);
        command.args([
            "-s",
            fixture.socket("gnu-interop").to_str().unwrap(),
            "-w",
            "5",
            "-e",
            expr,
        ]);
        bounded(command, Duration::from_secs(10))
    };
    let value = |expr: &str| {
        let output = evaluate(expr);
        assert!(output.status.success(), "{output:?}");
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    };
    assert_eq!(
        value("(list (daemonp) noninteractive (not (null after-init-time)))"),
        "(\"gnu-interop\" nil t)"
    );
    assert_eq!(
        fixture.eval(
            "gnu-interop",
            "(progn (setq gnu-interop-value 99) gnu-interop-value)"
        ),
        "99"
    );
    assert_eq!(value("gnu-interop-value"), "99");
    let error = evaluate("(error \"gnu-interop-expected-error\")");
    assert!(!error.status.success());
    assert!(
        String::from_utf8(error.stderr)
            .unwrap()
            .contains("gnu-interop-expected-error")
    );
    assert_eq!(value("gnu-interop-value"), "99");
    let pid = fixture.daemon.as_ref().unwrap().id();
    assert_eq!(value("(emacs-pid)"), pid.to_string());
    value("(kill-emacs nil t)");
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let output = evaluate("(list (emacs-pid) (daemonp) (boundp 'gnu-interop-value))");
        if output.status.success()
            && String::from_utf8_lossy(&output.stdout).trim()
                == format!("({pid} \"gnu-interop\" nil)")
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "GNU restart never ready: {output:?}"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    value("(kill-emacs 13)");
    assert_eq!(
        wait_child(fixture.daemon.as_mut().unwrap(), Duration::from_secs(10)).code(),
        Some(13)
    );
    assert!(!fixture.socket("gnu-interop").exists());
}

#[cfg(feature = "jit")]
#[test]
fn optional_aot_persistence_does_not_block_daemon_terminal_and_process_cleanup() {
    // Prove the same runtime and warm bytecode really activate persistence
    // outside daemon mode; an inert PGO setup must not make this test green.
    let fixture = Fixture::new();
    let mut command = fixture.editor();
    configure_blocking_aot(&fixture, &mut command);
    command.args(["-Q", "--batch", "--eval", HOT_AOT_LEAF]);
    command.stdout(Stdio::null()).stderr(Stdio::null());
    let mut editor = OwnedChild(command.spawn().unwrap());
    wait_for_path(&fixture.path("compiler-pid"), Duration::from_secs(30));
    let compiler = PinnedProcess::new(
        fs::read_to_string(fixture.path("compiler-pid"))
            .unwrap()
            .trim()
            .parse()
            .unwrap(),
    );
    assert!(editor.0.try_wait().unwrap().is_none());
    compiler.signal(libc::SIGKILL);
    assert!(wait_child(&mut editor.0, Duration::from_secs(10)).success());
    compiler.assert_exited();
    signal_terminal_cleanup(true);
}

const HOT_AOT_LEAF: &str = "(progn (defalias 'daemon-aot-leaf (byte-compile (lambda (x) (+ x 1)))) (unless (byte-code-function-p (symbol-function 'daemon-aot-leaf)) (error \"not bytecode\")) (dotimes (i 3000) (daemon-aot-leaf i)) (unless (= (daemon-aot-leaf 41) 42) (error \"wrong result\")))";

fn configure_blocking_aot(fixture: &Fixture, command: &mut Command) {
    fs::create_dir(fixture.path("bin")).unwrap();
    fs::create_dir(fixture.path("aot")).unwrap();
    let compiler = fixture.path("bin/cc");
    // Only the first compiler blocks. Its PID is retained before signalling;
    // later invocations fail immediately, so settling the control cannot hang.
    fs::write(&compiler, format!(
        "#!/bin/sh\nif test -e {:?}; then exit 1; fi\nprintf '%s' $$ > {:?}\nexec /usr/bin/sleep 3600\n",
        fixture.path("compiler-pid"), fixture.path("compiler-pid"),
    )).unwrap();
    fs::set_permissions(compiler, fs::Permissions::from_mode(0o700)).unwrap();
    command
        .env("NEOVM_AOT_PGO", "1")
        .env("NEOVM_AOT_DIR", fixture.path("aot"))
        .env(
            "PATH",
            format!(
                "{}:{}",
                fixture.path("bin").display(),
                std::env::var("PATH").unwrap()
            ),
        );
}

fn signal_terminal_cleanup(pgo: bool) {
    use std::io::Read;
    for sig in [libc::SIGTERM, libc::SIGHUP] {
        let mut fixture = Fixture::new();
        if pgo {
            let mut command = fixture.editor();
            configure_blocking_aot(&fixture, &mut command);
            command
                .args(["--fg-daemon=signal-tty", "-Q"])
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            fixture.daemon = Some(command.spawn().unwrap());
            wait_for_path(&fixture.socket("signal-tty"), Duration::from_secs(60));
            assert_eq!(fixture.eval("signal-tty", "(+ 20 22)"), "42");
        } else {
            fixture.foreground("signal-tty", &["-Q"]);
        }
        let subprocess = prepare_signal_exit(&fixture, "signal-tty");
        if pgo {
            // Do not allow Lisp's standard hooks to mask missing host cleanup.
            fixture.eval("signal-tty", &format!("(setq kill-emacs-hook (list (lambda () (with-temp-file {:?} (insert \"1\")))))", fixture.path("shutdown-hook")));
            let mut heat = fixture.client("signal-tty", HOT_AOT_LEAF);
            heat.args(["-w", "30"]);
            let output = bounded(heat, Duration::from_secs(40));
            assert!(output.status.success(), "{output:?}");
        }
        let mut master = fs::File::from(
            rustix::pty::openpt(rustix::pty::OpenptFlags::RDWR | rustix::pty::OpenptFlags::NOCTTY)
                .unwrap(),
        );
        rustix::pty::grantpt(&master).unwrap();
        rustix::pty::unlockpt(&master).unwrap();
        let slave = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(
                rustix::pty::ptsname(&master, Vec::new())
                    .unwrap()
                    .to_string_lossy()
                    .as_ref(),
            )
            .unwrap();
        let original = format!("{:?}", rustix::termios::tcgetattr(&slave).unwrap());
        assert_ne!(
            unsafe { libc::fcntl(master.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK) },
            -1
        );
        let expr = format!(
            "(with-temp-file {:?} (insert \"attached\"))",
            fixture.path("attached")
        );
        let mut command = fixture.client("signal-tty", &expr);
        command
            .args(["-t", "-w", "30"])
            .env("TERM", "xterm-256color")
            .stdin(Stdio::from(slave.try_clone().unwrap()))
            .stdout(Stdio::from(slave.try_clone().unwrap()))
            .stderr(Stdio::null());
        let mut client = OwnedChild(command.spawn().unwrap());
        wait_for_path(&fixture.path("attached"), Duration::from_secs(40));
        assert_ne!(
            format!("{:?}", rustix::termios::tcgetattr(&slave).unwrap()),
            original,
            "attachment did not enter raw mode"
        );
        assert!(
            client.0.try_wait().unwrap().is_none(),
            "client detached before the signal"
        );
        assert_eq!(
            unsafe { libc::kill(fixture.daemon.as_ref().unwrap().id() as i32, sig) },
            0
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        // On RED, pin the unexpected blocking compiler before unwinding;
        // never leave a probe descendant behind or release it to pass cleanup.
        let mut compiler_cleanup = None;
        loop {
            if compiler_cleanup.is_none() && fixture.path("compiler-pid").exists() {
                compiler_cleanup = Some(PinnedProcess::new(
                    fs::read_to_string(fixture.path("compiler-pid"))
                        .unwrap()
                        .trim()
                        .parse()
                        .unwrap(),
                ));
            }
            let mut bytes = [0; 4096];
            while let Ok(n) = master.read(&mut bytes) {
                if n == 0 {
                    break;
                }
            }
            if client.0.try_wait().unwrap().is_some() {
                break;
            }
            assert!(Instant::now() < deadline, "attached client did not settle");
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            wait_child(fixture.daemon.as_mut().unwrap(), Duration::from_secs(5)).code(),
            Some(sig)
        );
        assert_eq!(
            format!("{:?}", rustix::termios::tcgetattr(&slave).unwrap()),
            original
        );
        assert_eq!(
            fs::read_to_string(fixture.path("shutdown-hook")).unwrap(),
            "1"
        );
        assert!(!fixture.socket("signal-tty").exists());
        subprocess.assert_exited();
        assert!(
            !fixture.path("compiler-pid").exists(),
            "daemon ran optional compiler during shutdown"
        );
    }
}

#[test]
fn foreground_daemon_initializes_headlessly_and_services_eval_timers_and_processes() {
    let mut fixture = Fixture::new();
    fixture.write_init("(setq daemon-probe-init (list (daemonp) noninteractive (frame-visible-p (selected-frame)) (frame-initial-p)))");
    fixture.foreground("development", &["--eval", "(setq daemon-probe-action 17)"]);
    assert_eq!(
        fixture.eval("development", "daemon-probe-init"),
        "(\"development\" nil t t)"
    );
    assert_eq!(fixture.eval("development", "(list daemon-probe-action (not (null after-init-time)) (not (null (process-live-p server-process))))"), "(17 t t)");
    assert_eq!(
        fixture.eval(
            "development",
            "(condition-case err (daemon-initialized) (error (error-message-string err)))"
        ),
        "\"The daemon has already been initialized\""
    );
    assert_eq!(fixture.eval("development", "(progn (setq daemon-probe-timer nil) (run-at-time 0.01 nil (lambda () (setq daemon-probe-timer t))) (get-buffer-create \"daemon-output\") (set-process-sentinel (start-process \"daemon-process\" \"daemon-output\" \"sh\" \"-c\" \"printf daemon-output\") #'ignore) 'scheduled)"), "scheduled");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let result = fixture.eval(
            "development",
            "(list daemon-probe-timer (with-current-buffer \"daemon-output\" (buffer-string)))",
        );
        if result == "(t \"daemon-output\")" {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "timer/process made no progress: {result}"
        );
    }
    assert_eq!(fixture.eval("development", "(progn (setq daemon-probe-counter 0) (setq daemon-probe-counter (1+ daemon-probe-counter)))"), "1");
    assert_eq!(
        fixture.eval(
            "development",
            "(setq daemon-probe-counter (1+ daemon-probe-counter))"
        ),
        "2"
    );
    let failed = bounded(
        fixture.client("development", "(error \"expected-client-error\")"),
        Duration::from_secs(10),
    );
    assert!(!failed.status.success());
    assert!(String::from_utf8_lossy(&failed.stderr).contains("expected-client-error"));
    assert_eq!(fixture.eval("development", "(+ 1 2)"), "3");
    let pid = fixture.eval("development", "(emacs-pid)");
    let mut existing = fixture.client("development", "(emacs-pid)");
    existing.args(["-a", ""]);
    let output = bounded(existing, Duration::from_secs(10));
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), pid);
    let mut duplicate = fixture.editor();
    duplicate.args(["-Q", "--daemon=development"]);
    assert!(!bounded(duplicate, Duration::from_secs(70)).status.success());
    assert_eq!(fixture.eval("development", "(emacs-pid)"), pid);
    let owned_process_pid: i32 = fixture.eval("development", "(progn (setq daemon-probe-long-process (start-process \"daemon-long-process\" nil \"sleep\" \"60\")) (set-process-query-on-exit-flag daemon-probe-long-process nil) (process-id daemon-probe-long-process))").parse().unwrap();
    let owned_process = PinnedProcess::new(owned_process_pid);
    fixture.eval("development", "(kill-emacs 7)");
    let child = fixture.daemon.as_mut().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < deadline, "kill-emacs failed to exit");
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(status.code(), Some(7));
    owned_process.assert_exited();
    assert!(
        !fixture.socket("development").exists(),
        "server socket not cleaned"
    );
}

#[test]
fn background_daemon_reports_ready_then_cleans_up_and_allows_same_name_restart() {
    let fixture = Fixture::new();
    fixture.write_init("(setq daemon-probe-ready 19) (add-hook 'emacs-startup-hook (lambda () (setq daemon-probe-startup-hook t)))");
    for flag in ["--daemon=background", "--bg-daemon=background"] {
        let mut editor = fixture.editor();
        editor.arg(flag);
        let output = bounded(editor, Duration::from_secs(70));
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            fixture.eval(
                "background",
                "(list (daemonp) daemon-probe-ready noninteractive daemon-probe-startup-hook)"
            ),
            "(\"background\" 19 nil t)"
        );
        fixture.eval("background", "(kill-emacs)");
        assert!(!fixture.socket("background").exists());
    }
}

#[test]
fn automatic_startup_default_named_environment_and_concurrent_clients() {
    let fixture = Fixture::new();
    fixture.write_init("(setq daemon-probe-ready 23)");
    let output = {
        // No explicit selection: the daemon's identity must be t, not an
        // invented absolute socket name or the literal default name.
        let mut command = fixture.command(env!("CARGO_BIN_EXE_neomacsclient"));
        command.args([
            "-a",
            "",
            "-w",
            "30",
            "-e",
            "(list (daemonp) daemon-probe-ready)",
        ]);
        bounded(command, Duration::from_secs(40))
    };
    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "(t 23)");
    fixture.eval("server", "(kill-emacs)");

    let mut handles = Vec::new();
    for _ in 0..4 {
        let mut command = fixture.command(env!("CARGO_BIN_EXE_neomacsclient"));
        command
            .env("EMACS_SOCKET_NAME", "concurrent")
            .env("ALTERNATE_EDITOR", "")
            .args([
                "-w",
                "30",
                "-e",
                "(list (emacs-pid) (daemonp) daemon-probe-ready)",
            ]);
        handles.push(std::thread::spawn(move || {
            bounded(command, Duration::from_secs(40))
        }));
    }
    let outputs: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect();
    for output in &outputs {
        assert!(output.status.success(), "{output:?}");
    }
    for output in &outputs[1..] {
        assert_eq!(output.stdout, outputs[0].stdout);
    }
    assert!(String::from_utf8_lossy(&outputs[0].stdout).contains("\"concurrent\" 23)"));
    fixture.eval("concurrent", "(kill-emacs)");
    assert!(!fixture.socket("concurrent").exists());
}

#[test]
fn automatic_startup_recovers_a_stale_socket_and_preserves_absolute_selection() {
    let fixture = Fixture::new();
    fixture.write_init("");
    fs::create_dir(fixture.path("runtime/emacs")).unwrap();
    fs::set_permissions(
        fixture.path("runtime/emacs"),
        fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    let socket = fixture.socket("absolute");
    drop(std::os::unix::net::UnixListener::bind(&socket).unwrap());
    let mut command = fixture.client(socket.to_str().unwrap(), "(daemonp)");
    command.args(["-a", "", "-w", "30"]);
    let output = bounded(command, Duration::from_secs(40));
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        format!("{:?}", socket.to_str().unwrap())
    );
    fixture.eval(socket.to_str().unwrap(), "(kill-emacs)");
    assert!(!socket.exists());
}

#[test]
fn startup_failure_returns_nonzero_without_publishing_readiness() {
    let fixture = Fixture::new();
    let mut command = fixture.editor();
    command.args([
        "-Q",
        "--daemon=failed",
        "--eval",
        "(error \"expected-startup-failure\")",
    ]);
    let output = bounded(command, Duration::from_secs(70));
    assert!(!output.status.success(), "{output:?}");
    assert!(!fixture.socket("failed").exists());
    assert!(String::from_utf8_lossy(&output.stderr).contains("daemon"));
}

#[test]
fn restart_reexecs_foreground_daemon_and_preserves_pid_and_name() {
    let mut fixture = Fixture::new();
    fixture.foreground("restart", &["-Q"]);
    let pid = fixture.eval(
        "restart",
        "(progn (setq daemon-pre-restart-state 19) (emacs-pid))",
    );
    fixture.eval("restart", "(kill-emacs nil t)");
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let output = bounded(
            fixture.client(
                "restart",
                "(list (emacs-pid) (daemonp) (boundp 'daemon-pre-restart-state))",
            ),
            Duration::from_secs(10),
        );
        if output.status.success() {
            assert_eq!(
                String::from_utf8_lossy(&output.stdout).trim(),
                format!("({pid} \"restart\" nil)")
            );
            break;
        }
        assert!(Instant::now() < deadline, "restart failed: {output:?}");
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[test]
fn inherited_readiness_descriptor_errors_identify_the_operation() {
    use std::os::unix::process::CommandExt;
    let fixture = Fixture::new();
    let mut closed = fixture.editor();
    closed
        .args(["-Q", "--fg-daemon=invalid-fd"])
        .env("NEOMACS_DAEMON_NOTIFY_FD", i32::MAX.to_string());
    let output = bounded(closed, Duration::from_secs(5));
    assert!(!output.status.success(), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("cannot duplicate daemon readiness descriptor 2147483647"),
        "{output:?}"
    );
    let file = tempfile::NamedTempFile::new().unwrap();
    let fd = file.as_file().as_raw_fd();
    let mut nonsocket = fixture.editor();
    nonsocket
        .args(["-Q", "--fg-daemon=invalid-socket"])
        .env("NEOMACS_DAEMON_NOTIFY_FD", "198");
    // SAFETY: file remains open through spawn; dup2 is async-signal-safe and
    // makes descriptor 198 an inherited regular file, not a readiness socket.
    unsafe {
        nonsocket.pre_exec(move || {
            if libc::dup2(fd, 198) < 0 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
    }
    let output = bounded(nonsocket, Duration::from_secs(5));
    assert!(!output.status.success(), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("invalid daemon readiness socket descriptor 198"),
        "{output:?}"
    );
    assert!(!fixture.socket("invalid-fd").exists());
    assert!(!fixture.socket("invalid-socket").exists());
}

#[test]
fn empty_alternates_connect_to_live_servers_without_creation_permissions() {
    let mut fixture = Fixture::new();
    fixture.foreground("live", &["-Q"]);
    let pid = fixture.eval("live", "(emacs-pid)");
    let directory = fixture.path("runtime/emacs");
    let lock = fixture.path("runtime/emacs/live.startup-lock");
    assert!(!lock.exists());
    // A reachable endpoint needs neither a private creation directory nor
    // write access. Also cover a persistent lock which is only readable.
    for (mode, existing_lock) in [(0o755, false), (0o500, false), (0o500, true)] {
        if existing_lock {
            fs::write(&lock, "").unwrap();
            fs::set_permissions(&lock, fs::Permissions::from_mode(0o400)).unwrap();
        }
        fs::set_permissions(&directory, fs::Permissions::from_mode(mode)).unwrap();
        let mut outputs = Vec::new();
        for environment in [false, true] {
            let mut command = fixture.client("live", "(list (emacs-pid) (+ 1 2))");
            if environment {
                command.env("ALTERNATE_EDITOR", "");
            } else {
                command.args(["-a", ""]);
            }
            outputs.push(bounded(command, Duration::from_secs(10)));
        }
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        for output in outputs {
            assert!(output.status.success(), "mode={mode:o}: {output:?}");
            assert_eq!(
                String::from_utf8_lossy(&output.stdout).trim(),
                format!("({pid} 3)")
            );
        }
        assert_eq!(
            lock.exists(),
            existing_lock,
            "live connection created a lock"
        );
    }
}

#[test]
fn client_startup_rejects_unsafe_directories_and_symlink_locks() {
    let fixture = Fixture::new();
    fs::create_dir(fixture.path("unsafe")).unwrap();
    fs::set_permissions(fixture.path("unsafe"), fs::Permissions::from_mode(0o777)).unwrap();
    let mut command = fixture.client(fixture.path("unsafe/server").to_str().unwrap(), "t");
    command.args(["-a", ""]);
    let output = bounded(command, Duration::from_secs(5));
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unsafe server directory"));
    fs::create_dir(fixture.path("runtime/emacs")).unwrap();
    fs::set_permissions(
        fixture.path("runtime/emacs"),
        fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    let target = fixture.path("untouched");
    fs::write(&target, "preserve").unwrap();
    std::os::unix::fs::symlink(&target, fixture.path("runtime/emacs/locked.startup-lock")).unwrap();
    let mut command = fixture.client("locked", "t");
    command.args(["-a", ""]);
    assert!(!bounded(command, Duration::from_secs(5)).status.success());
    assert_eq!(fs::read_to_string(target).unwrap(), "preserve");
}

#[test]
fn startup_deadline_includes_lock_wait_and_does_not_start_a_second_daemon() {
    use std::os::fd::AsRawFd;
    let fixture = Fixture::new();
    fs::create_dir(fixture.path("runtime/emacs")).unwrap();
    fs::set_permissions(
        fixture.path("runtime/emacs"),
        fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    let lock = fs::OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(fixture.path("runtime/emacs/busy.startup-lock"))
        .unwrap();
    fs::set_permissions(
        fixture.path("runtime/emacs/busy.startup-lock"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    // SAFETY: lock's file descriptor remains live until the assertion finishes.
    assert_eq!(unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) }, 0);
    let mut command = fixture.client("busy", "t");
    command.args(["-a", "", "--startup-timeout", "1"]);
    let output = bounded(command, Duration::from_secs(5));
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("timed out"));
    assert!(!fixture.socket("busy").exists());
}

#[test]
fn daemon_client_tty_attaches_evaluates_detaches_and_leaves_daemon_usable() {
    use std::io::Read;
    use std::os::fd::AsRawFd;
    let mut fixture = Fixture::new();
    fixture.foreground("tty", &["-Q"]);
    let mut master = fs::File::from(
        rustix::pty::openpt(rustix::pty::OpenptFlags::RDWR | rustix::pty::OpenptFlags::NOCTTY)
            .unwrap(),
    );
    rustix::pty::grantpt(&master).unwrap();
    rustix::pty::unlockpt(&master).unwrap();
    let slave_name = rustix::pty::ptsname(&master, Vec::new())
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let slave = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(slave_name)
        .unwrap();
    // Configure this client's terminal with ERASE = ^H before it attaches.
    // GNU reads each terminal's ERASE byte in `init_sys_modes`
    // (src/sysdep.c:1130) and `normal-erase-is-backspace-setup-frame` acts on
    // it, so the daemon must report the *attaching* terminal's byte rather
    // than the one it read from its own (absent) stdin.
    {
        let mut modes = rustix::termios::tcgetattr(&slave).unwrap();
        modes.special_codes[rustix::termios::SpecialCodeIndex::VERASE] = 8;
        rustix::termios::tcsetattr(&slave, rustix::termios::OptionalActions::Now, &modes).unwrap();
    }
    let original_modes = format!("{:?}", rustix::termios::tcgetattr(&slave).unwrap());
    // SAFETY: master is an owned, live descriptor. Nonblocking drain prevents
    // a failed client assertion from hanging the integration test runner.
    assert_ne!(
        unsafe { libc::fcntl(master.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK) },
        -1
    );
    let errors = tempfile::NamedTempFile::new().unwrap();
    let mut command = fixture.client("tty", "(progn (setq daemon-probe-initial-delete (condition-case err (delete-frame terminal-frame) (error (error-message-string err)))) (run-at-time 0.1 nil #'delete-frame (selected-frame)) (list 'daemon-tty (frame-initial-p) noninteractive))");
    command
        // Source-only bootstrap images load the terminal Lisp on first
        // attachment. This response timeout is separate from daemon startup.
        .args(["-t", "-w", "30"])
        .env("TERM", "xterm-256color")
        .stdout(Stdio::from(slave.try_clone().unwrap()))
        .stdin(Stdio::from(slave.try_clone().unwrap()))
        .stderr(errors.reopen().unwrap());
    let mut client = command.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(45);
    let mut output = Vec::new();
    let status = loop {
        let mut bytes = [0; 4096];
        while let Ok(count) = master.read(&mut bytes) {
            if count == 0 {
                break;
            }
            output.extend_from_slice(&bytes[..count]);
        }
        if let Some(status) = client.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = client.kill();
            let _ = client.wait();
            panic!("TTY client timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(
        status.success(),
        "{}; daemon log={}",
        fs::read_to_string(errors.path()).unwrap(),
        fs::read_to_string(fixture.path("runtime.log")).unwrap_or_default()
    );
    assert!(
        String::from_utf8_lossy(&output).contains("(daemon-tty nil nil)"),
        "{:?}",
        String::from_utf8_lossy(&output)
    );
    assert_eq!(fixture.eval("tty", "(length (frame-list))"), "1");
    assert_eq!(
        fixture.eval("tty", "daemon-probe-initial-delete"),
        "\"Attempt to delete daemon's initial frame\""
    );
    assert_eq!(fixture.eval("tty", "(+ 2 3)"), "5");
    assert_eq!(
        fixture.eval("tty", "tty-erase-char"),
        "8",
        "the attaching terminal's ERASE byte must replace the daemon's"
    );
    assert_eq!(
        format!("{:?}", rustix::termios::tcgetattr(&slave).unwrap()),
        original_modes,
        "detach did not restore the client terminal"
    );

    // Exercise host-owned terminal cleanup, not just the default Lisp hook:
    // shutdown must restore an attached client even if a user disables hooks.
    let shutdown_errors = tempfile::NamedTempFile::new().unwrap();
    let mut shutdown = fixture.client("tty", "(progn (setq kill-emacs-hook nil) (kill-emacs 9))");
    shutdown
        .args(["-t", "-w", "30"])
        .env("TERM", "xterm-256color")
        .stdout(Stdio::from(slave.try_clone().unwrap()))
        .stdin(Stdio::from(slave.try_clone().unwrap()))
        .stderr(shutdown_errors.reopen().unwrap());
    let mut shutdown_client = shutdown.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        let mut bytes = [0; 4096];
        while let Ok(count) = master.read(&mut bytes) {
            if count == 0 {
                break;
            }
        }
        if let Some(status) = shutdown_client.try_wait().unwrap() {
            assert!(
                status.success(),
                "{}",
                fs::read_to_string(shutdown_errors.path()).unwrap()
            );
            break;
        }
        if Instant::now() >= deadline {
            let _ = shutdown_client.kill();
            let _ = shutdown_client.wait();
            panic!("attached client shutdown timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = fixture.daemon.as_mut().unwrap().try_wait().unwrap() {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "daemon did not exit after attached-client shutdown"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(status.code(), Some(9));
    assert_eq!(
        format!("{:?}", rustix::termios::tcgetattr(&slave).unwrap()),
        original_modes,
        "daemon exit did not restore the client terminal"
    );
}

#[test]
fn daemon_graphical_request_fails_without_creating_a_phantom_frame() {
    let mut fixture = Fixture::new();
    fixture.foreground("graphical", &["-Q"]);
    let mut command = fixture.client("graphical", "t");
    command.arg("-c").env("DISPLAY", ":987");
    let output = bounded(command, Duration::from_secs(10));
    assert!(!output.status.success(), "{output:?}");
    assert_eq!(fixture.eval("graphical", "(length (frame-list))"), "1");
    assert_eq!(fixture.eval("graphical", "(+ 3 4)"), "7");
}

#[test]
fn full_socket_backlog_has_a_bounded_client_wait_without_duplicate_startup() {
    let fixture = Fixture::new();
    fixture.write_init("");
    fs::create_dir(fixture.path("runtime/emacs")).unwrap();
    fs::set_permissions(
        fixture.path("runtime/emacs"),
        fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    let listener =
        socket2::Socket::new(socket2::Domain::UNIX, socket2::Type::STREAM, None).unwrap();
    let address = socket2::SockAddr::unix(fixture.socket("backlogged")).unwrap();
    listener.bind(&address).unwrap();
    listener.listen(1).unwrap();
    let mut connections = Vec::new();
    loop {
        let connection =
            socket2::Socket::new(socket2::Domain::UNIX, socket2::Type::STREAM, None).unwrap();
        connection.set_nonblocking(true).unwrap();
        match connection.connect(&address) {
            Ok(()) => connections.push(connection),
            Err(error) => {
                assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
                break;
            }
        }
    }
    let mut command = fixture.client("backlogged", "t");
    command.args(["-a", "", "--startup-timeout", "1"]);
    let output = bounded(command, Duration::from_secs(5));
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("timed out connecting to existing daemon")
    );
    assert!(
        !fixture.path("owned-pid").exists(),
        "full backlog launched a second daemon"
    );
}

#[test]
fn auto_start_waits_for_initialization_even_when_init_binds_server_early() {
    let fixture = Fixture::new();
    fixture.write_init(&format!(
        "(require 'server) (setq server-name (daemonp)) (server-start) \
         (while (not (file-exists-p {:?})) (sleep-for 0.01)) \
         (setq daemon-probe-ready t daemon-probe-counter 0) \
         (add-hook 'emacs-startup-hook (lambda () (setq daemon-probe-startup-hook t))) \
         (with-temp-file {:?} (insert \"done\"))",
        fixture.path("release-init"),
        fixture.path("init-done"),
    ));
    let request = format!(
        "(progn (with-temp-file {:?} (insert \"requested\")) \
         (list (emacs-pid) daemon-probe-ready daemon-probe-startup-hook \
         (setq daemon-probe-counter (1+ daemon-probe-counter))))",
        fixture.path("request-ran"),
    );
    let mut handles = Vec::new();
    for _ in 0..4 {
        let mut command = fixture.client("early", &request);
        command.args(["-a", "", "-w", "30"]);
        handles.push(std::thread::spawn(move || {
            bounded(command, Duration::from_secs(40))
        }));
    }
    wait_for_path(&fixture.socket("early"), Duration::from_secs(25));
    assert!(
        !fixture.socket("server").exists(),
        "init bound the wrong endpoint"
    );
    assert!(!fixture.path("init-done").exists());
    // This client starts only after the selected endpoint has been bound. Its
    // startup deadline must expire on the active lock, not on a sent request.
    let mut late = fixture.client("early", &request);
    late.env("ALTERNATE_EDITOR", "")
        .args(["--startup-timeout", "1"]);
    let late_output = bounded(late, Duration::from_secs(5));
    let requested_early = fixture.path("request-ran").exists();
    // The selected socket really is bound while initialization is blocked.
    // Releasing init, not merely seeing that socket, permits the requests.
    fs::write(fixture.path("release-init"), "release").unwrap();
    assert!(!late_output.status.success(), "{late_output:?}");
    assert!(
        String::from_utf8_lossy(&late_output.stderr)
            .contains("timed out waiting for daemon startup"),
        "{late_output:?}"
    );
    assert!(!requested_early, "client submitted before initialization");
    let outputs: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect();
    let pid = fs::read_to_string(fixture.path("owned-pid")).unwrap();
    let mut replies = Vec::new();
    for output in outputs {
        assert!(output.status.success(), "{output:?}");
        replies.push(String::from_utf8(output.stdout).unwrap().trim().to_owned());
    }
    replies.sort();
    assert_eq!(
        replies,
        (1..=4)
            .map(|count| format!("({pid} t t {count})"))
            .collect::<Vec<_>>()
    );
    assert_eq!(fixture.eval("early", "daemon-probe-counter"), "4");
    assert!(fixture.path("init-done").exists());
    fixture.eval("early", "(kill-emacs)");
}

// A gate reached by the real init file, before or after an early listener.
fn gate_startup(fixture: &Fixture, early: bool) {
    fixture.write_init(&format!(
        "{} (with-temp-buffer \
             (call-process \"/bin/sh\" nil t nil \"-c\" \
               \"for fd in /proc/self/fd/*; do readlink \\\"$fd\\\"; done\") \
             (write-region (point-min) (point-max) {:?})) \
         (with-temp-file {:?} (prin1 (list (getenv \"NEOMACS_DAEMON_LOCK_FD\") (getenv \"NEOMACS_DAEMON_NOTIFY_FD\")) (current-buffer))) \
         (with-temp-file {:?} (insert \"entered\")) \
         (while (not (file-exists-p {:?})) (sleep-for 0.01)) \
         (setq daemon-probe-ready t daemon-probe-counter 0)",
        if early {
            "(require 'server) (setq server-name (daemonp)) (server-start)"
        } else {
            ""
        },
        fixture.path("subprocess-fds"),
        fixture.path("internal-environment"),
        fixture.path("init-gate"),
        fixture.path("release-init"),
    ));
}

fn assert_startup_locked(fixture: &Fixture, name: &str) {
    let lock = fs::File::open(fixture.socket(&format!("{name}.startup-lock"))).unwrap();
    // SAFETY: lock is owned and live, separate from the starter's open description.
    assert_eq!(
        unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
        -1
    );
    assert_eq!(
        std::io::Error::last_os_error().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn reply_timeout_starts_after_gated_initialization_and_zero_is_rejected() {
    let fixture = Fixture::new();
    gate_startup(&fixture, false);
    let mut command = fixture.client("reply", "(list (emacs-pid) daemon-probe-ready)");
    command.args(["-a", "", "-w", "1"]);
    let mut client = OwnedChild(
        command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let marker_deadline = Instant::now() + Duration::from_secs(25);
    while !fixture.path("init-gate").exists() {
        assert!(
            client.0.try_wait().unwrap().is_none(),
            "reply budget ended client during startup"
        );
        assert!(Instant::now() < marker_deadline, "init did not enter gate");
        std::thread::sleep(Duration::from_millis(10));
    }
    std::thread::sleep(Duration::from_millis(1300));
    assert!(
        client.0.try_wait().unwrap().is_none(),
        "reply budget killed startup"
    );
    fs::write(fixture.path("release-init"), "release").unwrap();
    assert!(wait_child(&mut client.0, Duration::from_secs(15)).success());
    // GNU rejects a zero reply timeout outright — `Invalid timeout: "0"`,
    // exit 1 (lib-src/emacsclient.c:540-549) — so zero is not a way to ask for
    // an unlimited reply budget; only the absence of `-w` is.
    let mut zero = fixture.client("reply", "(progn (sleep-for 1.3) daemon-probe-ready)");
    zero.args(["-w", "0"]);
    let output = bounded(zero, Duration::from_secs(10));
    assert!(!output.status.success(), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Invalid timeout: \"0\""),
        "{output:?}"
    );
    fixture.eval("reply", "(kill-emacs)");
}

#[test]
fn initiating_client_death_preserves_startup_owner_before_and_after_bind() {
    for early in [false, true] {
        for signal in [libc::SIGTERM, libc::SIGKILL] {
            let fixture = Fixture::new();
            gate_startup(&fixture, early);
            let request = format!(
                "(progn (with-temp-file {:?} (insert \"bad\")) t)",
                fixture.path("abandoned-request")
            );
            let mut command = fixture.client("abandon", &request);
            command.args(["-a", "", "-w", "30"]);
            let mut initiator = OwnedChild(
                command
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .unwrap(),
            );
            wait_for_path(&fixture.path("init-gate"), Duration::from_secs(25));
            let descriptors = fs::read_to_string(fixture.path("subprocess-fds")).unwrap();
            assert!(
                !descriptors.contains("socket:") && !descriptors.contains(".startup-lock"),
                "{descriptors}"
            );
            assert_eq!(
                fs::read_to_string(fixture.path("internal-environment")).unwrap(),
                "(nil nil)"
            );
            assert_eq!(fixture.socket("abandon").exists(), early);
            let pid: i32 = fs::read_to_string(fixture.path("owned-pid"))
                .unwrap()
                .parse()
                .unwrap();
            let daemon = PinnedProcess::new(pid);
            let parent: i32 = fs::read_to_string(format!("/proc/{pid}/status"))
                .unwrap()
                .lines()
                .find_map(|line| line.strip_prefix("PPid:"))
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            let launcher = PinnedProcess::new(parent);
            let initiator_identity = PinnedProcess::new(initiator.0.id() as i32);
            initiator_identity.signal(signal);
            assert!(!wait_child(&mut initiator.0, Duration::from_secs(5)).success());
            assert_startup_locked(&fixture, "abandon");
            let request = format!(
                "(progn (with-temp-file {:?} (insert \"ran\")) (list (emacs-pid) daemon-probe-ready (setq daemon-probe-counter (1+ daemon-probe-counter))))",
                fixture.path("later-request")
            );
            let mut handles = Vec::new();
            for _ in 0..3 {
                let mut later = fixture.client("abandon", &request);
                later.args(["-a", ""]);
                handles.push(std::thread::spawn(move || {
                    bounded(later, Duration::from_secs(30))
                }));
            }
            std::thread::sleep(Duration::from_millis(300));
            assert_startup_locked(&fixture, "abandon");
            assert_eq!(
                fs::read_to_string(fixture.path("owned-pid")).unwrap(),
                pid.to_string()
            );
            assert!(!fixture.path("later-request").exists());
            fs::write(fixture.path("release-init"), "release").unwrap();
            let mut replies = Vec::new();
            for handle in handles {
                let output = handle.join().unwrap();
                assert!(output.status.success(), "{output:?}");
                replies.push(String::from_utf8(output.stdout).unwrap().trim().to_owned());
            }
            replies.sort();
            assert_eq!(
                replies,
                (1..=3)
                    .map(|n| format!("({pid} t {n})"))
                    .collect::<Vec<_>>()
            );
            assert!(!fixture.path("abandoned-request").exists());
            launcher.assert_exited();
            fixture.eval("abandon", "(kill-emacs)");
            daemon.assert_exited();
        }
    }
}

#[test]
fn direct_background_startup_waits_for_gate_and_does_not_leak_capabilities() {
    let fixture = Fixture::new();
    fixture.write_init(&format!(
        "(with-temp-buffer \
           (call-process \"/bin/sh\" nil t nil \"-c\" \
             \"for fd in /proc/self/fd/*; do readlink \\\"$fd\\\"; done\") \
           (write-region (point-min) (point-max) {:?})) \
         (with-temp-file {:?} (prin1 (list (getenv \"NEOMACS_DAEMON_LOCK_FD\") (getenv \"NEOMACS_DAEMON_NOTIFY_FD\")) (current-buffer))) \
         (with-temp-file {:?} (insert \"entered\")) \
         (while (not (file-exists-p {:?})) (sleep-for 0.01))",
        fixture.path("subprocess-fds"), fixture.path("internal-environment"),
        fixture.path("init-gate"), fixture.path("release-init"),
    ));
    let mut command = fixture.editor();
    command.arg("--bg-daemon=direct");
    let mut launcher = OwnedChild(
        command
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    wait_for_path(&fixture.path("init-gate"), Duration::from_secs(25));
    std::thread::sleep(Duration::from_millis(1300));
    assert!(launcher.0.try_wait().unwrap().is_none());
    let fds = fs::read_to_string(fixture.path("subprocess-fds")).unwrap();
    assert!(
        !fds.contains("socket:") && !fds.contains(".startup-lock"),
        "{fds}"
    );
    assert_eq!(
        fs::read_to_string(fixture.path("internal-environment")).unwrap(),
        "(nil nil)"
    );
    fs::write(fixture.path("release-init"), "release").unwrap();
    assert!(wait_child(&mut launcher.0, Duration::from_secs(15)).success());
    fixture.eval("direct", "(kill-emacs)");
}

#[test]
fn startup_wait_expiry_leaves_initializer_owned_and_never_replays_request() {
    for early in [false, true] {
        let fixture = Fixture::new();
        gate_startup(&fixture, early);
        // Expire the initiating client's explicitly separate startup budget
        // while the surviving launcher still owns initialization.
        let mut command = fixture.client(
            "slow",
            "(setq daemon-probe-counter (1+ daemon-probe-counter))",
        );
        command.args(["-a", "", "--startup-timeout", "8"]);
        let mut client = OwnedChild(
            command
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        // The startup budget can expire before Lisp reaches the gate (for
        // example while loading server.el). The owner must still reach it;
        // do not require fixture preparation to fit inside the client budget.
        wait_for_path(&fixture.path("init-gate"), Duration::from_secs(25));
        let pid: i32 = fs::read_to_string(fixture.path("owned-pid"))
            .unwrap()
            .parse()
            .unwrap();
        let daemon = PinnedProcess::new(pid);
        assert!(!wait_child(&mut client.0, Duration::from_secs(10)).success());
        assert_startup_locked(&fixture, "slow");
        assert_eq!(fixture.socket("slow").exists(), early);
        fs::write(fixture.path("release-init"), "release").unwrap();
        let mut later = fixture.client(
            "slow",
            "(list (emacs-pid) daemon-probe-ready daemon-probe-counter)",
        );
        later.args(["-a", ""]);
        let output = bounded(later, Duration::from_secs(15));
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            format!("({pid} t 0)")
        );
        fixture.eval("slow", "(kill-emacs)");
        daemon.assert_exited();
    }
}
