//! Native precise scroll diagnostic on an isolated headless Wayland compositor.
//! This exercises the shared PixelDelta path, not AppKit's event generation.
#![cfg(target_os = "linux")]

use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command},
    thread,
    time::{Duration, Instant},
};
#[path = "native_scrolling/wayland.rs"]
mod wayland;

struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn state(path: &Path, after: u64) -> Value {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if let Ok(bytes) = fs::read(path)
            && let Ok(value) = serde_json::from_slice::<Value>(&bytes)
            && value["sample"]
                .as_u64()
                .is_some_and(|sample| sample > after)
        {
            return value;
        }
        assert!(Instant::now() < deadline, "no fresh state: {path:?}");
        thread::sleep(Duration::from_millis(20));
    }
}

fn readback(path: &Path) -> image::DynamicImage {
    // The running renderer rewrites its diagnostic PNG on each frame. A
    // timer sample does not synchronize file publication: wait for a complete
    // PNG instead of treating a concurrent write as a rendering failure.
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        match image::open(path) {
            Ok(image) => return image,
            Err(error) => {
                assert!(
                    Instant::now() < deadline,
                    "no complete GUI readback at {path:?}: {error}"
                );
                thread::sleep(Duration::from_millis(20));
            }
        }
    }
}

#[test]
// Prerequisites: fresh release Neomacs/runtime and sway on PATH.
fn precise_native_scroll_advances_without_snapback() {
    run_native_scroll(ScrollKind::Precise, ScrollTarget::Selected);
}

#[test]
fn native_wheel_scroll_advances_without_snapback() {
    run_native_scroll(ScrollKind::Wheel, ScrollTarget::Selected);
}

#[test]
fn precise_native_scroll_targets_the_unselected_window_under_the_pointer() {
    run_native_scroll(ScrollKind::Precise, ScrollTarget::OtherWindow);
}

#[derive(Clone, Copy, Debug)]
enum ScrollKind {
    Precise,
    Wheel,
}

#[derive(Clone, Copy, Debug)]
enum ScrollTarget {
    Selected,
    OtherWindow,
}

