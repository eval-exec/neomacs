use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use neomacs_gui_tests::{DisplayHarness, GuiArtifactSet, GuiBackend};

#[test]
fn real_gui_resize_does_not_ghost_the_previous_presentation() {
    run_resize_test(ResizeScenario::StartupText);
}

#[test]
fn idle_gui_resize_updates_content_without_keyboard_or_mouse_input() {
    run_resize_test(ResizeScenario::IdleEmpty);
}

#[test]
fn idle_split_gui_resize_updates_content_without_keyboard_or_mouse_input() {
    run_resize_test(ResizeScenario::IdleSplit);
}

// Real pointer motion publishes a hit observation before the movement itself.
// Issue 391: this observation must not block later native resize events.
#[test]
fn pointer_motion_does_not_block_idle_native_resize() {
    run_resize_test(ResizeScenario::IdleAfterPointerMotion);
}

#[test]
fn focus_change_does_not_block_idle_native_resize() {
    run_resize_test(ResizeScenario::IdleAfterFocusChange);
}

#[derive(Clone, Copy)]
enum ResizeScenario {
    StartupText,
    IdleEmpty,
    IdleSplit,
    IdleAfterPointerMotion,
    IdleAfterFocusChange,
}

impl ResizeScenario {
    fn is_idle(self) -> bool {
        matches!(
            self,
            Self::IdleEmpty
                | Self::IdleSplit
                | Self::IdleAfterPointerMotion
                | Self::IdleAfterFocusChange
        )
    }
}

