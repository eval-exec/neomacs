//! #688: exercise real client/server GUI creation, including client lifetimes.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{DisplayHarness, DisplaySession, GuiBackend, run_input_tool};
use neomacs_test_env::{CanonicalEnvironment, CanonicalPin, CanonicalProfile, PinnedCommand};
use std::{
    fs::{self, File},
    os::fd::{AsRawFd, FromRawFd},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const DEADLINE: Duration = Duration::from_secs(45);
const FIND_VISIBLE_GUI: &str = r#"
(progn
  (setq neomacsclient-test-frame nil)
  (dolist (frame (frame-list))
    (when (and (eq (window-system frame) 'neo)
               (display-graphic-p frame)
               (eq (frame-visible-p frame) t))
      (setq neomacsclient-test-frame frame)))
  (and neomacsclient-test-frame t))
"#;

struct OwnedChild(Child);

impl OwnedChild {
    /// Bound child completion while retaining ownership for panic cleanup.
    fn wait(&mut self, description: &str, artifacts: &Path) -> ExitStatus {
        let deadline = Instant::now() + DEADLINE;
        loop {
            if let Some(status) = self.0.try_wait().expect("observe owned child") {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "{description} timed out; artifacts: {}",
                artifacts.display()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for OwnedChild {
    /// Reap every owned child, including when an assertion unwinds the fixture.
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Keep a real terminal attached to both client modes, as in the reported
/// invocation. The child is reaped before either PTY end closes on panic.
struct Client {
    child: OwnedChild,
    _master: File,
    _slave: File,
}

impl Client {
    /// Attach both standard streams to a sized PTY so client mode selection
    /// follows the terminal invocation reported in issue #688.
    fn spawn(mut command: PinnedCommand, stderr: &Path) -> Self {
        let mut master = -1;
        let mut slave = -1;
        let size = libc::winsize {
            ws_row: 24,
            ws_col: 80,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: writable descriptors, no name buffer, and default PTY modes.
        assert_eq!(
            unsafe {
                libc::openpty(
                    &mut master,
                    &mut slave,
                    std::ptr::null_mut(),
                    std::ptr::null(),
                    &size,
                )
            },
            0,
            "open client PTY: {}",
            std::io::Error::last_os_error()
        );
        // SAFETY: successful openpty transfers two distinct owned descriptors.
        let master = unsafe { File::from_raw_fd(master) };
        // SAFETY: this is the other descriptor returned by successful openpty.
        let slave = unsafe { File::from_raw_fd(slave) };
        for file in [&master, &slave] {
            // SAFETY: file owns a live descriptor throughout these calls.
            let flags = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETFD) };
            assert!(flags >= 0, "read PTY descriptor flags");
            // SAFETY: preserve flags and prevent unrelated children inheriting it.
            assert_eq!(
                unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETFD, flags | libc::FD_CLOEXEC) },
                0,
                "set client PTY close-on-exec"
            );
        }
        let child = OwnedChild(
            command
                .stdin(Stdio::from(slave.try_clone().unwrap()))
                .stdout(Stdio::from(slave.try_clone().unwrap()))
                .stderr(File::create(stderr).unwrap())
                .spawn()
                .expect("start real neomacsclient"),
        );
        Self {
            child,
            _master: master,
            _slave: slave,
        }
    }
}

struct Fixture<'a> {
    daemon: OwnedChild,
    display: &'a DisplaySession,
    environment: CanonicalEnvironment,
    runtime: PathBuf,
    client_binary: PathBuf,
    artifacts: PathBuf,
}

impl<'a> Fixture<'a> {
    /// Start a foreground daemon with an inherited private display and isolated
    /// server paths, retaining logs after the temporary home is removed.
    fn new(display: &'a DisplaySession, isolation: &Path, artifacts: PathBuf) -> Self {
        let root = neomacs_infra::workspace_root();
        for name in ["home", "tmp", "runtime", "config", "data", "cache"] {
            let path = isolation.join(name);
            fs::create_dir(&path).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let environment = CanonicalEnvironment::new(
            CanonicalProfile::Tui,
            isolation.join("home"),
            isolation.join("tmp"),
            &root,
        )
        .unwrap();
        let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("target/release/neomacs"));
        let client_binary = std::env::var_os("NEOMACS_GUI_TEST_CLIENT_BINARY")
            .map(PathBuf::from)
            .unwrap_or_else(|| binary.with_file_name("neomacsclient"));
        assert!(binary.is_file(), "fresh-build the editor: {binary:?}");
        assert!(
            client_binary.is_file(),
            "build the client: {client_binary:?}"
        );
        let runtime = isolation.join("runtime");
        let mut command = environment.command(binary);
        Self::isolate(&mut command, display, isolation, &runtime, &root);
        let daemon = OwnedChild(
            command
                .args(["-Q", "--fg-daemon=gui-client-test"])
                .env("NEOMACS_LOG_FILE", artifacts.join("daemon.log"))
                .stdin(Stdio::null())
                .stdout(File::create(artifacts.join("daemon.stdout")).unwrap())
                .stderr(File::create(artifacts.join("daemon.stderr")).unwrap())
                .spawn()
                .expect("start display-capable daemon"),
        );
        Self {
            daemon,
            display,
            environment,
            runtime,
            client_binary,
            artifacts,
        }
    }

    /// Share one display/socket namespace among the daemon and its clients
    /// without leaking ambient user server selection or overriding locale pins.
    fn isolate(
        command: &mut PinnedCommand,
        display: &DisplaySession,
        isolation: &Path,
        runtime: &Path,
        root: &Path,
    ) {
        command
            .current_dir(root)
            // DisplaySession also supplies locale defaults; the canonical
            // environment above owns those pins. Only copy display identity.
            .envs(
                display
                    .env()
                    .iter()
                    .filter(|(key, _)| matches!(key.as_str(), "DISPLAY" | "XAUTHORITY"))
                    .map(|(key, value)| (key, value)),
            )
            .override_pin(CanonicalPin::XdgRuntimeDir, runtime)
            .override_pin(CanonicalPin::XdgConfigHome, isolation.join("config"))
            .override_pin(CanonicalPin::XdgDataHome, isolation.join("data"))
            .override_pin(CanonicalPin::XdgCacheHome, isolation.join("cache"))
            .override_pin(CanonicalPin::Term, "xterm-256color")
            .env("WINIT_UNIX_BACKEND", "x11")
            .env("WGPU_BACKEND", "vulkan")
            .env("RUST_LOG", "info")
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("WAYLAND_SOCKET")
            .env_remove("EMACS_SERVER_FILE")
            .env_remove("EMACS_SOCKET_NAME")
            .env_remove("ALTERNATE_EDITOR");
    }

    /// Target this fixture's named server with the matching client executable.
    fn client_command(&self) -> PinnedCommand {
        let mut command = self.environment.command(&self.client_binary);
        Self::isolate(
            &mut command,
            self.display,
            self.runtime.parent().unwrap(),
            &self.runtime,
            &neomacs_infra::workspace_root(),
        );
        command.args(["-s", "gui-client-test"]);
        command
    }

    /// Evaluate through the real server protocol, preserving diagnostics on
    /// failure and bounding both the protocol reply and child lifetime.
    fn evaluate(&self, form: &str) -> String {
        let mut command = self.client_command();
        let stdout = self.artifacts.join("eval.stdout");
        let stderr = self.artifacts.join("eval.stderr");
        let mut child = OwnedChild(
            command
                .args(["-w", "10", "-e", form])
                .stdin(Stdio::null())
                .stdout(File::create(&stdout).unwrap())
                .stderr(File::create(&stderr).unwrap())
                .spawn()
                .expect("evaluate through real client/server socket"),
        );
        let status = child.wait("client eval", &self.artifacts);
        assert!(
            status.success(),
            "client eval failed ({status}): {}; artifacts: {}",
            fs::read_to_string(stderr).unwrap_or_default(),
            self.artifacts.display()
        );
        fs::read_to_string(stdout).unwrap().trim().to_owned()
    }

    /// Await an observable condition while treating an exited daemon as an
    /// immediate failure rather than consuming the whole timeout.
    fn wait_for(&mut self, description: &str, mut condition: impl FnMut(&Self) -> bool) {
        let deadline = Instant::now() + DEADLINE;
        loop {
            assert!(
                self.daemon.0.try_wait().unwrap().is_none(),
                "daemon exited while waiting for {description}; artifacts: {}",
                self.artifacts.display()
            );
            if condition(self) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "{description} timed out; artifacts: {}",
                self.artifacts.display()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Find mapped X11 windows by the owned daemon's PID, independent of Lisp's
    /// frame registry, to detect missing or retained native windows.
    fn native_window_ids(&self) -> Vec<String> {
        let mut command = Command::new("xdotool");
        command
            .args(["search", "--onlyvisible", "--pid"])
            .arg(self.daemon.0.id().to_string())
            .envs(self.display.env().iter().map(|(key, value)| (key, value)));
        let output = run_input_tool(command, Duration::from_secs(5));
        assert!(
            output.status.success() || output.status.code() == Some(1),
            "xdotool: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::to_owned)
            .collect()
    }

    /// Count this daemon's mapped native windows on the private display.
    fn native_windows(&self) -> usize {
        self.native_window_ids().len()
    }

    /// Reject a reopened native window sized from the initial terminal's
    /// synthetic one-pixel character metrics.
    fn has_normal_native_geometry(&self) -> bool {
        let windows = self.native_window_ids();
        let [window] = windows.as_slice() else {
            return false;
        };
        let mut command = Command::new("xdotool");
        command
            .args(["getwindowgeometry", "--shell", window])
            .envs(self.display.env().iter().map(|(key, value)| (key, value)));
        let output = run_input_tool(command, Duration::from_secs(5));
        assert!(output.status.success(), "native geometry query: {output:?}");
        let geometry = String::from_utf8_lossy(&output.stdout);
        let dimension = |name| {
            geometry
                .lines()
                .find_map(|line| line.strip_prefix(name))
                .and_then(|value| value.parse::<u32>().ok())
                .expect("xdotool shell geometry dimension")
        };
        // TTY metrics inherited after deleting the last GUI frame would
        // create an approximately 80x25-pixel native window on reopening.
        dimension("WIDTH=") >= 300 && dimension("HEIGHT=") >= 100
    }

    /// Submit a real graphical client request with either server lifetime mode.
    fn open_frame(&self, nowait: bool, name: &str) -> Client {
        let mut command = self.client_command();
        command.args(["-a", "", "--create-frame"]);
        if nowait {
            command.arg("--no-wait");
        }
        Client::spawn(command, &self.artifacts.join(format!("{name}.stderr")))
    }

    /// Require agreement between Lisp visibility/font metrics and native size
    /// before testing client lifetime or frame deletion.
    fn wait_for_gui(&mut self) {
        self.wait_for(
            "visible client GUI frame with graphical font metrics",
            |fixture| {
                fixture.evaluate(FIND_VISIBLE_GUI) == "t"
                    && fixture.evaluate(
                        "(and (> (frame-char-width neomacsclient-test-frame) 1) \
                          (> (frame-char-height neomacsclient-test-frame) 1))",
                    ) == "t"
                    && fixture.has_normal_native_geometry()
            },
        );
    }

    /// Delete the client frame and require native retirement, a responsive
    /// server, and preservation of the initial frame and graphical connection.
    fn delete_gui(&mut self) {
        self.evaluate("(delete-frame neomacsclient-test-frame t)");
        self.wait_for("native client window deletion", |fixture| {
            fixture.native_windows() == 0
        });
        assert_eq!(self.evaluate("(+ 20 22)"), "42");
        assert_eq!(self.evaluate("(length (frame-list))"), "1");
        assert_eq!(
            self.evaluate(
                "(list window-system initial-window-system (length (terminal-list)) baud-rate \
                       (eq (selected-frame) neomacsclient-test-initial-frame) \
                       (and (frame-live-p neomacsclient-test-initial-frame) t))",
            ),
            "(nil nil 2 19200 t t)",
            "deleting the last GUI frame must preserve the daemon's initial frame"
        );
    }

    /// Ensure an unsupported display fails before either frame registry gains
    /// a phantom window, including after graphical initialization.
    fn assert_invalid_display_refused(&self) {
        let before_frames = self.evaluate("(length (frame-list))");
        let before_windows = self.native_windows();
        assert_eq!(
            self.evaluate(
                "(condition-case nil (make-frame-on-display \":987\") (error 'refused))",
            ),
            "refused",
            "GUI host admission must validate the requested display"
        );
        assert_eq!(self.evaluate("(length (frame-list))"), before_frames);
        assert_eq!(self.native_windows(), before_windows);
        assert_eq!(self.evaluate("(+ 20 22)"), "42");
    }
}

/// Exercise both client lifetimes and repeated native close/reopen cycles on
/// one daemon while its initial terminal frame remains usable.
#[test]
// Prerequisites: fresh-built editor/client, Xvfb, xauth, xdotool and Vulkan.
fn neomacsclient_creates_gui_frames_from_a_daemon_with_display() {
    let root = neomacs_infra::workspace_root();
    let artifacts = root.join(format!(
        "target/neomacs-gui-tests/neomacsclient-frames-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    fs::create_dir_all(&artifacts).unwrap();
    let isolation = neomacs_test_env::bounded_temp_dir("gui-client-").unwrap();
    let display = DisplayHarness::for_backend(GuiBackend::LinuxX11)
        .start_session(&artifacts)
        .expect("start private X11 display");
    let mut fixture = Fixture::new(&display, isolation.path(), artifacts);
    fixture.wait_for("daemon socket", |fixture| {
        fixture.runtime.join("emacs/gui-client-test").exists()
    });
    assert_eq!(fixture.evaluate("(+ 20 22)"), "42");
    assert_eq!(
        fixture.native_windows(),
        0,
        "daemon created an initial window"
    );
    assert_eq!(fixture.evaluate("(length (frame-list))"), "1");
    assert_eq!(
        fixture.evaluate(
            "(list window-system initial-window-system (length (terminal-list)) baud-rate)",
        ),
        "(nil nil 1 0)",
    );
    assert_eq!(
        fixture.evaluate("(list (window-system) initial-window-system frame-initial-frame)"),
        "(nil nil nil)",
        "daemon startup must retain its initial terminal frame"
    );
    fixture.evaluate("(setq neomacsclient-test-initial-frame (selected-frame))");
    fixture.assert_invalid_display_refused();

    let mut nowait = fixture.open_frame(true, "nowait");
    let status = nowait.child.wait("no-wait client", &fixture.artifacts);
    assert!(
        status.success(),
        "no-wait GUI request failed ({status}): {}; artifacts: {}",
        fs::read_to_string(fixture.artifacts.join("nowait.stderr")).unwrap_or_default(),
        fixture.artifacts.display(),
    );
    fixture.wait_for_gui();
    // An initialized GUI host must still validate later display requests.
    fixture.assert_invalid_display_refused();
    // The client has exited; its native frame must stay usable independently.
    assert_eq!(
        fixture.evaluate("(and (frame-live-p neomacsclient-test-frame) t)"),
        "t"
    );
    assert_eq!(fixture.evaluate("(+ 20 22)"), "42");
    fixture.delete_gui();

    let mut waiting = fixture.open_frame(false, "waiting");
    fixture.wait_for_gui();
    assert!(
        waiting.child.0.try_wait().unwrap().is_none(),
        "waiting client detached before its GUI frame was deleted"
    );
    fixture.delete_gui();
    let status = waiting
        .child
        .wait("waiting client after deletion", &fixture.artifacts);
    assert!(status.success(), "waiting client failed: {status}");

    // A daemon remains a GUI host after its last client window disappears.
    let mut reopened = fixture.open_frame(true, "reopened");
    let status = reopened
        .child
        .wait("reopened GUI client", &fixture.artifacts);
    assert!(status.success(), "reopened GUI request failed: {status}");
    fixture.wait_for_gui();
    fixture.delete_gui();
}
