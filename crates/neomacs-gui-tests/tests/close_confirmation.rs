//! #390: native close requests must leave the display alive for Lisp's decision.
//! Uses an isolated headless Sway session: IPC sends xdg_toplevel.close, wtype
//! supplies real keyboard input, and grim captures the rendered confirmation.
//! Prerequisites: fresh-built release runtime, sway, swaymsg, wtype, and grim.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::DisplaySession;
use std::{
    fs,
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

struct Editor(Child);
impl Drop for Editor {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct CloseFixture {
    // Drop the editor before its compositor, including on assertion failure.
    editor: Editor,
    display: DisplaySession,
    artifacts: PathBuf,
    ipc: PathBuf,
}

impl CloseFixture {
    fn new(mode: &str) -> Self {
        let root = neomacs_infra::workspace_root();
        let artifacts = root.join(format!(
            "tmp/neomacs-gui-tests/close-{mode}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&artifacts).unwrap();
        let display = neomacs_infra::display::start_sway(&artifacts, "output * resolution 1000x700\nxwayland disable\nseat seat0 fallback true\ndefault_border none\n").expect("install sway");
        let runtime = display
            .env()
            .iter()
            .find(|(key, _)| key == "XDG_RUNTIME_DIR")
            .unwrap()
            .1
            .clone();
        let ipc = wait("Sway IPC socket", &artifacts, || {
            fs::read_dir(&artifacts)
                .ok()?
                .filter_map(Result::ok)
                .find(|e| e.file_name().to_string_lossy().starts_with("sway-ipc."))
                .map(|e| PathBuf::from(&runtime).join(e.file_name()))
        });
        let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("target/release/neomacs"));
        let mut editor = Editor(
            Command::new(binary)
                .args(["-Q", "-l"])
                .arg(root.join("crates/neomacs-gui-tests/fixtures/close-confirmation.el"))
                .envs(display.env().iter().map(|(k, v)| (k, v)))
                .env_remove("DISPLAY")
                .env("WINIT_UNIX_BACKEND", "wayland")
                .env("WGPU_BACKEND", "gl")
                .env("WAYLAND_DEBUG", "1")
                .env("NEOMACS_GUI_CLOSE_CONTROL", &artifacts)
                .env("NEOMACS_GUI_CLOSE_MODE", mode)
                .env("RUST_LOG", "warn,neomacs_display_runtime=info")
                .env("NEOMACS_LOG_FILE", artifacts.join("neomacs.log"))
                .stdout(fs::File::create(artifacts.join("stdout.log")).unwrap())
                .stderr(fs::File::create(artifacts.join("wayland.log")).unwrap())
                .stdin(Stdio::null())
                .spawn()
                .expect("run cargo xtask fresh-build --release first"),
        );
        wait("fixture readiness", &artifacts, || {
            assert!(
                editor.0.try_wait().unwrap().is_none(),
                "editor exited before readiness; {}",
                artifacts.display()
            );
            artifacts.join("ready").exists().then_some(())
        });
        Self {
            editor,
            display,
            artifacts,
            ipc,
        }
    }

    fn frame_command(&self, title: &str, command: &str) {
        let output = Command::new("swaymsg")
            .arg("-s")
            .arg(&self.ipc)
            .arg(format!(
                "[pid={} title=\"^{title}$\"] {command}",
                self.editor.0.id()
            ))
            .output()
            .expect("install swaymsg");
        let reply: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(
            output.status.success()
                && reply
                    .as_array()
                    .is_some_and(|items| !items.is_empty()
                        && items.iter().all(|item| item["success"] == true)),
            "Sway {command}: {}; artifacts: {}",
            String::from_utf8_lossy(&output.stdout),
            self.artifacts.display()
        );
    }

    fn keys(&self, keys: &[&str]) {
        let output =
            Command::new(std::env::var_os("NEOMACS_GUI_WTYPE").unwrap_or_else(|| "wtype".into()))
                // Activate the newly created virtual keyboard before meaningful keys;
                // its first press can arrive as wl_keyboard.enter's held-key state.
                .args(["-k", "Shift_L", "-s", "50"])
                .args(keys)
                .envs(self.display.env().iter().map(|(k, v)| (k, v)))
                .output()
                .expect("install wtype");
        assert!(
            output.status.success(),
            "wtype: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn popup_count(&self) -> usize {
        fs::read_to_string(self.artifacts.join("wayland.log"))
            .unwrap_or_default()
            .lines()
            .filter(|line| line.contains("xdg_popup") && line.contains(".configure("))
            .count()
    }

    fn wait_popup(&mut self, previous: usize) {
        let artifacts = self.artifacts.clone();
        wait("configured native confirmation popup", &artifacts, || {
            assert!(
                self.editor.0.try_wait().unwrap().is_none(),
                "editor exited before confirmation"
            );
            (self.popup_count() > previous).then_some(())
        });
    }

    fn observe_input(&self) -> serde_json::Value {
        self.keys(&["-M", "ctrl", "-k", "c", "-m", "ctrl", "-k", "t"]);
        wait("editor input after cancellation", &self.artifacts, || {
            serde_json::from_slice(&fs::read(self.artifacts.join("responsive")).ok()?).ok()
        })
    }

    fn capture(&self, name: &str) {
        let output =
            Command::new(std::env::var_os("NEOMACS_GUI_GRIM").unwrap_or_else(|| "grim".into()))
                .arg(self.artifacts.join(name))
                .envs(self.display.env().iter().map(|(k, v)| (k, v)))
                .output()
                .expect("install grim");
        assert!(
            output.status.success(),
            "grim: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn assert_exit(&mut self) {
        let status = wait("editor exit", &self.artifacts, || {
            self.editor.0.try_wait().unwrap()
        });
        assert!(status.success(), "{status}; {}", self.artifacts.display());
    }
}

fn wait<T>(label: &str, artifacts: &std::path::Path, mut observe: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let log = fs::read_to_string(artifacts.join("neomacs.log")).unwrap_or_default();
        assert!(
            !log.contains("failed to show popup menu: sending on a disconnected channel"),
            "#390: GUI disconnected before close confirmation; artifacts: {}",
            artifacts.display()
        );
        assert!(
            !log.contains("native menu presentation failed"),
            "native menu failed; artifacts: {}",
            artifacts.display()
        );
        if let Some(value) = observe() {
            return value;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {label}; artifacts: {}",
            artifacts.display()
        );
        thread::sleep(Duration::from_millis(25));
    }
}

#[test]
fn clean_window_close_exits_successfully() {
    let mut fixture = CloseFixture::new("clean");
    fixture.frame_command("CLOSE-TEST-PRIMARY", "kill");
    fixture.assert_exit();
}

#[test]
fn modified_window_close_keeps_confirmation_and_input_alive() {
    let mut fixture = CloseFixture::new("modified");
    fixture.frame_command("CLOSE-TEST-PRIMARY", "kill");
    fixture.wait_popup(0);
    fixture.capture("save-confirmation.png");
    // GNU's Unsaved Buffers dialog ends with Cancel.
    fixture.keys(&["-k", "End", "-k", "Return"]);
    let state = fixture.observe_input();
    assert_eq!(state["modified"], true);
    assert_eq!(state["text"], "unsaved change");
    fixture.capture("after-cancel.png");
    let previous = fixture.popup_count();
    fixture.frame_command("CLOSE-TEST-PRIMARY", "kill");
    fixture.wait_popup(previous);
    let previous = fixture.popup_count();
    // Close Without Saving, then confirm exit despite modified buffers.
    fixture.keys(&["-k", "Home", "-k", "Return"]);
    fixture.wait_popup(previous);
    fixture.keys(&["-k", "Home", "-k", "Return"]);
    fixture.assert_exit();
    assert!(
        !fixture.artifacts.join("unsaved.txt").exists(),
        "discard must not save the file"
    );
}

#[test]
fn secondary_close_waits_for_lisp_decision() {
    let mut fixture = CloseFixture::new("protected-secondary");
    fixture.frame_command("CLOSE-TEST-SECONDARY", "kill");
    wait("Lisp close handler", &fixture.artifacts, || {
        fixture
            .artifacts
            .join("close-handled")
            .exists()
            .then_some(())
    });
    fixture.frame_command("CLOSE-TEST-SECONDARY", "focus");
    let state = fixture.observe_input();
    assert_eq!(state["protected-live"], true);
    assert!(fixture.editor.0.try_wait().unwrap().is_none());
}