fn run_resize_test(scenario: ResizeScenario) {
    if !x11_backend_requested() {
        return;
    }

    require_tool("xdotool");
    require_tool("import");

    let workspace_root = workspace_root();
    let binary = neomacs_binary(&workspace_root);
    assert!(
        binary.exists(),
        "build {binary:?} before running the resize presentation test"
    );

    let backend = GuiBackend::LinuxX11;
    let artifact_root = workspace_root
        .join("target/neomacs-gui-tests")
        .join(format!("resize-{}", std::process::id()));
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifact_root)
        .expect("display session should start");
    let name = match scenario {
        ResizeScenario::StartupText => "resize-presentation",
        ResizeScenario::IdleEmpty => "idle-resize-presentation",
        ResizeScenario::IdleSplit => "idle-split-resize-presentation",
        ResizeScenario::IdleAfterPointerMotion => "pointer-motion-resize-presentation",
        ResizeScenario::IdleAfterFocusChange => "focus-change-resize-presentation",
    };
    let artifacts = GuiArtifactSet::new(&artifact_root, backend, name);
    std::fs::create_dir_all(
        artifacts
            .png
            .parent()
            .expect("resize artifact should have a parent"),
    )
    .expect("resize artifact directory");
    let before_png = artifacts.png.with_file_name(format!("{name}.before.png"));
    let ready_path = artifacts.png.with_file_name(format!("{name}.ready"));
    for path in [
        &before_png,
        &artifacts.png,
        &artifacts.neomacs_log,
        &ready_path,
    ] {
        let _ = std::fs::remove_file(path);
    }

    let stdout = std::fs::File::create(&artifacts.stdout).expect("create stdout artifact");
    let stderr = std::fs::File::create(&artifacts.stderr).expect("create stderr artifact");
    let mut command = Command::new(binary);
    command
        .args([
            "-Q",
            "-l",
            workspace_root
                .join("crates/neomacs-gui-tests/fixtures/resize-presentation.el")
                .to_str()
                .expect("fixture path should be UTF-8"),
        ])
        .envs(session.env().iter().map(|(key, value)| (key, value)))
        .env("WINIT_UNIX_BACKEND", "x11")
        .env("NEOMACS_LOG_FILE", &artifacts.neomacs_log)
        .env("NEOMACS_DUMP_FRAME_GLYPHS", "1")
        .env("NEOMACS_GUI_RESIZE_READY", &ready_path)
        .env_remove("NEOMACS_GUI_RESIZE_AFTER_IDLE")
        .env_remove("NEOMACS_GUI_RESIZE_SPLIT")
        .env("RUST_LOG", "info")
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    if scenario.is_idle() {
        command.env("NEOMACS_GUI_RESIZE_AFTER_IDLE", "1");
    }
    if let ResizeScenario::IdleSplit = scenario {
        command.env("NEOMACS_GUI_RESIZE_SPLIT", "1");
    }
    let child = command.spawn().expect("start Neomacs resize fixture");
    let pid = child.id();
    let mut child = KillOnDrop(Some(child));

    let window = wait_for_x11_window(pid, session.env(), Duration::from_secs(12));
    run_x11_tool(session.env(), "xdotool", ["windowmap", "--sync", &window]);
    wait_for_path(&ready_path, Duration::from_secs(12));
    let (before, old_mode_rows) =
        wait_for_red_mode_line(session.env(), &window, &before_png, Duration::from_secs(12));

    if scenario.is_idle() {
        // The one-shot idle callback has reported readiness. Let it return
        // before issuing the native resize; no periodic Lisp observer runs.
        thread::sleep(Duration::from_millis(300));
    }

    if let ResizeScenario::IdleAfterFocusChange = scenario {
        // Window managers change focus during a border drag. A focus event
        // must remain readable even when input-pending-p would filter it.
        run_x11_tool(session.env(), "xdotool", ["windowfocus", "--sync", &window]);
        thread::sleep(Duration::from_millis(100));
    }

    if let ResizeScenario::IdleAfterPointerMotion = scenario {
        // Do not click: that is precisely the unrelated command input which
        // rescues the stalled queue in issue 391. Native pointer movement is
        // sufficient to reproduce it on a bare X server, without a WM.
        run_x11_tool(
            session.env(),
            "xdotool",
            ["mousemove", "--sync", "--window", &window, "100", "100"],
        );
        thread::sleep(Duration::from_millis(100));
    }

    let new_width = 1100_u32;
    let new_height = 760_u32;
    if scenario.is_idle() {
        // Replay the reporter's burst of native size notifications, instead
        // of testing only one isolated resize during startup.
        for step in 1..8 {
            let width = (before.width() * (8 - step) + new_width * step) / 8;
            let height = (before.height() * (8 - step) + new_height * step) / 8;
            run_x11_tool(
                session.env(),
                "xdotool",
                [
                    "windowsize",
                    "--sync",
                    &window,
                    &width.to_string(),
                    &height.to_string(),
                ],
            );
            thread::sleep(Duration::from_millis(20));
        }
    }
    run_x11_tool(
        session.env(),
        "xdotool",
        [
            "windowsize",
            "--sync",
            &window,
            &new_width.to_string(),
            &new_height.to_string(),
        ],
    );
    if scenario.is_idle() {
        // Observe the actual presentation, not a VM log or a Lisp polling
        // timer (which could itself wake redisplay and mask the bug).
        // The bottom mode line must remain the same distance from the
        // bottom of the native window, and span its new width.
        let bottom_inset = before.height() - old_mode_rows.last().unwrap();
        wait_for_resized_mode_line(
            session.env(),
            &window,
            &artifacts.png,
            (new_width, new_height),
            bottom_inset,
            Duration::from_secs(3),
        );
        return;
    }
    wait_for_log(
        &artifacts.neomacs_log,
        &format!("size={new_width}x{new_height}"),
        Duration::from_secs(30),
    );

    // Allow one ordinary 60 Hz present after the correctly-sized display
    // matrix arrives, while remaining well inside the historical 200 ms
    // resize crossfade that exposed the stale presentation.
    thread::sleep(Duration::from_millis(50));
    capture_x11_window(session.env(), &window, &artifacts.png);

    let resized = image::open(&artifacts.png)
        .expect("decode resized window capture")
        .to_rgba8();
    assert_eq!(resized.dimensions(), (new_width, new_height));
    let old_band_red_pixels = old_mode_rows
        .iter()
        .filter(|&&y| y < resized.height())
        .flat_map(|&y| (0..before.width().min(resized.width())).map(move |x| (x, y)))
        .filter(|&(x, y)| is_red_tinted(resized.get_pixel(x, y).0))
        .count();

    // The only red pixels in a current presentation belong to its new mode
    // line near the bottom of the enlarged window. Red at the old mode-line
    // rows proves that pixels from the old-size presentation are still being
    // composed over the current one.
    assert_eq!(
        old_band_red_pixels,
        0,
        "resized GUI retained {old_band_red_pixels} red pixels at the old mode-line rows {:?}; \
         before={} resized={} log={}",
        old_mode_rows,
        before_png.display(),
        artifacts.png.display(),
        artifacts.neomacs_log.display(),
    );

    if let Some(mut process) = child.0.take() {
        let _ = process.kill();
        let _ = process.wait();
    }
}

fn x11_backend_requested() -> bool {
    match std::env::var("NEOMACS_GUI_TEST_BACKEND").ok().as_deref() {
        None => cfg!(target_os = "linux"),
        Some("x11" | "linux-x11") => true,
        Some("wayland" | "linux-wayland" | "macos" | "windows") => {
            eprintln!("skipping resize presentation test; native window capture is X11-only");
            false
        }
        Some(other) => panic!("unsupported NEOMACS_GUI_TEST_BACKEND={other:?}"),
    }
}

fn workspace_root() -> PathBuf {
    neomacs_infra::workspace_root()
}

fn neomacs_binary(workspace_root: &Path) -> PathBuf {
    std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root.join("target/release/neomacs"))
}

