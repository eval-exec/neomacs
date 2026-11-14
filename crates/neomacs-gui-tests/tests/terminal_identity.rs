//! Real GUI startup identities come from the owned native connection, not the cached image.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{DisplayHarness, GuiBackend};
use std::fs::{self, File};
use std::os::fd::AsRawFd;
use std::os::unix::{net::UnixStream, process::CommandExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const INITIAL_IDENTITY_FORM: &str = r#"
(let ((initial frame-initial-frame))
  (unless (and initial (frame-live-p initial))
    (error "The initial graphical frame is not live"))
  (with-temp-file (getenv "NEOMACS_TEST_IDENTITY_RESULT")
    (prin1
     (list :initial-live (and (frame-live-p initial) t)
           :initial-graphical (and (display-graphic-p initial) t)
           :initial-placeholder (and (frame-initial-p initial) t)
           :x-display-name x-display-name
           :terminal-name (terminal-name (frame-terminal initial))
           :initial-display (frame-parameter initial 'display))
     (current-buffer))))
"#;

struct OwnedChild(Child);

impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct IdentityRun {
    artifacts: PathBuf,
    status: ExitStatus,
    expected_identity: String,
}

fn workspace_root() -> PathBuf {
    neomacs_infra::crate_root!().join("../..")
}

fn neomacs_binary() -> PathBuf {
    std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root().join("target/release/neomacs"))
}

fn owned_artifacts(scenario: &str) -> PathBuf {
    let root = workspace_root().join("target/neomacs-gui-tests");
    fs::create_dir_all(&root).unwrap();
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("artifact timestamp")
        .as_nanos();
    let artifacts = root.join(format!(
        "terminal-identity-{}-{scenario}-{unique}",
        std::process::id()
    ));
    fs::create_dir(&artifacts).unwrap();
    artifacts
}

fn remove_backend_forces(command: &mut Command) {
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

fn gui_command(binary: &Path, artifacts: &Path, form: &str) -> Command {
    let mut command = Command::new(binary);
    command
        .args(["-Q", "--eval", form])
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("WAYLAND_SOCKET")
        .env_remove("XAUTHORITY")
        .env(
            "NEOMACS_TEST_IDENTITY_RESULT",
            artifacts.join("identity.el"),
        )
        .env("RUST_LOG", "warn")
        .stdin(Stdio::null())
        .stdout(File::create(artifacts.join("stdout.log")).unwrap())
        .stderr(File::create(artifacts.join("stderr.log")).unwrap());
    remove_backend_forces(&mut command);
    command
}

fn apply_session_environment(command: &mut Command, environment: &[(String, String)]) {
    for (name, value) in environment {
        if matches!(
            name.as_str(),
            "WINIT_UNIX_BACKEND"
                | "GDK_BACKEND"
                | "WGPU_BACKEND"
                | "WGPU_BACKENDS"
                | "NEOMACS_GUI_TEST_BACKEND"
        ) {
            continue;
        }
        if value.is_empty() {
            command.env_remove(name);
        } else {
            command.env(name, value);
        }
    }
    remove_backend_forces(command);
}

fn session_value<'a>(environment: &'a [(String, String)], name: &str) -> &'a str {
    environment
        .iter()
        .find(|(key, value)| key == name && !value.is_empty())
        .map(|(_, value)| value.as_str())
        .unwrap_or_else(|| panic!("owned display session has no {name}"))
}

fn spawn_gui(command: &mut Command, artifacts: &Path) -> OwnedChild {
    fs::write(artifacts.join("command.txt"), format!("{command:?}\n")).unwrap();
    OwnedChild(command.spawn().expect("start real Neomacs GUI"))
}

fn wait_for_gui(child: &mut OwnedChild, artifacts: &Path) -> ExitStatus {
    let deadline = Instant::now() + Duration::from_secs(25);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            return status;
        }
        assert!(
            Instant::now() < deadline,
            "GUI probe timed out; artifacts: {}",
            artifacts.display()
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn assert_initial_gui_identity(artifacts: &Path, expected_identity: &str, status: ExitStatus) {
    let observed = fs::read_to_string(artifacts.join("identity.el")).unwrap_or_default();
    let expected_name = serde_json::to_string(expected_identity).unwrap();
    assert_eq!(
        observed,
        format!(
            "(:initial-live t :initial-graphical t :initial-placeholder nil :x-display-name {expected_name} :terminal-name {expected_name} :initial-display {expected_name})"
        ),
        "status={status}; artifacts={}; stderr=\n{}",
        artifacts.display(),
        fs::read_to_string(artifacts.join("stderr.log")).unwrap_or_default()
    );
    assert!(status.success());
}

#[test]
// Prerequisites: Weston and a built binary/pdump; the test owns its compositor.
fn inherited_wayland_connection_is_not_the_bootstrap_terminal() {
    assert_inherited_wayland_identity(None);
}

#[test]
// Prerequisites: Weston and a built binary/pdump; the test owns its compositor.
fn inherited_wayland_connection_does_not_adopt_an_unused_socket_name() {
    assert_inherited_wayland_identity(Some("wayland-unused-issue-364"));
}

fn assert_inherited_wayland_identity(unused_display: Option<&str>) {
    let artifacts = owned_artifacts(if unused_display.is_some() {
        "unused-name"
    } else {
        "unnamed"
    });
    let session = DisplayHarness::for_backend(GuiBackend::LinuxWayland)
        .start_session(&artifacts)
        .expect("start owned Wayland compositor");
    let session_env = |key: &str| session_value(session.env(), key);
    let display = PathBuf::from(session_env("WAYLAND_DISPLAY"));
    let socket_path = if display.is_absolute() {
        display
    } else {
        PathBuf::from(session_env("XDG_RUNTIME_DIR")).join(display)
    };
    let socket = UnixStream::connect(socket_path).expect("connect to the real Wayland compositor");
    let fd = socket.as_raw_fd();
    let result = artifacts.join("result.el");
    let form = format!(
        r#"
(progn
  (split-window-right)
  (let* ((name (terminal-name (selected-frame)))
         (eligible (and (stringp name) (> (length name) 0)
                        (not (equal name "initial_terminal"))))
         (result (list :graphical (and (display-graphic-p) t)
                       :initial (frame-initial-p)
                       :eligible eligible :windows (length (window-list))
                       :name name :display (frame-parameter nil 'display))))
    (with-temp-file (getenv "NEOMACS_TEST_RESULT")
      (prin1 result (current-buffer)))
    {INITIAL_IDENTITY_FORM}
    (kill-emacs (if eligible 0 2))))
"#
    );
    let mut command = gui_command(&neomacs_binary(), &artifacts, &form);
    command
        .env("XDG_RUNTIME_DIR", session_env("XDG_RUNTIME_DIR"))
        .env("WAYLAND_SOCKET", fd.to_string())
        .env("NEOMACS_TEST_RESULT", &result);
    if let Some(display) = unused_display {
        command.env("WAYLAND_DISPLAY", display);
    }
    // SAFETY: only async-signal-safe fcntl runs between fork and exec. The
    // socket stays owned by this test until spawn finishes; no other FD changes.
    unsafe {
        command.pre_exec(move || {
            if libc::fcntl(fd, libc::F_SETFD, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = spawn_gui(&mut command, &artifacts);
    drop(socket);
    let status = wait_for_gui(&mut child, &artifacts);
    let observed = fs::read_to_string(&result).unwrap_or_default();
    assert_eq!(
        observed,
        "(:graphical t :initial nil :eligible t :windows 2 :name \"wayland\" :display \"wayland\")",
        "status={status}; logs={}\n{}",
        artifacts.join("stderr.log").display(),
        fs::read_to_string(artifacts.join("stderr.log")).unwrap_or_default()
    );
    assert!(status.success());
    assert_initial_gui_identity(&artifacts, "wayland", status);
}

fn run_named_identity(backend: GuiBackend, scenario: &str, image: Option<&Path>) -> IdentityRun {
    let display_variable = match backend {
        GuiBackend::LinuxX11 => "DISPLAY",
        GuiBackend::LinuxWayland => "WAYLAND_DISPLAY",
        other => panic!("unsupported named identity backend: {other:?}"),
    };
    let artifacts = owned_artifacts(scenario);
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifacts)
        .expect("start owned display");
    let expected_identity = session_value(session.env(), display_variable).to_owned();
    let form = format!("(progn {INITIAL_IDENTITY_FORM} (kill-emacs 0))");
    let mut command = gui_command(&neomacs_binary(), &artifacts, &form);
    if let Some(image) = image {
        command.arg("--dump-file").arg(image);
        fs::write(artifacts.join("dump-file.txt"), image.display().to_string()).unwrap();
    }
    apply_session_environment(&mut command, session.env());
    let mut child = spawn_gui(&mut command, &artifacts);
    let status = wait_for_gui(&mut child, &artifacts);
    IdentityRun {
        artifacts,
        status,
        expected_identity,
    }
}

fn assert_named_identity(run: &IdentityRun) {
    assert_initial_gui_identity(&run.artifacts, &run.expected_identity, run.status);
}

#[test]
fn named_x11_default_display_identity_is_consistent() {
    assert_named_identity(&run_named_identity(GuiBackend::LinuxX11, "named-x11", None));
}

#[test]
fn named_wayland_default_display_identity_is_consistent() {
    assert_named_identity(&run_named_identity(
        GuiBackend::LinuxWayland,
        "named-wayland",
        None,
    ));
}

#[test]
fn both_owned_display_environments_use_wayland_identity() {
    assert_both_display_identity(None);
}

#[test]
fn command_line_display_updates_the_seeded_native_startup_environment() {
    assert_both_display_identity(Some("--display"));
}

fn assert_both_display_identity(display_switch: Option<&str>) {
    let artifacts = owned_artifacts("both-environments");
    let x11 = DisplayHarness::for_backend(GuiBackend::LinuxX11)
        .start_session(&artifacts.join("x11"))
        .expect("start owned X11 display");
    let wayland = DisplayHarness::for_backend(GuiBackend::LinuxWayland)
        .start_session(&artifacts.join("wayland"))
        .expect("start owned Wayland display");
    let x11_display = session_value(x11.env(), "DISPLAY");
    let wayland_display = session_value(wayland.env(), "WAYLAND_DISPLAY");
    assert_ne!(x11_display, wayland_display);
    let command_line_check = if display_switch.is_some() {
        r#"(unless (equal (getenv "DISPLAY")
                         (terminal-name (frame-terminal frame-initial-frame)))
             (error "The explicit display did not update the startup environment"))"#
    } else {
        ""
    };
    let form = format!("(progn {command_line_check} {INITIAL_IDENTITY_FORM} (kill-emacs 0))");
    let mut command = gui_command(&neomacs_binary(), &artifacts, &form);
    if let Some(switch) = display_switch {
        command.arg(switch).arg(wayland_display);
    }
    // Keep Wayland's owned software-driver/locale environment, then copy only
    // X11's actual address/authentication. Xvfb's empty Wayland entries and
    // GDK_BACKEND=x11 must not overwrite the real Wayland connection.
    apply_session_environment(&mut command, wayland.env());
    command.env("DISPLAY", x11_display);
    if let Some((_, value)) = x11.env().iter().find(|(name, _)| name == "XAUTHORITY") {
        if value.is_empty() {
            command.env_remove("XAUTHORITY");
        } else {
            command.env("XAUTHORITY", value);
        }
    }
    command
        .env(
            "XDG_RUNTIME_DIR",
            session_value(wayland.env(), "XDG_RUNTIME_DIR"),
        )
        .env("WAYLAND_DISPLAY", wayland_display)
        .env_remove("WAYLAND_SOCKET");
    remove_backend_forces(&mut command);
    let mut child = spawn_gui(&mut command, &artifacts);
    let status = wait_for_gui(&mut child, &artifacts);
    assert_initial_gui_identity(&artifacts, wayland_display, status);
}

#[test]
fn cached_image_uses_each_actual_display_identity_without_image_mutation() {
    let image = neomacs_binary().with_extension("pdump");
    assert!(
        image.is_file(),
        "cached image is absent: {}",
        image.display()
    );
    let before = fs::read(&image).expect("read the one cached image before both starts");
    let x11 = run_named_identity(GuiBackend::LinuxX11, "cached-image-x11", Some(&image));
    assert!(
        fs::read(&image).expect("read cached image between starts") == before,
        "cached image bytes changed during the X11 start: {}",
        image.display()
    );
    assert_named_identity(&x11);
    let wayland = run_named_identity(
        GuiBackend::LinuxWayland,
        "cached-image-wayland",
        Some(&image),
    );
    assert!(
        fs::read(&image).expect("read cached image after both starts") == before,
        "cached image bytes changed during the Wayland start: {}",
        image.display()
    );
    assert_ne!(x11.expected_identity, wayland.expected_identity);
    assert_named_identity(&wayland);
}
