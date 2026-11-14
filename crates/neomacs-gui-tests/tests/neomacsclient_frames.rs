//! #688: exercise real client/server GUI creation, including client lifetimes.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{DisplayHarness, DisplaySession, GuiBackend, run_input_tool};
use neomacs_test_env::{CanonicalEnvironment, CanonicalPin, CanonicalProfile, PinnedCommand};
use std::{
    ffi::OsString,
    fs::{self, File},
    os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd},
    os::unix::ffi::{OsStrExt, OsStringExt},
    os::unix::fs::{MetadataExt, PermissionsExt},
    os::unix::net::UnixStream,
    os::unix::process::ExitStatusExt,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

// Unique within this test process, including concurrent fixture evaluations.
static EVALUATE_ORDINAL: AtomicU64 = AtomicU64::new(1);

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

// Copy only the display's graphics selection: canonical locale/identity pins
// still belong to CanonicalEnvironment, including for daemon/client commands.
fn apply_display_environment(command: &mut PinnedCommand, environment: &[(String, String)]) {
    command.envs(
        environment
            .iter()
            .filter(|(key, _)| {
                matches!(
                    key.as_str(),
                    "DISPLAY"
                        | "XAUTHORITY"
                        | "WAYLAND_DISPLAY"
                        | "WAYLAND_SOCKET"
                        | "VK_DRIVER_FILES"
                        | "VK_ICD_FILENAMES"
                )
            })
            .map(|(key, value)| (key, value)),
    );
}

// Independently validate the owned session's policy before comparing the
// daemon. Ambient inheritance alone must not conceal deleted session wiring.
fn expected_vulkan_environment(
    display_environment: &[(String, String)],
    caller: [Option<OsString>; 2],
    keep_caller_selection: bool,
) -> ([Option<OsString>; 2], serde_json::Value) {
    let keys = ["VK_DRIVER_FILES", "VK_ICD_FILENAMES"];
    let architecture = match std::env::consts::ARCH {
        "x86" => "i686",
        architecture => architecture,
    };
    let host_manifest = format!("lvp_icd.{architecture}.json");
    let mut caller_lavapipe = Vec::new();
    for (index, value) in caller.iter().enumerate() {
        if let Some(value) = value {
            for path in std::env::split_paths(value) {
                let lavapipe = path.to_str().is_some()
                    && path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name == "lvp_icd.json" || name == host_manifest);
                if lavapipe {
                    caller_lavapipe.push((index, path));
                }
            }
        }
    }
    let caller_receipt: Vec<_> = caller
        .iter()
        .map(|value| {
            value
                .as_ref()
                .map(|value| value.to_string_lossy().into_owned())
        })
        .collect();
    if !keep_caller_selection && !caller_lavapipe.is_empty() {
        let selected: Vec<_> = keys
            .iter()
            .map(|key| {
                let matches: Vec<_> = display_environment
                    .iter()
                    .filter(|(name, _)| name.as_str() == *key)
                    .collect();
                assert_eq!(
                    matches.len(),
                    1,
                    "owned Xvfb must publish exactly one {key}"
                );
                OsString::from(matches[0].1.as_str())
            })
            .collect();
        assert_eq!(
            selected[0], selected[1],
            "owned Xvfb loader variables must agree"
        );
        let paths: Vec<_> = std::env::split_paths(&selected[0]).collect();
        assert_eq!(
            paths.len(),
            1,
            "owned Xvfb must select a singleton software ICD"
        );
        let (source, _) = caller_lavapipe
            .iter()
            .find(|(_, path)| path == &paths[0])
            .expect("owned Xvfb selected an ICD absent from the caller's lavapipe entries");
        let original = caller[*source].as_ref().unwrap();
        let caller_was_mixed = std::env::split_paths(original).count() > 1;
        let caller_was_narrowed = caller_was_mixed && original != &selected[0];
        assert!(
            !caller_was_mixed || caller_was_narrowed,
            "mixed caller ICD list was not narrowed"
        );
        let receipt = serde_json::json!({
            "policy_default_active": true,
            "caller_loader_variables": caller_receipt,
            "selected_singleton": selected[0].to_string_lossy(),
            "selected_from_caller_variable": keys[*source],
            "caller_list_was_mixed": caller_was_mixed,
            "caller_list_was_narrowed": caller_was_narrowed,
        });
        return (
            [Some(selected[0].clone()), Some(selected[1].clone())],
            receipt,
        );
    }
    for (index, key) in keys.iter().enumerate() {
        for (_, value) in display_environment
            .iter()
            .filter(|(name, _)| name.as_str() == *key)
        {
            assert_eq!(
                Some(std::ffi::OsStr::new(value)),
                caller[index].as_deref(),
                "owned Xvfb changed caller {key} while selection was inactive"
            );
        }
    }
    let receipt = serde_json::json!({
        "policy_default_active": false,
        "opt_out": keep_caller_selection,
        "caller_loader_variables": caller_receipt,
        "caller_list_was_narrowed": false,
    });
    (caller, receipt)
}

#[derive(Clone, Copy)]
enum DisplaySelection {
    PinnedX11,
    FromEnvironment,
}

/// PinnedCommand deliberately exposes no pre-exec hook. Keep the one real
/// compositor connection inheritable only during the owned daemon's spawn.
struct InheritedWaylandDescriptor<'a> {
    descriptor: BorrowedFd<'a>,
    flags: libc::c_int,
}

