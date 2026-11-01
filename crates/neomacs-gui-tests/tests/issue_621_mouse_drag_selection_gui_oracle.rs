#![cfg(target_os = "linux")]
//! Issue #621 GNU oracle: a real X11 pointer drag must leave the dragged
//! range selected.
//!
//! GNU's drag release is `(drag-mouse-1 START END)`, with START copied from
//! the down event (`src/keyboard.c:6640`, `6739-6741`).  `mouse-set-region`
//! sets mark to START and point to END, so the dragged range survives the
//! transient-map cleanup.  Neomacs emitted `(drag-mouse-1 END)`, which ran
//! the same command but left an empty region.
//!
//! The same XTest drag is injected into GNU and Neomacs on one Xvfb display.
//! Each editor's state is checked against the GNU contract; absolute buffer
//! positions are not compared because the two editors' default font metrics
//! and chrome need not match.

use neomacs_gui_tests::{
    CommandSpec, DisplayHarness, GuiArtifactSet, GuiBackend, GuiCommandRunner, GuiRunOptions,
    GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant};

const FIXTURE: &str = "crates/neomacs-gui-tests/fixtures/issue621-mouse-drag-selection.el";
const SCENARIO: &str = "issue-621-mouse-drag-selection-oracle";
const READY_ENV: &str = "NEOMACS_ISSUE621_READY";
const RESULT_ENV: &str = "NEOMACS_ISSUE621_RESULT";
const DRAG_Y: &str = "30";
const DRAG_XS: [&str; 6] = ["100", "180", "260", "340", "420", "500"];

