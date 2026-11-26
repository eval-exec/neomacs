//! #265: scaled cursor geometry against GNU; Neo paint edges.
#![cfg(target_os = "linux")]
use neomacs_gui_tests::{
    DisplayHarness, GuiArtifactSet, GuiBackend, GuiCommandRunner, GuiRunOptions, GuiScenario,
    GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Output},
    thread,
    time::{Duration, Instant},
};

struct OwnedHelper(Child);
impl Drop for OwnedHelper {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn scaled_cursor_with_line_numbers_and_hl_line_matches_gnu() {
    check_scaled_cursor(GuiBackend::LinuxX11);
}

#[test]
fn wayland_scaled_cursor_with_line_numbers_and_hl_line_matches_gnu() {
    check_scaled_cursor(GuiBackend::LinuxWayland);
}

fn check_scaled_cursor(native: GuiBackend) {
    let root = neomacs_infra::workspace_root();
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let oracle = neomacs_test_oracle::gnu::gnu_oracle();
    assert!(oracle.is_attested(), "requires the pinned GNU oracle");
    eprintln!("{}", oracle.stamp());
    let artifacts = root.join(format!(
        "tmp/cursor265-{}-{}",
        native.slug(),
        std::process::id()
    ));
    fs::create_dir_all(&artifacts).unwrap();
    let session = DisplayHarness::Xvfb.start_session(&artifacts).unwrap();
    let fontconfig = crate::desktop_font_pins::desktop_font_fontconfig(&artifacts);
    let wayland = (native == GuiBackend::LinuxWayland).then(|| {
        neomacs_infra::display::start_sway(
            &artifacts.join("wayland"),
            "output * resolution 1280x800\nxwayland disable\nseat seat0 fallback true\n",
        )
        .unwrap()
    });
    // Headless Sway starts with WLR_LIBINPUT_NO_DEVICES=1, so keep one idle
    // virtual keyboard attached for the session instead of racing transient
    // WL_SEAT_CAPABILITY_KEYBOARD attach/detach on every subcase under load.
    let _wayland_keyboard_holder = wayland.as_ref().map(|session| {
        OwnedHelper(
            Command::new("wtype")
                .envs(session.env().iter().cloned())
                .args(["-s", "120000"])
                .spawn()
                .expect("wtype is required for Wayland scaled-cursor testing"),
        )
    });
    let highlight = if native == GuiBackend::LinuxWayland {
        "global"
    } else {
        "local"
    };
    for scale in [-2, 0, 2, 4] {
        for edge in ["beginning", "end"] {
            let native_env = wayland
                .as_ref()
                .map_or(session.env(), |session| session.env());
            for (engine, program, backend, env) in [
                (
                    "gnu",
                    oracle.executable(),
                    GuiBackend::LinuxX11,
                    session.env(),
                ),
                ("neo", binary.as_path(), native, native_env),
            ] {
                let dir = artifacts.join(format!("{engine}-{scale}-{edge}"));
                fs::create_dir_all(&dir).unwrap();
                let case = CursorCase {
                    engine,
                    scale,
                    edge,
                    backend,
                    dir,
                    env,
                };
                run_case(&case, &root, program, &fontconfig, highlight);
            }
        }
    }
}

struct CursorCase<'a> {
    engine: &'a str,
    scale: i32,
    edge: &'a str,
    backend: GuiBackend,
    dir: PathBuf,
    env: &'a [(String, String)],
}

fn run_case(
    case: &CursorCase<'_>,
    root: &Path,
    program: &Path,
    fontconfig: &Path,
    highlight: &str,
) {
    let CursorCase {
        engine,
        scale,
        edge,
        backend,
        dir,
        env,
    } = case;
    let state_file = dir.join("state.json");
    let script = root.join("crates/neomacs-gui-tests/fixtures/line-number-scaled-cursor.el");
    let mut plan = GuiTestPlan::new(
        *backend,
        root,
        dir,
        GuiScenario::new("scaled-cursor", &script),
    )
    .with_program(program)
    .with_env("CURSOR265_RESULT", state_file.display().to_string())
    .with_env("CURSOR265_SCALE", scale.to_string())
    .with_env("CURSOR265_EDGE", *edge)
    .with_env("CURSOR265_HIGHLIGHT", highlight)
    .with_env(
        "GDK_BACKEND",
        if *backend == GuiBackend::LinuxWayland {
            "wayland"
        } else {
            "x11"
        },
    )
    .with_env("FONTCONFIG_FILE", fontconfig.display().to_string())
    .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "10000");
    for (key, value) in *env {
        plan = plan.with_env(key, value);
    }
    let options = GuiRunOptions::with_timeout(Duration::from_secs(20));
    let readback = GuiArtifactSet::new(dir, *backend, "scaled-cursor").png;
    thread::scope(|scope| {
        let driver = scope.spawn(|| drive(case, &state_file, &readback));
        if *engine == "gnu" {
            let files = plan.write_manifest().unwrap();
            let output = ProcessGuiCommandRunner
                .run(&plan.command_spec(), &files, &options)
                .unwrap();
            fs::write(&files.stderr, &output.stderr).unwrap();
            assert_eq!(output.exit_code, Some(0), "{dir:?}");
        } else {
            let output = plan
                .run_with(&mut ProcessGuiCommandRunner, options)
                .unwrap();
            assert_eq!(output.exit_code, Some(0), "{output:#?}");
        };
        driver.join().unwrap();
    });
    assert_state(case, &state_file);
    if *engine == "neo" {
        assert_pixels(case, &dir.join("observed.png"));
    }
}

