//! #381: translated popup geometry must survive modifier-triggered redraw.
//! #387: real submenu input must preserve the native display connection.
//! Requires Weston (X11 backend/kiosk shell), Xvfb, xdotool, ImageMagick import,
//! and a fresh-built
//! release runtime. Executables may be overridden with NEOMACS_GUI_WESTON and
//! NEOMACS_GUI_TEST_BINARY. All pointer input goes to an isolated X server.
//! Weston 15 aborts in weston_coord_global_to_surface on this sequence;
//! Weston 16 updates the dirty parent transform and passes. Keep this test
//! behavioral: do not skip old compositors or assert a particular crash text.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::DisplayHarness;
use std::{
    fs,
    os::{fd::AsRawFd, unix::fs::PermissionsExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
struct Ready {
    char_width: u32,
    char_height: u32,
}

#[test]
fn native_submenu_hover_keeps_editor_and_compositor_alive() {
    with_native_menu(
        "native-menus.el",
        |env, window, ready, artifacts, editor, compositor| {
            let x = (ready.char_width * 3).to_string();
            let menu_y = (ready.char_height / 2).to_string();
            let row_y = (ready.char_height * 2).to_string();
            for cycle in 0..10 {
                let before = popup_requests(artifacts);
                input(
                    env,
                    &["mousemove", "--window", window, &x, &menu_y, "click", "1"],
                );
                wait_for("root popup", artifacts, || {
                    alive(compositor, "compositor", artifacts);
                    alive(editor, "editor", artifacts);
                    (popup_requests(artifacts) > before).then_some(())
                });
                input(env, &["mousemove", "--window", window, &x, &row_y]);
                wait_for("submenu request", artifacts, || {
                    alive(compositor, "compositor", artifacts);
                    alive(editor, "editor", artifacts);
                    (popup_requests(artifacts) >= before + 2).then_some(())
                });
                // get_popup is asynchronous; allow processing before dismissing.
                thread::sleep(Duration::from_millis(150));
                alive(compositor, "compositor after submenu", artifacts);
                alive(editor, "editor after submenu", artifacts);
                input(
                    env,
                    &["mousemove", "--window", window, "950", "650", "click", "1"],
                );
                thread::sleep(Duration::from_millis(100));
                alive(compositor, "compositor after dismissal", artifacts);
                alive(editor, "editor after dismissal", artifacts);
                fs::write(artifacts.join("completed-cycles"), (cycle + 1).to_string()).unwrap();
            }
        },
    );
}

#[test]
fn translated_menu_geometry_survives_control_key_redraw() {
    with_native_menu(
        "issue-381-translated-menu.el",
        |env, window, ready, artifacts, editor, compositor| {
            let x = (ready.char_width * 3).to_string();
            let y = (ready.char_height / 2).to_string();
            input(
                env,
                &["mousemove", "--window", window, &x, &y, "click", "1"],
            );
            wait_for("translated popup", artifacts, || {
                alive(editor, "editor opening translated popup", artifacts);
                (popup_requests(artifacts) > 0).then_some(())
            });
            let before = stable_menu_pixels(env, ready, artifacts, "before-control");
            input(env, &["keydown", "Control_L"]);
            let held = stable_menu_pixels(env, ready, artifacts, "control-held");
            input(env, &["keyup", "Control_L"]);
            let released = stable_menu_pixels(env, ready, artifacts, "control-released");
            alive(editor, "editor after Control", artifacts);
            alive(compositor, "compositor after Control", artifacts);
            let changed = |after: &image::RgbImage| {
                before
                    .pixels()
                    .zip(after.pixels())
                    .filter(|(a, b)| a != b)
                    .count()
            };
            assert_eq!(
                changed(&held),
                0,
                "Control must not change Chinese/Latin menu glyph spacing or shortcut alignment; artifacts: {}",
                artifacts.display()
            );
            assert_eq!(
                changed(&released),
                0,
                "releasing Control must preserve menu geometry; artifacts: {}",
                artifacts.display()
            );
        },
    );
}

#[test]
fn cjk_menu_bar_last_character_opens_its_own_menu() {
    with_native_menu(
        "issue-381-menu-bar.el",
        |env, window, ready, artifacts, editor, compositor| {
            // Within the fourth CJK glyph, beyond the old four-scalar hit box.
            let x = (16 + ready.char_width * 6).to_string();
            let y = (ready.char_height / 2).to_string();
            input(
                env,
                &["mousemove", "--window", window, &x, &y, "click", "1"],
            );
            wait_for("wide heading popup", artifacts, || {
                alive(editor, "opening wide heading", artifacts);
                (popup_requests(artifacts) > 0).then_some(())
            });
            let _ = stable_menu_pixels(env, ready, artifacts, "wide-heading-open");
            input(env, &["key", "Down", "Return"]);
            let selected = wait_for("heading command", artifacts, || {
                alive(editor, "selecting heading command", artifacts);
                alive(compositor, "selecting heading command", artifacts);
                fs::read_to_string(artifacts.join("selected-heading")).ok()
            });
            assert_eq!(
                selected,
                "first",
                "right side of CJK heading must open its own menu; artifacts: {}",
                artifacts.display()
            );
        },
    );
}

fn stable_menu_pixels(
    env: &[(String, String)],
    ready: &Ready,
    artifacts: &Path,
    name: &str,
) -> image::RgbImage {
    // Input delivery and native popup presentation are asynchronous.
    thread::sleep(Duration::from_millis(150));
    let path = artifacts.join(format!("{name}.png"));
    let mut previous = None;
    wait_for("stable menu screenshot", artifacts, || {
        let output = Command::new("import")
            .args(["-window", "root"])
            .arg(&path)
            .envs(env.iter().map(|(k, v)| (k, v)))
            .output()
            .expect("install ImageMagick import for native popup screenshots");
        assert!(
            output.status.success(),
            "screenshot: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        // The native popup belongs to a separate Wayland surface, so capture
        // the compositor output, not the editor's main-surface readback.
        // Exclude the menu bar and the minibuffer, which can echo modifiers.
        let pixels = image::open(&path)
            .unwrap()
            .crop_imm(0, ready.char_height, 600, ready.char_height * 6)
            .to_rgb8();
        let stable = previous.as_ref() == Some(&pixels);
        previous = Some(pixels.clone());
        stable.then_some(pixels)
    })
}

fn with_native_menu(
    fixture: &str,
    exercise: impl FnOnce(&[(String, String)], &str, &Ready, &Path, &mut OwnedChild, &mut OwnedChild),
) {
    let root = neomacs_infra::workspace_root();
    let artifacts = root.join(format!(
        "tmp/neomacs-gui-tests/native-menu-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&artifacts).unwrap();
    fs::set_permissions(&artifacts, fs::Permissions::from_mode(0o700)).unwrap();
    // Keep the handle alive: this short alias avoids Unix socket path limits.
    let directory = fs::File::open(&artifacts).unwrap();
    let runtime = format!("/proc/{}/fd/{}", std::process::id(), directory.as_raw_fd());
    let display = DisplayHarness::Xvfb.start_session(&artifacts).unwrap();
    let log = fs::File::create(artifacts.join("weston.log")).unwrap();
    let mut compositor = OwnedChild(
        Command::new(std::env::var_os("NEOMACS_GUI_WESTON").unwrap_or_else(|| "weston".into()))
            .args([
                "--backend=x11",
                "--renderer=gl",
                "--shell=kiosk-shell.so",
                "--no-config",
                "--width=1000",
                "--height=700",
                "--socket=menu-test",
            ])
            .envs(display.env().iter().map(|(k, v)| (k, v)))
            .env("XDG_RUNTIME_DIR", &runtime)
            .env_remove("WAYLAND_DISPLAY")
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .expect("install Weston with X11 backend and kiosk shell"),
    );
    wait_for("Wayland socket", &artifacts, || {
        alive(&mut compositor, "compositor", &artifacts);
        artifacts.join("menu-test").exists().then_some(())
    });
    // Read the exact outer X11 window ID, not a localized title or PID property.
    let window = wait_for("Weston X11 window", &artifacts, || {
        let log = fs::read_to_string(artifacts.join("weston.log")).ok()?;
        log.lines().find_map(|l| {
            l.split_once("window id ")
                .map(|(_, id)| id.trim().to_owned())
        })
    });
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let mut editor = OwnedChild(
        Command::new(binary)
            .args(["-Q", "-l"])
            .arg(root.join("crates/neomacs-gui-tests/fixtures").join(fixture))
            .env("XDG_RUNTIME_DIR", &runtime)
            .env("WAYLAND_DISPLAY", "menu-test")
            .env_remove("DISPLAY")
            .env("WINIT_UNIX_BACKEND", "wayland")
            // Avoid the separate software Vulkan/Weston GL dma-buf import failure.
            .env("WGPU_BACKEND", "gl")
            .env("NEOMACS_GUI_MENU_CONTROL", &artifacts)
            .env("WAYLAND_DEBUG", "1")
            .env("RUST_LOG", "warn,neomacs_display_runtime=debug")
            .env("NEOMACS_LOG_FILE", artifacts.join("neomacs.log"))
            .stdout(fs::File::create(artifacts.join("stdout.log")).unwrap())
            .stderr(fs::File::create(artifacts.join("wayland.log")).unwrap())
            .stdin(Stdio::null())
            .spawn()
            .expect("fresh-build the editor runtime"),
    );
    let ready: Ready = wait_for("fixture readiness", &artifacts, || {
        alive(&mut compositor, "compositor", &artifacts);
        alive(&mut editor, "editor", &artifacts);
        let state: serde_json::Value =
            serde_json::from_slice(&fs::read(artifacts.join("state.json")).ok()?).ok()?;
        Some(Ready {
            char_width: state["char-width"].as_u64()?.try_into().ok()?,
            char_height: state["char-height"].as_u64()?.try_into().ok()?,
        })
    });
    input(display.env(), &["windowfocus", &window]);
    exercise(
        display.env(),
        &window,
        &ready,
        &artifacts,
        &mut editor,
        &mut compositor,
    );
    fs::write(artifacts.join("stop"), "stop").unwrap();
    let status = wait_for("editor shutdown", &artifacts, || {
        editor.0.try_wait().unwrap()
    });
    assert!(
        status.success(),
        "editor exit: {status}; artifacts: {}",
        artifacts.display()
    );
}

fn alive(child: &mut OwnedChild, role: &str, artifacts: &Path) {
    if let Some(status) = child.0.try_wait().unwrap() {
        panic!(
            "{role} exited unexpectedly: {status}; artifacts: {}",
            artifacts.display()
        );
    }
}
fn popup_requests(artifacts: &Path) -> usize {
    fs::read_to_string(artifacts.join("wayland.log"))
        .unwrap_or_default()
        .lines()
        .filter(|line| line.contains(".get_popup("))
        .count()
}
fn wait_for<T>(label: &str, artifacts: &Path, mut observe: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
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
fn input(env: &[(String, String)], args: &[&str]) {
    let output = Command::new("xdotool")
        .args(args)
        .envs(env.iter().map(|(k, v)| (k, v)))
        .output()
        .expect("install xdotool");
    assert!(
        output.status.success(),
        "xdotool {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