fn run_native_scroll(kind: ScrollKind, target: ScrollTarget) {
    let root = neomacs_infra::workspace_root();
    let artifact_root = root.join("target/neomacs-gui-tests");
    fs::create_dir_all(&artifact_root).unwrap();
    let artifacts = artifact_root.join(format!(
        "native-scrolling-{kind:?}-{target:?}-{}",
        std::process::id()
    ));
    fs::create_dir(&artifacts).unwrap();
    let session = neomacs_infra::display::start_sway(
        &artifacts,
        r#"
output * resolution 1000x700
xwayland disable
seat seat0 fallback true
default_border none
focus_follows_mouse yes
"#,
    )
    .expect("start sway");
    let session_env: std::collections::HashMap<String, String> =
        session.env().iter().cloned().collect();
    let runtime = session_env["XDG_RUNTIME_DIR"].clone();
    let socket = session_env["WAYLAND_DISPLAY"].clone();
    let state_path = artifacts.join("state.json");
    let pixels_path = artifacts.join("surface.png");
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let mut command = Command::new(binary);
    command.env_remove("NEOMACS_GUI_SCROLL_OTHER_WINDOW");
    if matches!(target, ScrollTarget::OtherWindow) {
        command.env("NEOMACS_GUI_SCROLL_OTHER_WINDOW", "1");
    }
    let mut editor = OwnedChild(
        command
            .args(["-Q", "-l"])
            .arg(root.join("crates/neomacs-gui-tests/fixtures/native-scrolling.el"))
            .env("XDG_RUNTIME_DIR", &runtime)
            .env("WAYLAND_DISPLAY", &socket)
            .env("WINIT_UNIX_BACKEND", "wayland")
            .env_remove("DISPLAY")
            .env("NEOMACS_GUI_STATE_JSON", &state_path)
            .env("NEOMACS_DEBUG_SURFACE_READBACK", "10000")
            .env("NEOMACS_DEBUG_SURFACE_READBACK_PNG", &pixels_path)
            .env("WAYLAND_DEBUG", "1")
            .env(
                "RUST_LOG",
                "warn,neomacs=debug,neomacs_display_runtime=debug",
            )
            .env("NEOMACS_LOG_FILE", artifacts.join("neomacs.log"))
            .stdout(fs::File::create(artifacts.join("stdout")).unwrap())
            .stderr(fs::File::create(artifacts.join("stderr")).unwrap())
            .spawn()
            .unwrap(),
    );
    let mut trackpad = wayland::Trackpad::connect(&PathBuf::from(&runtime).join(&socket));
    let initial = state(&state_path, 2);
    trackpad.move_to_body();
    thread::sleep(Duration::from_millis(200));
    let mut previous = state(&state_path, initial["sample"].as_u64().unwrap());
    let initial_pixels = readback(&pixels_path);
    initial_pixels.save(artifacts.join("before.png")).unwrap();
    // Sample text away from point at the left edge: moving the cursor alone
    // must not make a stale text presentation look like successful scrolling.
    let text_pixels = |pixels: &image::DynamicImage| {
        pixels
            .crop_imm(
                pixels.width() / 25,
                pixels.height() / 8,
                pixels.width() / 2,
                pixels.height() / 2,
            )
            .to_rgba8()
    };
    let initial_text = text_pixels(&initial_pixels);
    let position = |s: &Value| (s["start"].as_i64().unwrap(), s["vscroll"].as_i64().unwrap());
    let initial_position = position(&previous);
    let mut trace = vec![previous.clone()];
    let steps = match kind {
        ScrollKind::Precise => 24,
        ScrollKind::Wheel => 12,
    };
    for step in 0..steps {
        let down = step < 12;
        match kind {
            ScrollKind::Precise => trackpad.scroll(if down { 4.0 } else { -4.0 }),
            ScrollKind::Wheel => trackpad.wheel(),
        }
        thread::sleep(Duration::from_millis(500));
        let current = state(&state_path, previous["sample"].as_u64().unwrap());
        trace.push(current.clone());
        fs::write(
            artifacts.join("trace.json"),
            serde_json::to_vec_pretty(&trace).unwrap(),
        )
        .unwrap();
        let pixels = readback(&pixels_path);
        pixels
            .save(artifacts.join(format!("step-{step}.png")))
            .unwrap();
        eprintln!("step={step} before={previous} after={current}; artifacts={artifacts:?}");
        assert!(editor.0.try_wait().unwrap().is_none(), "editor exited");
        assert_eq!(
            current["selected"], initial["selected"],
            "scrolling must preserve window selection"
        );
        if matches!(target, ScrollTarget::OtherWindow) {
            assert_eq!(
                current["selected-start"], initial["selected-start"],
                "scrolling the other window must not move the selected viewport"
            );
        }
        let native_log = fs::read_to_string(artifacts.join("neomacs.log")).unwrap();
        let expected_event = match kind {
            ScrollKind::Precise => "PixelScroll {",
            ScrollKind::Wheel => "MouseScroll {",
        };
        assert!(
            native_log.contains(expected_event),
            "native input did not reach the VM bridge as {expected_event}: {artifacts:?}"
        );
        assert!(
            if down {
                position(&current) > position(&previous)
            } else {
                position(&current) < position(&previous)
            },
            "{kind:?} scrolling must follow the gesture without snapping back: down={down}, before={previous}, after={current}; artifacts={artifacts:?}"
        );
        if step == 0 && matches!(kind, ScrollKind::Precise) {
            assert_eq!(
                position(&current),
                (initial_position.0, initial_position.1 + 4),
                "a four-pixel gesture must not fall back to whole-line wheel scrolling"
            );
        }
        previous = current;
        if step == 11 {
            assert!(
                text_pixels(&pixels) != initial_text,
                "scrolling must update rendered text, not only Lisp state: {artifacts:?}"
            );
        }
    }
    if matches!(kind, ScrollKind::Precise) {
        assert_eq!(
            position(&previous),
            initial_position,
            "opposite precise gestures must return to the initial viewport"
        );
    }
}