impl<'a> InheritedWaylandDescriptor<'a> {
    fn new(descriptor: BorrowedFd<'a>) -> Self {
        // SAFETY: the borrowed descriptor remains owned throughout the spawn.
        let flags = unsafe { libc::fcntl(descriptor.as_raw_fd(), libc::F_GETFD) };
        assert!(flags >= 0, "read Wayland socket descriptor flags");
        // SAFETY: preserve every descriptor flag except close-on-exec.
        assert_eq!(
            unsafe {
                libc::fcntl(
                    descriptor.as_raw_fd(),
                    libc::F_SETFD,
                    flags & !libc::FD_CLOEXEC,
                )
            },
            0,
            "make the supplied Wayland descriptor inheritable"
        );
        Self { descriptor, flags }
    }
}

impl Drop for InheritedWaylandDescriptor<'_> {
    fn drop(&mut self) {
        // SAFETY: the borrowed descriptor is still owned; restore its exact flags.
        assert_eq!(
            unsafe { libc::fcntl(self.descriptor.as_raw_fd(), libc::F_SETFD, self.flags) },
            0,
            "restore supplied descriptor flags"
        );
    }
}

struct Fixture<'a> {
    daemon: OwnedChild,
    display: &'a DisplaySession,
    display_environment: Vec<(String, String)>,
    display_selection: DisplaySelection,
    environment: CanonicalEnvironment,
    runtime: PathBuf,
    client_binary: PathBuf,
    artifacts: PathBuf,
}

impl<'a> Fixture<'a> {
    /// Start a foreground daemon with an inherited private display and isolated
    /// server paths, retaining logs after the temporary home is removed.
    fn new(display: &'a DisplaySession, isolation: &Path, artifacts: PathBuf) -> Self {
        Self::with_display_environment(
            display,
            isolation,
            artifacts,
            display.env().to_vec(),
            DisplaySelection::PinnedX11,
            None,
        )
    }