fn drive(case: &CursorCase<'_>, state_file: &Path, readback: &Path) {
    let dir = &case.dir;
    let ready = state_file.with_extension("json.ready");
    let deadline = Instant::now() + Duration::from_secs(15);
    while !ready.exists() {
        assert!(Instant::now() < deadline, "fixture not ready: {dir:?}");
        thread::sleep(Duration::from_millis(20));
    }
    send_input(case, &ready);
    if case.engine != "neo" {
        return;
    }
    // Wait for the state file and a complete diagnostic PNG before asserting
    // cursor geometry/pixels. The 10 s cap is hang-avoidance only (not a
    // latency assertion): the fixture writes state.json once on the command
    // and exits, so a missing state/readback cannot self-heal under a wider cap.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let state_ready = fs::read(state_file)
            .ok()
            .is_some_and(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).is_ok());
        if state_ready
            && let Ok(bytes) = fs::read(readback)
            && image::load_from_memory(&bytes).is_ok()
        {
            fs::write(dir.join("observed.png"), bytes).unwrap();
            break;
        }
        assert!(
            Instant::now() < deadline,
            "cursor observation/readback missing within 10s hang-avoidance cap: {dir:?}"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn send_input(case: &CursorCase<'_>, ready: &Path) {
    let CursorCase {
        backend,
        env,
        dir,
        edge,
        ..
    } = case;
    if *backend == GuiBackend::LinuxWayland {
        let runtime = env
            .iter()
            .find(|(key, _)| key == "XDG_RUNTIME_DIR")
            .unwrap()
            .1
            .clone();
        let _focused = crate::wayland_readiness::wait_for_focused_child(Path::new(&runtime), dir);
        let key = if *edge == "end" { "e" } else { "a" };
        run_input(
            case,
            "wtype",
            &[
                "-d", "100", "-k", "Shift_L", "-s", "300", "-M", "ctrl", "-k", key, "-m", "ctrl",
            ],
        );
    } else {
        let pid = fs::read_to_string(ready).unwrap();
        let input =
            |args: &[&str]| String::from_utf8(run_input(case, "xdotool", args).stdout).unwrap();
        let windows = input(&["search", "--onlyvisible", "--pid", pid.trim()]);
        let window = windows.lines().last().expect("editor window");
        input(&["windowfocus", "--sync", window]);
        input(&[
            "key",
            "--clearmodifiers",
            if *edge == "end" { "ctrl+e" } else { "ctrl+a" },
        ]);
    }
}

fn run_input(case: &CursorCase<'_>, tool: &str, arguments: &[&str]) -> Output {
    let mut command = Command::new(tool);
    command.args(arguments).envs(case.env.iter().cloned());
    let output = if tool == "wtype" {
        neomacs_gui_tests::run_input_tool(command, Duration::from_secs(10))
    } else {
        command.output().unwrap()
    };
    assert!(
        output.status.success(),
        "{tool}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn assert_state(case: &CursorCase<'_>, state_file: &Path) {
    let CursorCase {
        engine,
        scale,
        edge,
        dir,
        ..
    } = case;
    let state: serde_json::Value =
        serde_json::from_slice(&fs::read(&state_file).expect("fixture state")).unwrap();
    eprintln!("{engine}/{scale}/{edge}: {state}; {dir:?}");
    assert_eq!(state["family"], "DejaVu Sans Mono");
    assert_eq!(state["point"], if *edge == "end" { 9 } else { 1 });
    assert_eq!(state["scale"].as_f64(), Some(f64::from(*scale)));
    assert_eq!(state["line_numbers"], true);
    assert_eq!(state["highlight"], true);
    let expected = state[if *edge == "end" { "eol" } else { "bol" }]
        .as_i64()
        .unwrap();
    let cell_width = (state["eol"].as_i64().unwrap() - state["bol"].as_i64().unwrap()) / 8;
    assert_eq!(
        state["cursor"][3].as_i64(),
        Some(cell_width),
        "cursor width"
    );
    assert_eq!(state["cursor"][1].as_i64(), Some(expected), "cursor x");
}

fn assert_pixels(case: &CursorCase<'_>, png: &Path) {
    let CursorCase {
        scale, edge, dir, ..
    } = case;
    let pixels = image::open(&png).unwrap().to_rgba8();
    let bounds = |colour: [u8; 3]| {
        let xs: Vec<u32> = pixels
            .enumerate_pixels()
            .filter_map(|(x, _, p)| (p.0[..3] == colour).then_some(x))
            .collect();
        (
            *xs.iter().min().expect("colour present"),
            *xs.iter().max().unwrap() + 1,
        )
    };
    let reference = bounds([255, 0, 255]);
    let cursor = bounds([0, 255, 255]);
    let cell_width = (reference.1 - reference.0) / 8;
    assert!(
        (cursor.1 - cursor.0).abs_diff(cell_width) <= 1,
        "{scale}/{edge}: painted width {} != cell {cell_width}; {dir:?}",
        cursor.1 - cursor.0
    );
    let expected = if *edge == "end" {
        reference.1
    } else {
        reference.0
    };
    assert!(
        cursor.0.abs_diff(expected) <= 1,
        "{scale}/{edge}: painted cursor {} != reference {expected}; {dir:?}",
        cursor.0
    );
}