fn gnu_binary() -> PathBuf {
    std::env::var_os("NEOMACS_GNU_GUI_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("emacs"))
}

fn neomacs_binary(root: &Path) -> PathBuf {
    std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"))
}

fn xdotool(env: &[(String, String)], args: &[&str]) -> Output {
    let output = Command::new("xdotool")
        .args(args)
        .envs(env.iter().map(|(key, value)| (key, value)))
        .output()
        .expect("xdotool must be installed for the X11 GUI drag oracle");
    assert!(
        output.status.success(),
        "xdotool {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn xdotool_allow_failure(env: &[(String, String)], args: &[&str]) {
    let _ = Command::new("xdotool")
        .args(args)
        .envs(env.iter().map(|(key, value)| (key, value)))
        .output();
}

/// Pick the largest visible window.  On a bare Xvfb there is no window
/// manager, so the editor's own window is the meaningful one; tiny helper
/// windows are ignored.
fn find_window(env: &[(String, String)]) -> Option<String> {
    let search = Command::new("xdotool")
        .args(["search", "--onlyvisible", "--name", ".*"])
        .envs(env.iter().map(|(key, value)| (key, value)))
        .output()
        .ok()?;
    if !search.status.success() {
        return None;
    }
    let ids = String::from_utf8_lossy(&search.stdout);
    let mut best: Option<(u64, String)> = None;
    for id in ids.split_whitespace() {
        let Ok(output) = Command::new("xdotool")
            .args(["getwindowgeometry", "--shell", id])
            .envs(env.iter().map(|(key, value)| (key, value)))
            .output()
        else {
            continue;
        };
        if !output.status.success() {
            continue;
        }
        let geometry = String::from_utf8_lossy(&output.stdout);
        let dimension = |name: &str| {
            geometry
                .lines()
                .find_map(|line| line.strip_prefix(name))
                .and_then(|value| value.trim().parse::<u64>().ok())
        };
        let (Some(width), Some(height)) = (dimension("WIDTH="), dimension("HEIGHT=")) else {
            continue;
        };
        if width < 200 || height < 100 {
            continue;
        }
        let area = width * height;
        let is_larger = match &best {
            None => true,
            Some((best_area, _)) => area > *best_area,
        };
        if is_larger {
            best = Some((area, id.to_string()));
        }
    }
    best.map(|(_, id)| id)
}

fn inject_drag(env: &[(String, String)], ready: &Path, label: &str) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !ready.exists() {
        assert!(
            Instant::now() < deadline,
            "{label} drag fixture did not become ready"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    let window = loop {
        if let Some(window) = find_window(env) {
            break window;
        }
        assert!(
            Instant::now() < deadline,
            "{label} did not map a visible window"
        );
        std::thread::sleep(Duration::from_millis(100));
    };
    let window = window.as_str();
    // Without a window manager the frame is not activated by a click; give
    // the window system time to map and focus it before warping the pointer.
    xdotool_allow_failure(env, &["windowfocus", window]);
    xdotool_allow_failure(env, &["windowraise", window]);
    std::thread::sleep(Duration::from_millis(800));
    xdotool(
        env,
        &[
            "mousemove",
            "--sync",
            "--window",
            window,
            DRAG_XS[0],
            DRAG_Y,
        ],
    );
    std::thread::sleep(Duration::from_millis(200));
    xdotool(env, &["mousedown", "1"]);
    std::thread::sleep(Duration::from_millis(200));
    for x in DRAG_XS.iter().skip(1) {
        xdotool(env, &["mousemove", "--sync", "--window", window, x, DRAG_Y]);
        std::thread::sleep(Duration::from_millis(60));
    }
    xdotool(env, &["mouseup", "1"]);
}

#[derive(Debug)]
struct DragState {
    command: String,
    active: String,
    mark: i64,
    point: i64,
    head: String,
    event_len: i64,
    event_start: i64,
    event_end: i64,
}

impl DragState {
    fn parse(label: &str, raw: &str) -> Self {
        let tokens: Vec<&str> = raw
            .trim()
            .trim_start_matches('(')
            .trim_end_matches(')')
            .split_whitespace()
            .collect();
        assert_eq!(
            tokens.len(),
            8,
            "{label} did not record a drag command: {raw:?}"
        );
        let number = |index: usize, field: &str| {
            tokens[index]
                .parse::<i64>()
                .unwrap_or_else(|_| panic!("{label} {field} is not a number: {raw:?}"))
        };
        Self {
            command: tokens[0].to_string(),
            active: tokens[1].to_string(),
            mark: number(2, "mark"),
            point: number(3, "point"),
            head: tokens[4].to_string(),
            event_len: number(5, "event length"),
            event_start: number(6, "event start"),
            event_end: number(7, "event end"),
        }
    }

    /// The GNU contract for a drag release (`src/keyboard.c:6640`,
    /// `6739-6741`) and `mouse-set-region` (`lisp/mouse.el`): the region is
    /// active, the event is `(drag-mouse-1 START END)`, and mark/point are
    /// START/END.
    fn assert_gnu_drag(&self, label: &str, raw: &str) {
        assert_eq!(
            self.command, "mouse-set-region",
            "{label} must run mouse-set-region: {raw:?}"
        );
        assert_eq!(
            self.active, "t",
            "{label} must leave the region active: {raw:?}"
        );
        assert_eq!(
            self.head, "drag-mouse-1",
            "{label} must receive a drag event: {raw:?}"
        );
        assert_eq!(
            self.event_len, 3,
            "{label} drag events are (HEAD START END), not (HEAD END): {raw:?}"
        );
        assert!(
            self.mark < self.point,
            "{label} must leave a non-empty dragged region: {raw:?}"
        );
        assert_eq!(
            self.event_start, self.mark,
            "{label} mouse-set-region takes mark from event-start: {raw:?}"
        );
        assert_eq!(
            self.event_end, self.point,
            "{label} mouse-set-region takes point from event-end: {raw:?}"
        );
    }
}

#[test]
// Prerequisites: requires GNU GUI Emacs, Xvfb, and xdotool.
fn mouse_drag_release_carries_start_and_end_like_gnu() {
    let root = neomacs_infra::crate_root!().join("../..");
    let backend = GuiBackend::LinuxX11;
    let artifact_root = root.join("target/neomacs-gui-tests");
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifact_root)
        .expect("Xvfb display session should start");

    let scenario = GuiScenario::new(SCENARIO, root.join(FIXTURE));
    let artifacts = GuiArtifactSet::new(&artifact_root, backend, SCENARIO);
    let gnu_ready = artifact_root.join(format!("{SCENARIO}.gnu.ready"));
    let gnu_result = artifact_root.join(format!("{SCENARIO}.gnu-result.el"));
    let neo_ready = artifact_root.join(format!("{SCENARIO}.neomacs.ready"));
    let neo_result = artifact_root.join(format!("{SCENARIO}.neomacs-result.el"));
    for path in [&gnu_ready, &gnu_result, &neo_ready, &neo_result] {
        let _ = std::fs::remove_file(path);
    }

    let mut gnu_env = vec![
        (READY_ENV.to_string(), gnu_ready.display().to_string()),
        (RESULT_ENV.to_string(), gnu_result.display().to_string()),
        ("GDK_BACKEND".to_string(), "x11".to_string()),
    ];
    gnu_env.extend(session.env().iter().cloned());
    let gnu_command = CommandSpec {
        program: gnu_binary(),
        args: vec![
            "-Q".to_string(),
            "-l".to_string(),
            root.join(FIXTURE).display().to_string(),
        ],
        env: gnu_env,
    };
    let gnu_input_env = session.env().to_vec();
    let gnu_input_ready = gnu_ready.clone();
    let gnu_input =
        std::thread::spawn(move || inject_drag(&gnu_input_env, &gnu_input_ready, "GNU"));
    let mut runner = ProcessGuiCommandRunner;
    let gnu_output = runner
        .run(
            &gnu_command,
            &artifacts,
            &GuiRunOptions::with_timeout(Duration::from_secs(40)),
        )
        .expect("GNU GUI drag oracle should run");
    gnu_input
        .join()
        .expect("GNU input injection must not panic");
    assert_eq!(
        gnu_output.exit_code,
        Some(0),
        "GNU GUI drag oracle failed; stderr:\n{}",
        gnu_output.stderr
    );

    let binary = neomacs_binary(&root);
    assert!(
        binary.exists(),
        "build {binary:?} before running the GUI drag oracle"
    );
    let mut plan = GuiTestPlan::new(backend, &root, &artifact_root, scenario)
        .with_program(binary)
        .with_env(READY_ENV, neo_ready.display().to_string())
        .with_env(RESULT_ENV, neo_result.display().to_string())
        .with_env("GDK_BACKEND", "x11");
    for (key, value) in session.env() {
        plan = plan.with_env(key.clone(), value.clone());
    }
    let neo_input_env = session.env().to_vec();
    let neo_input_ready = neo_ready.clone();
    let neo_input =
        std::thread::spawn(move || inject_drag(&neo_input_env, &neo_input_ready, "Neomacs"));
    let result = plan
        .run_with(
            &mut runner,
            GuiRunOptions::with_timeout(Duration::from_secs(40)),
        )
        .expect("Neomacs GUI drag oracle should run");
    neo_input
        .join()
        .expect("Neomacs input injection must not panic");
    assert_eq!(
        result.exit_code,
        Some(0),
        "Neomacs GUI drag oracle failed; stderr:\n{}",
        std::fs::read_to_string(&result.artifacts.stderr)
            .unwrap_or_else(|_| "<missing stderr>".to_string())
    );

    let gnu_state = std::fs::read_to_string(&gnu_result)
        .unwrap_or_else(|error| panic!("GNU did not write {gnu_result:?}: {error}"));
    let neo_state = std::fs::read_to_string(&neo_result)
        .unwrap_or_else(|error| panic!("Neomacs did not write {neo_result:?}: {error}"));

    let gnu_drag = DragState::parse("GNU", &gnu_state);
    gnu_drag.assert_gnu_drag("GNU", &gnu_state);
    let neo_drag = DragState::parse("Neomacs", &neo_state);
    neo_drag.assert_gnu_drag("Neomacs", &neo_state);
}