    fn with_display_environment(
        display: &'a DisplaySession,
        isolation: &Path,
        artifacts: PathBuf,
        display_environment: Vec<(String, String)>,
        display_selection: DisplaySelection,
        inherited_descriptor: Option<BorrowedFd<'_>>,
    ) -> Self {
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
        Self::isolate(
            &mut command,
            &display_environment,
            display_selection,
            isolation,
            &runtime,
            &root,
        );
        command
            .args(["-Q", "--fg-daemon=gui-client-test"])
            .env("NEOMACS_LOG_FILE", artifacts.join("daemon.log"))
            .stdin(Stdio::null())
            .stdout(File::create(artifacts.join("daemon.stdout")).unwrap())
            .stderr(File::create(artifacts.join("daemon.stderr")).unwrap());
        let daemon = {
            let _inheritance = inherited_descriptor.map(InheritedWaylandDescriptor::new);
            OwnedChild(command.spawn().expect("start display-capable daemon"))
        };
        Self {
            daemon,
            display,
            display_environment,
            display_selection,
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
        display_environment: &[(String, String)],
        display_selection: DisplaySelection,
        isolation: &Path,
        runtime: &Path,
        root: &Path,
    ) {
        command.current_dir(root);
        for name in ["DISPLAY", "XAUTHORITY", "WAYLAND_DISPLAY", "WAYLAND_SOCKET"] {
            command.env_remove(name);
        }
        apply_display_environment(command, display_environment);
        command
            .override_pin(CanonicalPin::XdgRuntimeDir, runtime)
            .override_pin(CanonicalPin::XdgConfigHome, isolation.join("config"))
            .override_pin(CanonicalPin::XdgDataHome, isolation.join("data"))
            .override_pin(CanonicalPin::XdgCacheHome, isolation.join("cache"))
            .override_pin(CanonicalPin::Term, "xterm-256color")
            .env("RUST_LOG", "info")
            .env_remove("EMACS_SERVER_FILE")
            .env_remove("EMACS_SOCKET_NAME")
            .env_remove("ALTERNATE_EDITOR");
        match display_selection {
            DisplaySelection::PinnedX11 => {
                command
                    .env("WINIT_UNIX_BACKEND", "x11")
                    .env("WGPU_BACKEND", "vulkan")
                    .env_remove("WAYLAND_DISPLAY")
                    .env_remove("WAYLAND_SOCKET");
            }
            DisplaySelection::FromEnvironment => {
                for name in [
                    "WINIT_UNIX_BACKEND",
                    "GDK_BACKEND",
                    "WGPU_BACKEND",
                    "WGPU_BACKENDS",
                    "NEOMACS_GUI_TEST_BACKEND",
                ] {
                    command.env_remove(name);
                }
            }
        }
    }

    /// Read the real daemon's environment, rather than assuming the spawn
    /// builder propagated the owned display's selected Vulkan driver.
    fn assert_daemon_vulkan_environment(&self) {
        let path = PathBuf::from(format!("/proc/{}/environ", self.daemon.0.id()));
        let environment = fs::read(&path).expect("read owned daemon environment");
        let keep = std::env::var_os("NEOMACS_XVFB_KEEP_VK_ICD").is_some_and(|value| value == "1");
        let (expected, policy_receipt) = expected_vulkan_environment(
            self.display.env(),
            [
                std::env::var_os("VK_DRIVER_FILES"),
                std::env::var_os("VK_ICD_FILENAMES"),
            ],
            keep,
        );
        let mut observed = serde_json::Map::new();
        for (index, key) in ["VK_DRIVER_FILES", "VK_ICD_FILENAMES"].iter().enumerate() {
            let prefix = format!("{key}=");
            let actual = environment
                .split(|byte| *byte == 0)
                .find_map(|entry| entry.strip_prefix(prefix.as_bytes()))
                .map(|value| OsString::from_vec(value.to_vec()));
            observed.insert(
                (*key).to_owned(),
                actual
                    .as_ref()
                    .map(|value| serde_json::Value::String(value.to_string_lossy().into_owned()))
                    .unwrap_or(serde_json::Value::Null),
            );
            assert_eq!(
                actual, expected[index],
                "owned daemon did not inherit {key}"
            );
        }
        let receipt = serde_json::json!({
            "pid": self.daemon.0.id(),
            "source": path,
            "actual_daemon_environment": observed,
            "owned_session_policy": policy_receipt,
        });
        fs::write(
            self.artifacts.join("daemon-vulkan-environment.json"),
            serde_json::to_vec_pretty(&receipt).unwrap(),
        )
        .expect("preserve actual daemon Vulkan environment");
    }

    /// Target this fixture's named server with the matching client executable.
    fn client_command(&self) -> PinnedCommand {
        let mut command = self.environment.command(&self.client_binary);
        Self::isolate(
            &mut command,
            &self.display_environment,
            self.display_selection,
            self.runtime.parent().unwrap(),
            &self.runtime,
            &neomacs_infra::workspace_root(),
        );
        // The native daemon consumed this descriptor during startup. Protocol
        // clients use the server socket and must not inherit its old fd number.
        command.env_remove("WAYLAND_SOCKET");
        command.args(["-s", "gui-client-test"]);
        command
    }

    /// Evaluate through the real server protocol, preserving diagnostics on
    /// failure and bounding both the protocol reply and child lifetime.
    fn evaluate(&self, form: &str) -> String {
        let ordinal = EVALUATE_ORDINAL.fetch_add(1, Ordering::Relaxed);
        let wall_ns = || {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .ok()
                .map(|duration| duration.as_nanos().to_string())
        };
        let started_unix_ns = wall_ns();
        let caller_pid = std::process::id();
        let history = self.artifacts.join(format!(
            "evaluate-{}-{caller_pid}-{ordinal}",
            started_unix_ns.as_deref().unwrap_or("unknown")
        ));
        // Exclusive creation refuses reuse, including if clock/PID/ordinal repeat.
        fs::create_dir(&history).expect("create client evaluation history");
        fs::write(history.join("form.el"), form.as_bytes()).expect("preserve full client form");
        // This is the exact source-constructed argv, not a /proc argv observation.
        // PinnedCommand has no argv getter; no environment values are collected.
        let mut metadata = serde_json::json!({
            "phase": "CALLER_ATTEMPT_BEFORE_SPAWN",
            "ordinal": ordinal,
            "caller_pid": caller_pid,
            "started_unix_ns": started_unix_ns,
            "client_path_display": self.client_binary.to_string_lossy(),
            "client_path_unix_bytes": self.client_binary.as_os_str().as_bytes(),
            "argv_source": "Fixture::client_command + Fixture::evaluate literals",
            "argv": [
                self.client_binary.to_string_lossy(),
                "-s", "gui-client-test", "-w", "10", "-e", form
            ],
            "child_pid": null,
            "child_start_ticks_observed": null,
            "completed_unix_ns": null,
            "raw_unix_exit_status": null,
        });
        fs::write(
            history.join("attempt.json"),
            serde_json::to_vec_pretty(&metadata).unwrap(),
        )
        .expect("preserve client evaluation attempt");
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
        let child_pid = child.0.id();
        // This entire test module is Linux-only. A missing/racing /proc read is
        // diagnostic absence. Keep this observation in memory until wait returns.
        let child_start_ticks = fs::read_to_string(format!("/proc/{child_pid}/stat"))
            .ok()
            .and_then(|stat| {
                stat.rsplit_once(')')
                    .and_then(|(_, fields)| fields.split_whitespace().nth(19)?.parse::<u64>().ok())
            });
        let status = child.wait("client eval", &self.artifacts);
        let completed_unix_ns = wall_ns();
        // Preserve complete bytes before the unchanged success assertion, even
        // when the client failed. Legacy output paths and readers stay intact.
        fs::copy(&stdout, history.join("stdout")).expect("preserve client stdout history");
        fs::copy(&stderr, history.join("stderr")).expect("preserve client stderr history");
        metadata["phase"] = "CALLER_COMPLETED".into();
        metadata["child_pid"] = child_pid.into();
        metadata["child_start_ticks_observed"] = serde_json::to_value(child_start_ticks).unwrap();
        metadata["child_observation"] = "Before wait; persisted only after wait returned".into();
        metadata["completed_unix_ns"] = serde_json::to_value(completed_unix_ns).unwrap();
        metadata["raw_unix_exit_status"] = status.into_raw().into();
        metadata["exit_status_display"] = status.to_string().into();
        metadata["exit_code"] = serde_json::to_value(status.code()).unwrap();
        fs::write(
            history.join("metadata.json"),
            serde_json::to_vec_pretty(&metadata).unwrap(),
        )
        .expect("preserve client evaluation completion");

        assert!(
            status.success(),
            "client eval failed ({status}): {}; artifacts: {}",
            fs::read_to_string(stderr).unwrap_or_default(),
            self.artifacts.display()
        );
        fs::read_to_string(stdout).unwrap().trim().to_owned()
    }

    /// Compare Elisp data without depending on the client pretty printer.
    fn assert_eval_data(&self, form: &str, expected_literal: &str, label: &str) -> String {
        self.evaluate(&format!(
            "(progn (setq neomacsclient-test-observation {form}) neomacsclient-test-observation)"
        ));
        // Preserve the actual protocol bytes before the equality query replaces
        // eval.stdout. These bytes are diagnostic evidence, not an assertion format.
        let raw = fs::read(self.artifacts.join("eval.stdout")).unwrap();
        fs::write(self.artifacts.join(format!("{label}.el")), &raw).unwrap();
        let observed = String::from_utf8(raw).unwrap();
        assert_eq!(
            self.evaluate(&format!(
                "(equal neomacsclient-test-observation '{expected_literal})"
            )),
            "t",
            "{label}: expected Elisp data {expected_literal}; actual protocol reply: {observed}; artifacts: {}",
            self.artifacts.display()
        );
        observed
    }

    fn wait_for_socket(&mut self) {
        self.wait_for("daemon socket", |fixture| {
            fixture.runtime.join("emacs/gui-client-test").exists()
        });
    }

    fn assert_boot_identity(&self) {
        let observed = self.evaluate(
            "(list x-display-name initial-window-system (window-system) (terminal-name))",
        );
        fs::write(self.artifacts.join("boot-display-identity.el"), &observed).unwrap();
        assert_eq!(
            observed, "(nil nil nil \"initial_terminal\")",
            "daemon startup must not seed a GUI display identity"
        );
    }

    fn assert_graphical_identity(&self, display: &str, x_display_name: Option<&str>, label: &str) {
        let name = x_display_name
            .map(|name| serde_json::to_string(name).unwrap())
            .unwrap_or_else(|| "nil".to_owned());
        let display = serde_json::to_string(display).unwrap();
        self.assert_eval_data(
            "(list x-display-name \
                   (terminal-name (frame-terminal neomacsclient-test-frame)) \
                   (frame-parameter neomacsclient-test-frame 'display) \
                   (and (display-graphic-p neomacsclient-test-frame) t) \
                   initial-window-system)",
            &format!("({name} {display} {display} t nil)"),
            &format!("{label}-display-identity"),
        );
    }

    // /proc/environ records the original exec environment block, including
    // descriptors later consumed/unset. The separate Lisp hygiene assertion
    // checks the logical environment after startup has consumed a live socket.
    fn assert_unforced_daemon_environment(&self) {
        let path = PathBuf::from(format!("/proc/{}/environ", self.daemon.0.id()));
        let bytes = fs::read(&path).expect("read actual owned daemon display environment");
        let mut receipt = serde_json::Map::new();
        for name in [
            "DISPLAY",
            "XAUTHORITY",
            "WAYLAND_DISPLAY",
            "WAYLAND_SOCKET",
            "WINIT_UNIX_BACKEND",
            "GDK_BACKEND",
            "WGPU_BACKEND",
            "WGPU_BACKENDS",
            "NEOMACS_GUI_TEST_BACKEND",
        ] {
            let prefix = format!("{name}=");
            let actual = bytes
                .split(|byte| *byte == 0)
                .find_map(|entry| entry.strip_prefix(prefix.as_bytes()))
                .map(|value| String::from_utf8(value.to_vec()).unwrap());
            let expected = if matches!(
                name,
                "DISPLAY" | "XAUTHORITY" | "WAYLAND_DISPLAY" | "WAYLAND_SOCKET"
            ) {
                self.display_environment
                    .iter()
                    .rev()
                    .find(|(key, _)| key == name)
                    .map(|(_, value)| value.clone())
            } else {
                None
            };
            assert_eq!(actual, expected, "owned daemon environment {name}");
            receipt.insert(
                name.to_owned(),
                actual
                    .map(serde_json::Value::String)
                    .unwrap_or(serde_json::Value::Null),
            );
        }
        fs::write(
            self.artifacts.join("daemon-display-environment.json"),
            serde_json::to_vec_pretty(&receipt).unwrap(),
        )
        .unwrap();
    }

    fn create_direct_first_frame(&mut self) {
        self.assert_boot_identity();
        assert_eq!(
            self.evaluate(
                "(progn (setq neomacsclient-test-frame \
              (make-frame '((window-system . neo)))) t)"
            ),
            "t"
        );
        self.wait_for("visible direct GUI frame", |fixture| {
            fixture.evaluate(FIND_VISIBLE_GUI) == "t"
        });
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

    fn open_frame_on_display(&self, display: &str) -> Client {
        let mut command = self.client_command();
        command.args([
            "-a",
            "",
            "--create-frame",
            "--no-wait",
            "--display",
            display,
        ]);
        Client::spawn(command, &self.artifacts.join("explicit-display.stderr"))
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
    fixture.assert_daemon_vulkan_environment();
    fixture.assert_boot_identity();
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
    let display_name = display
        .env()
        .iter()
        .find(|(name, _)| name == "DISPLAY")
        .expect("owned X11 display")
        .1
        .as_str();
    fixture.assert_graphical_identity(display_name, Some(display_name), "normal-client");
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

#[test]
fn daemon_display_environment_forwards_paired_icds_without_replacing_canonical_locale() {
    let architecture = match std::env::consts::ARCH {
        "x86" => "i686",
        architecture => architecture,
    };
    let host = format!("/mesa/lvp_icd.{architecture}.json");
    let environment = CanonicalEnvironment::new(
        CanonicalProfile::Tui,
        "/owned-test/home",
        "/owned-test/tmp",
        "/owned-test/workspace",
    )
    .unwrap();
    let mut command = environment.command("not-executed");
    let display = vec![
        ("DISPLAY".to_owned(), "127.0.0.1:123".to_owned()),
        ("XAUTHORITY".to_owned(), "/owned-test/Xauthority".to_owned()),
        ("VK_DRIVER_FILES".to_owned(), host.clone()),
        ("VK_ICD_FILENAMES".to_owned(), host.clone()),
        ("LC_ALL".to_owned(), "incorrect-display-locale".to_owned()),
    ];
    let locale = command
        .get_env("LC_ALL")
        .map(|value| value.map(OsString::from));
    apply_display_environment(&mut command, &display);
    for key in [
        "DISPLAY",
        "XAUTHORITY",
        "VK_DRIVER_FILES",
        "VK_ICD_FILENAMES",
    ] {
        let expected = display.iter().find(|(name, _)| name == key).unwrap();
        assert_eq!(
            command.get_env(key),
            Some(Some(std::ffi::OsStr::new(&expected.1)))
        );
    }
    assert_eq!(
        command
            .get_env("LC_ALL")
            .map(|value| value.map(OsString::from)),
        locale
    );

    let caller = [
        Some(OsString::from(format!("/mesa/intel_icd.json:{host}"))),
        None,
    ];
    let (selected, receipt) = expected_vulkan_environment(&display, caller.clone(), false);
    assert_eq!(selected[0], Some(OsString::from(&host)));
    assert_eq!(selected[0], selected[1]);
    assert_eq!(receipt["caller_list_was_mixed"], true);
    assert_eq!(receipt["caller_list_was_narrowed"], true);
    assert!(
        std::panic::catch_unwind(|| expected_vulkan_environment(&[], caller.clone(), false))
            .is_err(),
        "deleted session wiring must fail even though the daemon could inherit caller ICDs"
    );
    assert_eq!(
        expected_vulkan_environment(&[], caller.clone(), true).0,
        caller
    );
    let no_lavapipe = [
        Some(OsString::from("/mesa/intel_icd.json")),
        Some(OsString::new()),
    ];
    assert_eq!(
        expected_vulkan_environment(&[], no_lavapipe.clone(), false).0,
        no_lavapipe
    );
}

fn identity_artifacts(label: &str) -> PathBuf {
    let artifacts = neomacs_infra::workspace_root().join(format!(
        "target/neomacs-gui-tests/daemon-identity-{label}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    fs::create_dir_all(&artifacts).unwrap();
    artifacts
}

fn owned_display_value(session: &DisplaySession, key: &str) -> String {
    session
        .env()
        .iter()
        .find(|(name, value)| name == key && !value.is_empty())
        .unwrap_or_else(|| panic!("owned session has no {key}"))
        .1
        .clone()
}

fn named_wayland_environment(session: &DisplaySession) -> (Vec<(String, String)>, String) {
    let name = PathBuf::from(owned_display_value(session, "WAYLAND_DISPLAY"));
    let socket = if name.is_absolute() {
        name
    } else {
        PathBuf::from(owned_display_value(session, "XDG_RUNTIME_DIR")).join(name)
    };
    // The daemon's canonical XDG_RUNTIME_DIR belongs to its server socket, not
    // the compositor. An absolute address preserves both owned namespaces.
    let name = socket
        .to_str()
        .expect("owned Wayland socket path")
        .to_owned();
    let mut environment = session.env().to_vec();
    environment.retain(|(key, _)| {
        !matches!(
            key.as_str(),
            "DISPLAY" | "XAUTHORITY" | "WAYLAND_DISPLAY" | "WAYLAND_SOCKET"
        )
    });
    environment.push(("WAYLAND_DISPLAY".to_owned(), name.clone()));
    (environment, name)
}

#[test]
// Prerequisites: fresh-built editor/client, Xvfb, xauth, xdotool and Vulkan.
fn first_explicit_client_display_matches_the_daemon_frame_and_terminal() {
    let artifacts = identity_artifacts("explicit-client");
    let isolation = neomacs_test_env::bounded_temp_dir("gui-explicit-client-").unwrap();
    let display = DisplayHarness::for_backend(GuiBackend::LinuxX11)
        .start_session(&artifacts)
        .expect("start owned X11 display");
    let name = owned_display_value(&display, "DISPLAY");
    let mut fixture = Fixture::new(&display, isolation.path(), artifacts);
    fixture.wait_for_socket();
    fixture.assert_boot_identity();
    let mut client = fixture.open_frame_on_display(&name);
    let status = client
        .child
        .wait("explicit-display GUI client", &fixture.artifacts);
    assert!(status.success(), "explicit-display client failed: {status}");
    fixture.wait_for_gui();
    fixture.assert_graphical_identity(&name, Some(&name), "explicit-client");
    fixture.assert_invalid_display_refused();
}

#[test]
// Prerequisites: fresh-built editor/client, Xvfb, xauth and Vulkan.
fn daemon_direct_nil_display_first_frame_keeps_x_display_name_nil() {
    let artifacts = identity_artifacts("direct-x11");
    let isolation = neomacs_test_env::bounded_temp_dir("gui-direct-x11-").unwrap();
    let display = DisplayHarness::for_backend(GuiBackend::LinuxX11)
        .start_session(&artifacts)
        .expect("start owned X11 display");
    let name = owned_display_value(&display, "DISPLAY");
    let mut fixture = Fixture::new(&display, isolation.path(), artifacts);
    fixture.wait_for_socket();
    fixture.create_direct_first_frame();
    // GNU frame.el sets x-display-name back to the nil DISPLAY argument after
    // first-frame initialization; the new frame still names its connection.
    fixture.assert_graphical_identity(&name, None, "direct-x11");
}

#[test]
// Prerequisites: fresh-built editor/client, owned Xvfb and Weston, and Vulkan.
fn daemon_direct_nil_display_prefers_wayland_when_both_owned_displays_exist() {
    let artifacts = identity_artifacts("direct-both");
    let isolation = neomacs_test_env::bounded_temp_dir("gui-direct-both-").unwrap();
    let x11 = DisplayHarness::for_backend(GuiBackend::LinuxX11)
        .start_session(artifacts.join("x11"))
        .expect("start owned X11 display");
    let wayland = DisplayHarness::for_backend(GuiBackend::LinuxWayland)
        .start_session(artifacts.join("wayland"))
        .expect("start owned Wayland display");
    let (mut environment, name) = named_wayland_environment(&wayland);
    for key in ["DISPLAY", "XAUTHORITY"] {
        environment.push((key.to_owned(), owned_display_value(&x11, key)));
    }
    let mut fixture = Fixture::with_display_environment(
        &wayland,
        isolation.path(),
        artifacts,
        environment,
        DisplaySelection::FromEnvironment,
        None,
    );
    fixture.wait_for_socket();
    fixture.assert_unforced_daemon_environment();
    fixture.create_direct_first_frame();
    fixture.assert_graphical_identity(&name, None, "direct-both");
    assert_ne!(name, owned_display_value(&x11, "DISPLAY"));
}

#[test]
// Prerequisites: fresh-built editor/client, an owned Weston and Vulkan.
fn daemon_direct_nil_display_recomputes_the_environment_after_a_refused_name() {
    let artifacts = identity_artifacts("direct-retry");
    let isolation = neomacs_test_env::bounded_temp_dir("gui-direct-retry-").unwrap();
    let display = DisplayHarness::for_backend(GuiBackend::LinuxWayland)
        .start_session(&artifacts)
        .expect("start owned Wayland display");
    let (environment, name) = named_wayland_environment(&display);
    let mut fixture = Fixture::with_display_environment(
        &display,
        isolation.path(),
        artifacts,
        environment,
        DisplaySelection::FromEnvironment,
        None,
    );
    fixture.wait_for_socket();
    fixture.assert_boot_identity();
    fixture.assert_eval_data(
        r#"(let ((process-environment (copy-sequence process-environment)))
              (setenv "WAYLAND_DISPLAY" "neomacs-rejected-retry-display")
              (setenv "DISPLAY" nil)
              (condition-case err
                  (progn (make-frame '((window-system . neo))) 'unexpected-frame)
                (error (list err neomacs-initialized
                             (get 'neo 'window-system-initialized)))))"#,
        "((error \"Display neomacs-rejected-retry-display can’t be opened\") nil nil)",
        "direct-retry-refused-name-error-and-initialization",
    );
    assert_eq!(
        fixture.evaluate("(list x-display-name neomacs-initialized)"),
        "(\"neomacs-rejected-retry-display\" nil)",
        "the refused attempt leaves a name that must not shadow the restored environment"
    );
    assert_eq!(
        fixture.evaluate(
            "(progn (setq neomacsclient-test-frame \
              (make-frame '((window-system . neo)))) t)"
        ),
        "t"
    );
    fixture.wait_for("visible direct GUI frame after refusal", |fixture| {
        fixture.evaluate(FIND_VISIBLE_GUI) == "t"
    });
    fixture.assert_graphical_identity(&name, None, "direct-retry");
    fixture.assert_eval_data(
        "(list neomacs-initialized (get 'neo 'window-system-initialized))",
        "(t t)",
        "direct-retry-successful-initialization",
    );
}

#[test]
// Prerequisites: fresh-built editor/client, Xvfb, xauth and Vulkan.
fn daemon_direct_nil_display_ignores_an_empty_wayland_name_before_owned_x11() {
    let artifacts = identity_artifacts("direct-empty-wayland");
    let isolation = neomacs_test_env::bounded_temp_dir("gui-direct-empty-wayland-").unwrap();
    let display = DisplayHarness::for_backend(GuiBackend::LinuxX11)
        .start_session(&artifacts)
        .expect("start owned X11 display");
    let name = owned_display_value(&display, "DISPLAY");
    let mut environment = display.env().to_vec();
    // Empty WAYLAND_DISPLAY is intentional; no inherited WAYLAND_SOCKET may
    // turn this into an anonymous native connection instead of X11 fallback.
    environment.retain(|(key, _)| key != "WAYLAND_DISPLAY" && key != "WAYLAND_SOCKET");
    environment.push(("WAYLAND_DISPLAY".to_owned(), String::new()));
    let mut fixture = Fixture::with_display_environment(
        &display,
        isolation.path(),
        artifacts,
        environment,
        DisplaySelection::FromEnvironment,
        None,
    );
    fixture.wait_for_socket();
    fixture.assert_unforced_daemon_environment();
    fixture.create_direct_first_frame();
    fixture.assert_graphical_identity(&name, None, "direct-empty-wayland");
}

#[test]
// Prerequisites: fresh-built editor/client, an owned Weston and Vulkan.
fn anonymous_daemon_direct_frame_refuses_a_missing_name_with_a_live_wayland_socket() {
    let artifacts = identity_artifacts("direct-anonymous");
    let isolation = neomacs_test_env::bounded_temp_dir("gui-direct-anonymous-").unwrap();
    let display = DisplayHarness::for_backend(GuiBackend::LinuxWayland)
        .start_session(&artifacts)
        .expect("start owned Wayland display");
    let (mut environment, address) = named_wayland_environment(&display);
    let socket = UnixStream::connect(&address).expect("connect a real owned Wayland socket");
    let fd = socket.as_raw_fd();
    let endpoint = fs::read_link(format!("/proc/self/fd/{fd}")).unwrap();
    assert!(endpoint.to_string_lossy().starts_with("socket:["));
    environment.retain(|(key, _)| key != "WAYLAND_DISPLAY");
    environment.push(("WAYLAND_DISPLAY".to_owned(), String::new()));
    environment.push(("WAYLAND_SOCKET".to_owned(), fd.to_string()));
    let mut fixture = Fixture::with_display_environment(
        &display,
        isolation.path(),
        artifacts,
        environment,
        DisplaySelection::FromEnvironment,
        Some(socket.as_fd()),
    );
    fixture.wait_for_socket();
    fixture.assert_unforced_daemon_environment();
    fixture.assert_boot_identity();
    let inherited_endpoint = fs::read_dir(format!("/proc/{}/fd", fixture.daemon.0.id()))
        .unwrap()
        .filter_map(Result::ok)
        .filter_map(|entry| fs::read_link(entry.path()).ok())
        .any(|link| link == endpoint);
    assert!(
        inherited_endpoint,
        "daemon did not retain the real inherited socket"
    );
    let socket_hygiene = fixture.evaluate(
        r#"(let ((socket-entry-present nil))
          (dolist (entry process-environment)
            (when (or (equal entry "WAYLAND_SOCKET")
                      (string-prefix-p "WAYLAND_SOCKET=" entry))
              (setq socket-entry-present t)))
          (list (getenv "WAYLAND_SOCKET") socket-entry-present x-display-name))"#,
    );
    fs::write(
        fixture.artifacts.join("anonymous-socket-hygiene.el"),
        &socket_hygiene,
    )
    .unwrap();
    assert_eq!(
        socket_hygiene, "(nil nil nil)",
        "a consumed transport descriptor must not leak into Lisp environment or display names"
    );
    assert_eq!(
        fixture.evaluate("(list (getenv \"DISPLAY\") (getenv \"WAYLAND_DISPLAY\"))"),
        "(nil \"\")"
    );
    let before = fixture.evaluate("(length (frame-list))");
    let observed = fixture.evaluate(
        r#"(condition-case err
        (progn (make-frame '((window-system . neo))) 'unexpected-frame)
      (error (error-message-string err)))"#,
    );
    fs::write(
        fixture.artifacts.join("anonymous-direct-refusal.el"),
        &observed,
    )
    .unwrap();
    assert_eq!(
        observed, "\"Please specify display\"",
        "an unnamed inherited connection must not manufacture a display alias"
    );
    assert_eq!(fixture.evaluate("(length (frame-list))"), before);
    fixture.assert_boot_identity();
    // Only an explicit caller argument may name the anonymous connection. A
    // successful real frame proves that the earlier refusal was about a name,
    // rather than a daemon that never registered its supplied native host.
    assert_eq!(
        fixture.evaluate(
            r#"(progn
          (setq neomacsclient-test-frame
                (make-frame-on-display "wayland" '((window-system . neo))))
          t)"#,
        ),
        "t"
    );
    fixture.wait_for(
        "anonymous host explicit-display positive control",
        |fixture| fixture.evaluate(FIND_VISIBLE_GUI) == "t",
    );
    fixture.assert_graphical_identity("wayland", Some("wayland"), "anonymous-explicit-control");
    fs::write(
        fixture.artifacts.join("inherited-wayland-socket.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "parent_endpoint": endpoint.to_string_lossy(),
            "same_socket_present_in_daemon": inherited_endpoint,
            "Lisp_environment_hygiene": socket_hygiene,
            "native_host_registration_proven": true,
            "proof": "Explicit literal wayland created a visible real GUI frame with matching terminal and frame identity",
        })).unwrap(),
    ).unwrap();
}

fn assert_invalid_wayland_descriptor_is_not_a_native_host(
    display: Option<&DisplaySession>,
    artifacts: PathBuf,
    wayland_name: Option<String>,
) {
    let isolation = neomacs_test_env::bounded_temp_dir("gui-invalid-wayland-fd-").unwrap();
    let no_display = DisplaySession::current_desktop();
    let session = display.unwrap_or(&no_display);
    let path = artifacts.join("regular-file-not-wayland");
    fs::write(&path, b"a regular file is not a Wayland transport\n").unwrap();
    let descriptor = File::open(&path).unwrap();
    let fd = descriptor.as_raw_fd();
    let original = descriptor.metadata().unwrap();
    assert!(original.is_file());
    // Only an explicitly owned display may contribute graphics variables.
    // The no-display case does not consult the placeholder session environment.
    let mut environment = display
        .map(|owned| owned.env().to_vec())
        .unwrap_or_default();
    environment.retain(|(key, _)| key != "WAYLAND_DISPLAY" && key != "WAYLAND_SOCKET");
    environment.push((
        "WAYLAND_DISPLAY".to_owned(),
        wayland_name.clone().unwrap_or_default(),
    ));
    environment.push(("WAYLAND_SOCKET".to_owned(), fd.to_string()));
    let mut fixture = Fixture::with_display_environment(
        session,
        isolation.path(),
        artifacts,
        environment,
        DisplaySelection::FromEnvironment,
        Some(descriptor.as_fd()),
    );
    fixture.wait_for_socket();
    fixture.assert_unforced_daemon_environment();
    fixture.assert_boot_identity();
    let child_fd = PathBuf::from(format!("/proc/{}/fd/{fd}", fixture.daemon.0.id()));
    let inherited = fs::metadata(&child_fd).expect("invalid descriptor must remain open in daemon");
    assert!(inherited.is_file());
    assert_eq!(
        (inherited.dev(), inherited.ino()),
        (original.dev(), original.ino()),
        "producer precheck must not consume or replace an invalid descriptor"
    );
    let observed = fixture.evaluate(
        r#"(let (socket-entries)
        (dolist (entry process-environment)
          (when (or (equal entry "WAYLAND_SOCKET")
                    (string-prefix-p "WAYLAND_SOCKET=" entry))
            (push entry socket-entries)))
        (list (getenv "WAYLAND_SOCKET") socket-entries x-display-name))"#,
    );
    let value = serde_json::to_string(&fd.to_string()).unwrap();
    let entry = serde_json::to_string(&format!("WAYLAND_SOCKET={fd}")).unwrap();
    assert_eq!(
        observed,
        format!("({value} ({entry}) nil)"),
        "an invalid descriptor must remain in the Lisp environment for diagnosis"
    );
    fs::write(
        fixture.artifacts.join("invalid-socket-environment.el"),
        &observed,
    )
    .unwrap();
    assert_eq!(
        fixture.evaluate(
            "(list (length (frame-list)) \
               (mapcar #'terminal-live-p (terminal-list)) \
               (mapcar (lambda (frame) (and (display-graphic-p frame) t)) (frame-list)))"
        ),
        "(1 (t) (nil))",
        "invalid transport must not admit a graphical terminal or frame"
    );
    fixture.wait_for("invalid WAYLAND_SOCKET warning", |fixture| {
        ["daemon.stderr", "daemon.log"].iter().any(|name| {
            let text = fs::read_to_string(fixture.artifacts.join(name)).unwrap_or_default();
            text.contains("daemon continuing without graphical hosting")
                && text.contains("WAYLAND_SOCKET")
        })
    });
    let before = fixture.evaluate("(length (frame-list))");
    let requested = wayland_name
        .or_else(|| display.map(|session| owned_display_value(session, "DISPLAY")))
        .unwrap_or_else(|| "wayland".to_owned());
    let form = format!(
        "(condition-case err \
           (progn (make-frame-on-display {} '((window-system . neo))) 'unexpected-frame) \
           (error (setq neomacsclient-test-refusal-error err) (car err)))",
        serde_json::to_string(&requested).unwrap(),
    );
    fixture.assert_eval_data(
        &form,
        "cl-no-applicable-method",
        "invalid-explicit-display-refusal-class",
    );
    fixture.evaluate("neomacsclient-test-refusal-error");
    let refusal_error = fs::read(fixture.artifacts.join("eval.stdout")).unwrap();
    fs::write(
        fixture
            .artifacts
            .join("invalid-explicit-display-refusal-error.el"),
        &refusal_error,
    )
    .unwrap();
    assert_eq!(fixture.evaluate("(length (frame-list))"), before);
    fixture.assert_eval_data(
        "(list x-display-name (get 'neo 'window-system-initialized) \
           initial-window-system (window-system) (terminal-name) \
           (mapcar #'terminal-live-p (terminal-list)) \
           (mapcar (lambda (frame) (and (display-graphic-p frame) t)) (frame-list)))",
        &format!(
            "({} t nil nil \"initial_terminal\" (t) (nil))",
            serde_json::to_string(&requested).unwrap(),
        ),
        "invalid-post-refusal-terminal-and-frame-identity",
    );
    fixture.evaluate("x-display-name");
    let post_x_display_name = fs::read_to_string(fixture.artifacts.join("eval.stdout")).unwrap();
    fs::write(
        fixture
            .artifacts
            .join("invalid-post-refusal-x-display-name.el"),
        &post_x_display_name,
    )
    .unwrap();
    let post_environment = fixture.assert_eval_data(
        r#"(let (socket-entries)
        (dolist (entry process-environment)
          (when (or (equal entry "WAYLAND_SOCKET")
                    (string-prefix-p "WAYLAND_SOCKET=" entry))
            (push entry socket-entries)))
        (list (getenv "WAYLAND_SOCKET") socket-entries))"#,
        &format!("({value} ({entry}))"),
        "invalid-post-refusal-socket-environment",
    );
    let after = fs::metadata(&child_fd).expect("invalid descriptor must still be owned by daemon");
    assert_eq!((after.dev(), after.ino()), (original.dev(), original.ino()));
    fs::write(
        fixture.artifacts.join("invalid-wayland-descriptor.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "numeric_fd": fd,
            "regular_file": path,
            "same_regular_file_inode_retained": true,
            "device": after.dev(),
            "inode": after.ino(),
            "Lisp_environment": observed,
            "post_refusal_Lisp_environment": post_environment,
            "post_refusal_x_display_name_raw": post_x_display_name,
            "explicit_display_refused": requested,
            "refusal_error_class": "cl-no-applicable-method",
            "full_refusal_error_raw_artifact": "invalid-explicit-display-refusal-error.el",
            "graphical_fallback_admitted": false,
            "required_warning_fragments_in_one_file": [
                "daemon continuing without graphical hosting", "WAYLAND_SOCKET"
            ],
        }))
        .unwrap(),
    )
    .unwrap();
}

