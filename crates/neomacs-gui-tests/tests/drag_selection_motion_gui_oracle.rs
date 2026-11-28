//! #852: a click-and-drag delivers pointer motion to Lisp the way GNU does,
//! GNU vs Neomacs on X11.
//!
//! GNU contract: `x_note_mouse_movement` (src/xterm.c:14740-14773) reports a
//! movement only when the pointer leaves the glyph it was last over
//! (`remember_mouse_glyph`, src/xdisp.c:2779, a 1x1 box when
//! `mouse-fine-grained-tracking` is non-nil), and `frame->mouse_moved` is a
//! flag, not a queue, so each read of input sees at most one movement. The
//! number of drag steps Lisp handles is therefore bounded by the glyph cells
//! the pointer crosses, however many motion events the display sends.
//!
//! The stimulus is the same for both editors: the fixture in
//! `wayland-drag-selection.el` (plain Lisp), a press, 300 `xdotool mousemove`
//! steps along a diagonal and a release, on an Xvfb display with the frame
//! focused. The row requires, for both editors, the handled drag steps to stay
//! within the glyph-cell changes along the path (the fixture computes them
//! with `posn-at-x-y`), and the final region to equal GNU's.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant},
};

fn wait_for(path: &Path, deadline: Instant) -> bool {
    while Instant::now() < deadline {
        if path.exists() {
            return true;
        }
        thread::sleep(Duration::from_millis(50));
    }
    false
}

fn xdotool(env: &[(String, String)], args: &[&str]) -> String {
    let output = Command::new("xdotool")
        .args(args)
        .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .output()
        .expect("xdotool on PATH (the CI ecosystem profile installs it)");
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// Drive the drag once the fixture is ready: focus the editor's window, press
/// at (120,40), move in 300 steps to (620,540), release.
fn drive_drag(env: Vec<(String, String)>, ready: PathBuf, pressed: PathBuf) {
    let deadline = Instant::now() + Duration::from_secs(30);
    assert!(wait_for(&ready, deadline), "the fixture never became ready");
    let window = loop {
        let found = xdotool(&env, &["search", "--onlyvisible", "--name", "."]);
        if let Some(id) = found.lines().last() {
            break id.to_owned();
        }
        assert!(Instant::now() < deadline, "no viewable window");
        thread::sleep(Duration::from_millis(50));
    };
    xdotool(&env, &["windowfocus", "--sync", &window]);
    xdotool(&env, &["mousemove", "--window", &window, "110", "40"]);
    xdotool(&env, &["mousemove", "--window", &window, "120", "40"]);
    xdotool(&env, &["mousedown", "1"]);
    assert!(wait_for(&pressed, deadline), "the drag never started");
    for i in 1..=300u32 {
        let x = (120 + 500 * i / 300).to_string();
        let y = (40 + 500 * i / 300).to_string();
        xdotool(&env, &["mousemove", "--window", &window, &x, &y]);
    }
    xdotool(&env, &["mouseup", "1"]);
}

fn observe(root: &Path, artifacts: &Path, binary: &Path, editor: &str) -> Value {
    let fixture = root.join("crates/neomacs-gui-tests/fixtures/wayland-drag-selection.el");
    let _ = fs::remove_dir_all(artifacts);
    fs::create_dir_all(artifacts).unwrap();
    let session = DisplayHarness::for_backend(GuiBackend::LinuxX11)
        .start_session(&artifacts.join("session"))
        .unwrap();
    let ready = artifacts.join("ready.json");
    let pressed = artifacts.join("pressed.json");
    let out = artifacts.join("result.json");
    let mut plan = GuiTestPlan::new(
        GuiBackend::LinuxX11,
        root,
        artifacts,
        GuiScenario::new("drag-selection-motion", &fixture),
    )
    .without_paint_proof()
    .with_program(binary.to_path_buf())
    .with_args([
        "-Q".to_string(),
        "-l".to_string(),
        fixture.display().to_string(),
    ])
    .with_env("NEOMACS_DRAG_TEST_READY", ready.to_string_lossy())
    .with_env("NEOMACS_DRAG_TEST_PRESSED", pressed.to_string_lossy())
    .with_env("NEOMACS_DRAG_TEST_OUT", out.to_string_lossy())
    .with_env("GSETTINGS_BACKEND", "memory")
    .with_env("GDK_BACKEND", "x11");
    for (key, value) in session.env() {
        plan = plan.with_env(key, value);
    }
    let driver = thread::spawn({
        let env = session.env().to_vec();
        let (ready, pressed) = (ready.clone(), pressed.clone());
        move || drive_drag(env, ready, pressed)
    });
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            // Explicit hang-avoidance cap only; the fixture exits after release.
            GuiRunOptions::with_timeout(Duration::from_secs(60)),
        )
        .unwrap();
    driver.join().expect("drag driver");
    let stderr = fs::read_to_string(&result.artifacts.stderr).unwrap_or_default();
    assert!(
        !result.timed_out && result.exit_code == Some(0),
        "{editor}: {result:#?}\nstderr:\n{stderr}"
    );
    serde_json::from_slice(&fs::read(&out).expect("drag result")).unwrap()
}

