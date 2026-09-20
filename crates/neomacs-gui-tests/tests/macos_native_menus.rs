//! Requires a logged-in desktop and the permission-enabled GUI driver.
//! Explicitly fails prerequisites; a zero-test/skip result is never verification.
#![cfg(target_os = "macos")]
#[path = "native_menus/contracts.rs"]
mod contracts;
use neomacs_gui_tests::interaction::{DesktopDriver, macos::MacSession};
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
#[test]
fn shared_desktop_menu_contract() {
    run("menu-driver.el", false);
}
#[test]
fn shared_bottom_edge_contract() {
    run("menu-driver-tall.el", true);
}
fn run(fixture: &str, edges: bool) {
    let root = neomacs_infra::workspace_root();
    let artifacts = root.join(format!(
        "tmp/neomacs-gui-tests/macos-menu-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&artifacts).unwrap();
    eprintln!("macOS menu artifacts: {}", artifacts.display());
    let socket = std::env::var_os("NEOMACS_GUI_DRIVER_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("tmp/gui-driver.sock"));
    assert!(
        socket.exists(),
        "macOS GUI prerequisite: build scripts/build-macos-gui-driver.sh, then launch NeomacsGuiDriver.app in the desktop with socket {}",
        socket.display()
    );
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let mut editor = Editor(
        Command::new(binary)
            .args(["-Q", "-l"])
            .arg(root.join("crates/neomacs-gui-tests/fixtures").join(fixture))
            .env("NEOMACS_GUI_MENU_CONTROL", &artifacts)
            .env("NEOMACS_LOG_FILE", artifacts.join("neomacs.log"))
            .stdout(fs::File::create(artifacts.join("stdout.log")).unwrap())
            .stderr(fs::File::create(artifacts.join("stderr.log")).unwrap())
            .stdin(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(15);
    let state: serde_json::Value = loop {
        assert!(
            editor.0.try_wait().unwrap().is_none(),
            "editor exited; {}",
            artifacts.display()
        );
        if let Some(state) = fs::read(artifacts.join("state.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        {
            break state;
        }
        assert!(Instant::now() < deadline, "fixture readiness timeout");
        thread::sleep(Duration::from_millis(50));
    };
    let mut driver = MacSession::connect(&socket, editor.0.id())
        .expect("macOS GUI driver prerequisites (not a skipped test)");
    let observation = driver.observe().unwrap();
    let frame = observation
        .windows
        .iter()
        .find(|w| w.title == "NEOMACS-MENU-REPRO")
        .expect("observed editor")
        .bounds;
    if edges {
        contracts::exercise_bottom_edges(&mut driver, &artifacts);
    } else {
        contracts::exercise(
            &mut driver,
            &artifacts,
            frame,
            state["char-height"].as_f64().unwrap(),
            state["char-width"].as_f64().unwrap(),
        );
    }
    drop(driver);
    fs::write(artifacts.join("stop"), "stop").unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = editor.0.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(Instant::now() < deadline, "editor shutdown timeout");
        thread::sleep(Duration::from_millis(50));
    }
}
