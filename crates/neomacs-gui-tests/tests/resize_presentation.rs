use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use neomacs_gui_tests::{DisplayHarness, GuiArtifactSet, GuiBackend};

#[test]
fn real_gui_resize_does_not_ghost_the_previous_presentation() {
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
    let artifact_root = workspace_root.join("target/neomacs-gui-tests");
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifact_root)
        .expect("display session should start");
    let artifacts = GuiArtifactSet::new(&artifact_root, backend, "resize-presentation");
    std::fs::create_dir_all(
        artifacts
            .png
            .parent()
            .expect("resize artifact should have a parent"),
    )
    .expect("resize artifact directory");
    let before_png = artifacts
        .png
        .with_file_name("resize-presentation.before.png");
    let ready_path = artifacts.png.with_file_name("resize-presentation.ready");
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
        .env("RUST_LOG", "info")
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    let child = command.spawn().expect("start Neomacs resize fixture");
    let pid = child.id();
    let mut child = KillOnDrop(Some(child));

    let window = wait_for_x11_window(pid, session.env(), Duration::from_secs(12));
    run_x11_tool(session.env(), "xdotool", ["windowmap", "--sync", &window]);
    wait_for_path(&ready_path, Duration::from_secs(12));
    let (before, old_mode_rows) =
        wait_for_red_mode_line(session.env(), &window, &before_png, Duration::from_secs(12));

    let new_width = 1100_u32;
    let new_height = 760_u32;
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
        None => {
            eprintln!("skipping resize presentation test; set NEOMACS_GUI_TEST_BACKEND=x11");
            false
        }
        Some("x11" | "linux-x11") => true,
        Some("wayland" | "linux-wayland" | "macos" | "windows") => {
            eprintln!("skipping resize presentation test; native window capture is X11-only");
            false
        }
        Some(other) => panic!("unsupported NEOMACS_GUI_TEST_BACKEND={other:?}"),
    }
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_WORKSPACE_DIR"))
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
            .args(["search", "--pid", &pid.to_string()])
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

fn is_red_tinted([red, green, blue, _alpha]: [u8; 4]) -> bool {
    red >= 180 && red.saturating_sub(green) >= 10 && red.saturating_sub(blue) >= 10
}