#[test]
fn drag_motion_reaches_lisp_once_per_glyph_like_gnu_on_x11() {
    let root = neomacs_infra::workspace_root();
    let artifacts = root.join("target/neomacs-gui-tests/drag-selection-motion");
    let oracle = neomacs_test_oracle::gnu::gnu_oracle();
    assert!(
        oracle.is_attested(),
        "set NEOVM_FORCE_ORACLE_PATH to the pinned GUI oracle"
    );
    let neo = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let gnu = observe(
        &root,
        &artifacts.join("gnu"),
        &oracle.executable_path(),
        "gnu",
    );
    let actual = observe(&root, &artifacts.join("neo"), &neo, "neo");

    let bound = |result: &Value| {
        (
            result["move_count"].as_u64().expect("move_count"),
            result["cell_changes"].as_u64().expect("cell_changes"),
        )
    };
    let (gnu_moves, gnu_cells) = bound(&gnu);
    assert!(
        gnu_moves >= 1 && gnu_moves <= gnu_cells,
        "precondition: GNU handles at most one drag step per glyph cell: {gnu}"
    );
    let (neo_moves, neo_cells) = bound(&actual);
    assert!(
        neo_moves >= 1 && neo_moves <= neo_cells,
        "Neomacs handled {neo_moves} drag steps for {neo_cells} glyph-cell changes \
         (GNU {gnu_moves} for {gnu_cells}): {actual}"
    );
    for key in ["final_pt", "final_mk", "region_active"] {
        assert_eq!(actual[key], gnu[key], "{key}: gnu {gnu}\nneo {actual}");
    }
}

/// Motion inside the glyph last reported queues nothing in GNU
/// (`x_note_mouse_movement` returns false), so the wait never returns to
/// `read_char` and never redisplays for it.  A redisplay is owed to a
/// delivered step or to a scroll or timer step, and the fixture's own count
/// of glyph cells crossed bounds the second kind.
#[test]
fn drag_redisplays_only_for_delivered_steps_like_gnu_on_x11() {
    let root = neomacs_infra::workspace_root();
    let artifacts = root.join("target/neomacs-gui-tests/drag-selection-redisplay");
    let oracle = neomacs_test_oracle::gnu::gnu_oracle();
    assert!(
        oracle.is_attested(),
        "set NEOVM_FORCE_ORACLE_PATH to the pinned GUI oracle"
    );
    let neo = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let gnu = observe(
        &root,
        &artifacts.join("gnu"),
        &oracle.executable_path(),
        "gnu",
    );
    let actual = observe(&root, &artifacts.join("neo"), &neo, "neo");

    let counts = |result: &Value| {
        (
            result["redisplay_count"].as_u64().expect("redisplay_count"),
            result["move_count"].as_u64().expect("move_count"),
            result["cell_changes"].as_u64().expect("cell_changes"),
        )
    };
    let (gnu_redisplays, gnu_moves, gnu_cells) = counts(&gnu);
    assert!(
        gnu_redisplays >= gnu_moves && gnu_redisplays <= gnu_moves + gnu_cells,
        "precondition: GNU redisplays for each delivered step plus at most one \
         per glyph cell crossed: {gnu}"
    );
    let (neo_redisplays, neo_moves, neo_cells) = counts(&actual);
    assert!(
        neo_redisplays <= neo_moves + neo_cells,
        "Neomacs redisplayed {neo_redisplays} times for {neo_moves} drag steps and \
         {neo_cells} glyph-cell changes (GNU {gnu_redisplays} for {gnu_moves} and \
         {gnu_cells}): {actual}"
    );
}