struct KillOnDrop(Option<Child>);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn require_tool(program: &str) {
    assert!(
        Command::new(program).arg("-version").output().is_ok(),
        "{program} is required for the X11 resize presentation test"
    );
}

fn wait_for_x11_window(pid: u32, display_env: &[(String, String)], timeout: Duration) -> String {
    let started = Instant::now();
    loop {
        let output = Command::new("xdotool")
            .args(["search", "--onlyvisible", "--pid", &pid.to_string()])
            .envs(display_env.iter().map(|(key, value)| (key, value)))
            .output()
            .expect("run xdotool search");
        if output.status.success()
            && let Some(window) = String::from_utf8_lossy(&output.stdout)
                .lines()
                .rfind(|line| !line.is_empty())
        {
            return window.to_string();
        }
        assert!(
            started.elapsed() < timeout,
            "Neomacs PID {pid} did not create an X11 window within {timeout:?}"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

fn wait_for_log(path: &Path, needle: &str, timeout: Duration) {
    let started = Instant::now();
    loop {
        let contents = std::fs::read_to_string(path).unwrap_or_default();
        if contents.contains(needle) {
            return;
        }
        assert!(
            started.elapsed() < timeout,
            "{} did not contain {needle:?} within {timeout:?}; tail:\n{}",
            path.display(),
            contents
                .lines()
                .rev()
                .take(30)
                .collect::<Vec<_>>()
                .join("\n")
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn wait_for_path(path: &Path, timeout: Duration) {
    let started = Instant::now();
    while !path.exists() {
        assert!(
            started.elapsed() < timeout,
            "{} was not created within {timeout:?}",
            path.display()
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn run_x11_tool<const N: usize>(display_env: &[(String, String)], program: &str, args: [&str; N]) {
    let output = Command::new(program)
        .args(args)
        .envs(display_env.iter().map(|(key, value)| (key, value)))
        .output()
        .unwrap_or_else(|error| panic!("run {program}: {error}"));
    assert!(
        output.status.success(),
        "{program} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn capture_x11_window(display_env: &[(String, String)], window: &str, output_path: &Path) {
    let output = Command::new("import")
        .args(["-window", window])
        .arg(output_path)
        .envs(display_env.iter().map(|(key, value)| (key, value)))
        .output()
        .expect("capture X11 window");
    assert!(
        output.status.success(),
        "window capture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn wait_for_red_mode_line(
    display_env: &[(String, String)],
    window: &str,
    output_path: &Path,
    timeout: Duration,
) -> (image::RgbaImage, Vec<u32>) {
    let started = Instant::now();
    loop {
        capture_x11_window(display_env, window, output_path);
        let image = image::open(output_path)
            .expect("decode pre-resize window capture")
            .to_rgba8();
        let rows = red_tinted_rows(&image);
        if !rows.is_empty() {
            return (image, rows);
        }
        assert!(
            started.elapsed() < timeout,
            "fixture did not visibly present its saturated red mode line in {} within {timeout:?}",
            output_path.display()
        );
        thread::sleep(Duration::from_millis(20));
    }
}

fn red_tinted_rows(image: &image::RgbaImage) -> Vec<u32> {
    (0..image.height())
        .filter(|&y| {
            let red_pixels = (0..image.width())
                .filter(|&x| is_red_tinted(image.get_pixel(x, y).0))
                .count();
            red_pixels > image.width() as usize / 2
        })
        .collect()
}

fn wait_for_resized_mode_line(
    display_env: &[(String, String)],
    window: &str,
    output_path: &Path,
    size: (u32, u32),
    bottom_inset: u32,
    timeout: Duration,
) {
    let started = Instant::now();
    loop {
        capture_x11_window(display_env, window, output_path);
        let image = image::open(output_path)
            .expect("decode resized window capture")
            .to_rgba8();
        let expected_row = size.1.saturating_sub(bottom_inset);
        let wide_red_row = image.dimensions() == size
            && (expected_row.saturating_sub(2)..=(expected_row + 2).min(size.1 - 1)).any(|y| {
                let red_pixels = (0..size.0)
                    .filter(|&x| is_red_tinted(image.get_pixel(x, y).0))
                    .count();
                red_pixels * 10 >= size.0 as usize * 9
            });
        if wide_red_row {
            return;
        }
        assert!(
            started.elapsed() < timeout,
            "content did not resize without keyboard or mouse input within {timeout:?}: \
             expected mode line at y={expected_row} spanning width {}, \
             actual image {:?}, red rows {:?}; capture={}",
            size.0,
            image.dimensions(),
            red_tinted_rows(&image),
            output_path.display(),
        );
        thread::sleep(Duration::from_millis(20));
    }
}

fn is_red_tinted([red, green, blue, _alpha]: [u8; 4]) -> bool {
    red >= 180 && red.saturating_sub(green) >= 10 && red.saturating_sub(blue) >= 10
}
