//! Real GUI startup: an inherited Wayland socket need not have a display name.
#![cfg(target_os = "linux")]

use std::fs::{self, File};
use std::os::fd::AsRawFd;
use std::os::unix::{net::UnixStream, process::CommandExt};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
#[ignore = "requires a running Wayland compositor and a built binary/pdump"]
fn inherited_wayland_connection_is_not_the_bootstrap_terminal() {
    assert_inherited_wayland_identity(None);
}

#[test]
#[ignore = "requires a running Wayland compositor and a built binary/pdump"]
fn inherited_wayland_connection_does_not_adopt_an_unused_socket_name() {
    assert_inherited_wayland_identity(Some("wayland-unused-issue-364"));
}

fn assert_inherited_wayland_identity(unused_display: Option<&str>) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let display = PathBuf::from(std::env::var_os("WAYLAND_DISPLAY").expect("Wayland display"));
    let socket_path = if display.is_absolute() {
        display
    } else {
        PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").expect("runtime directory")).join(display)
    };
    let socket = UnixStream::connect(socket_path).expect("connect to the real Wayland compositor");
    let fd = socket.as_raw_fd();
    let artifacts = root.join(format!(
        "target/neomacs-gui-tests/terminal-identity-{}",
        std::process::id()
    ));
    fs::create_dir_all(&artifacts).unwrap();
    let result = artifacts.join("result.el");
    let stderr = artifacts.join("stderr.log");
    let mut command = Command::new(binary);
    command
        .args([
            "-Q",
            "--eval",
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
    (kill-emacs (if eligible 0 2))))
"#,
        ])
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("DISPLAY")
        .env("WAYLAND_SOCKET", fd.to_string())
        .env("NEOMACS_TEST_RESULT", &result)
        .env("RUST_LOG", "warn")
        .stdin(Stdio::null())
        .stdout(File::create(artifacts.join("stdout.log")).unwrap())
        .stderr(File::create(&stderr).unwrap());
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
    let mut child = command.spawn().expect("start real Neomacs GUI");
    drop(socket);
    let deadline = Instant::now() + Duration::from_secs(25);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("GUI probe timed out; artifacts: {}", artifacts.display());
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let observed = fs::read_to_string(&result).unwrap_or_default();
    assert_eq!(
        observed,
        "(:graphical t :initial nil :eligible t :windows 2 :name \"wayland\" :display \"wayland\")",
        "status={status}; logs={}\n{}",
        stderr.display(),
        fs::read_to_string(&stderr).unwrap_or_default()
    );
    assert!(status.success());
}