#[test]
// A real daemon inherits a real regular-file descriptor, with no display name.
fn daemon_preserves_an_invalid_wayland_descriptor_without_admitting_a_native_host() {
    assert_invalid_wayland_descriptor_is_not_a_native_host(
        None,
        identity_artifacts("invalid-wayland-fd-no-display"),
        None,
    );
}

#[test]
// Prerequisites: fresh-built editor/client, Xvfb and xauth. The usable X server
// must not conceal an invalid supplied Wayland transport through fallback.
fn daemon_invalid_wayland_descriptor_does_not_fall_back_to_owned_x11() {
    let artifacts = identity_artifacts("invalid-wayland-fd-with-x11");
    let display = DisplayHarness::for_backend(GuiBackend::LinuxX11)
        .start_session(artifacts.join("x11"))
        .expect("start owned X11 display");
    assert_invalid_wayland_descriptor_is_not_a_native_host(Some(&display), artifacts, None);
}

#[test]
// A Wayland name must not bypass validation of the transport the connection
// consumer reads first, or it can consume an unrelated inherited descriptor.
fn daemon_invalid_wayland_descriptor_does_not_use_an_owned_wayland_name() {
    let artifacts = identity_artifacts("invalid-wayland-fd-with-wayland");
    let display = DisplayHarness::for_backend(GuiBackend::LinuxWayland)
        .start_session(artifacts.join("wayland"))
        .expect("start owned Wayland display");
    let (_, name) = named_wayland_environment(&display);
    assert_invalid_wayland_descriptor_is_not_a_native_host(Some(&display), artifacts, Some(name));
}
