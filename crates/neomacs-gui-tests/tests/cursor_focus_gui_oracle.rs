//! #882 B1: the cursor of a frame with and without the display's focus, GNU
//! vs Neomacs.
//!
//! GNU contract: `get_window_cursor_type` (src/xdisp.c:34670) treats a
//! window as non-selected when it is not its frame's selected window OR its
//! frame is not the display's `highlight_frame`; with
//! `cursor-in-non-selected-windows` t a box becomes hollow and a bar one
//! pixel narrower (xdisp.c:34702-34712). GNU starts with no highlight frame
//! and sets it from a FocusIn.
//!
//! The test display has no window manager, so frames are unfocused unless the
//! fixture focuses them with `xdotool windowfocus`. The rows run Neomacs on
//! X11 only: focus is an X input here, and the GNU reference is X11.
//! - `window-cursor-info` (Lisp-visible) on a bar, unfocused and then after
//!   the frame gains focus: the focus change alone must reach the cursor.
//! - Painted cursor on an unfocused frame (a hollow box).
//! - Painted cursors on a focused frame split in two: the selected window's
//!   box and the other window's hollow box.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiRunStatus, GuiScenario, GuiTestPlan,
    ProcessGuiCommandRunner,
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

/// What one run observed: its `CURSOR-FOCUS` records and, for paint
/// scenarios, the red cursor pixels (count and bounding box).
#[derive(Debug, PartialEq, Eq)]
struct Observation {
    records: Vec<String>,
    red: Option<(usize, (u32, u32))>,
}

fn red_pixels(png: &Path, editor: &str) -> (usize, (u32, u32)) {
    let image = image::open(png)
        .unwrap_or_else(|error| panic!("{editor}: frame image {}: {error}", png.display()))
        .to_rgb8();
    let red: Vec<(u32, u32)> = image
        .enumerate_pixels()
        .filter(|(_, _, pixel)| pixel.0 == [0xff, 0x00, 0x00])
        .map(|(x, y, _)| (x, y))
        .collect();
    assert!(
        !red.is_empty(),
        "{editor}: no red cursor in {}",
        png.display()
    );
    let span = |coordinate: fn(&(u32, u32)) -> u32| {
        let values = red.iter().map(coordinate);
        values.clone().max().unwrap() - values.min().unwrap() + 1
    };
    (red.len(), (span(|(x, _)| *x), span(|(_, y)| *y)))
}

fn observe(
    root: &Path,
    artifacts: &Path,
    binary: &Path,
    editor: &str,
    scenario: &str,
) -> Observation {
    let fixture = root.join("crates/neomacs-gui-tests/fixtures/cursor-focus.el");
    let _ = fs::remove_dir_all(artifacts);
    fs::create_dir_all(artifacts).unwrap();
    let session = DisplayHarness::for_backend(GuiBackend::LinuxX11)
        .start_session(&artifacts.join("session"))
        .unwrap();
    let result_path = artifacts.join("records.txt");
    let paints = scenario != "focus-change";
    let mut plan = GuiTestPlan::new(
        GuiBackend::LinuxX11,
        root,
        artifacts,
        GuiScenario::new("cursor-focus", &fixture),
    )
    .with_program(binary.to_path_buf())
    .with_args([
        "-Q".to_string(),
        "-l".to_string(),
        fixture.display().to_string(),
    ])
    .with_env("NEOMACS_CF_SCENARIO", scenario)
    .with_env(
        "NEOMACS_COMMIT_RESULT_FILE",
        result_path.to_string_lossy().into_owned(),
    )
    .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "10000")
    .with_env("GSETTINGS_BACKEND", "memory")
    .with_env("GDK_BACKEND", "x11");
    if !paints {
        plan = plan.without_paint_proof();
    }
    for (key, value) in session.env() {
        plan = plan.with_env(key, value);
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            // Explicit hang-avoidance cap only; the fixture exits on its own.
            GuiRunOptions::with_timeout(Duration::from_secs(40)),
        )
        .unwrap();
    let stderr = fs::read_to_string(&result.artifacts.stderr).unwrap_or_default();
    let records = fs::read_to_string(&result_path).unwrap_or_default();
    assert!(
        !result.timed_out && result.exit_code == Some(0),
        "{editor} {scenario}: {result:#?}\nrecords:\n{records}\nstderr:\n{stderr}"
    );
    assert_eq!(result.status, GuiRunStatus::Passed, "{editor}: {result:#?}");
    Observation {
        records: records
            .lines()
            .filter_map(|line| line.strip_prefix("CURSOR-FOCUS "))
            .map(str::to_owned)
            .collect(),
        red: paints.then(|| red_pixels(&result.artifacts.png, editor)),
    }
}

fn matches_gnu(scenario: &str) -> (Observation, Observation) {
    let root = neomacs_infra::workspace_root();
    let artifacts = root
        .join("target/neomacs-gui-tests")
        .join("cursor-focus")
        .join(scenario);
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
        scenario,
    );
    let actual = observe(&root, &artifacts.join("neo"), &neo, "neo", scenario);
    (gnu, actual)
}

/// `window-cursor-info` on a bar: GNU reports W 1 while the frame is
/// unfocused and 2 once it has focus. The second record is taken after a
/// focus change alone, so it also checks the redraw on focus change.
#[test]
fn cursor_info_follows_frame_focus_like_gnu_on_x11() {
    let (gnu, neo) = matches_gnu("focus-change");
    assert_eq!(gnu.records.len(), 2, "{gnu:?}");
    assert_eq!(
        neo.records, gnu.records,
        "(frame-focus-state) and (window-cursor-info) differ from GNU 31.1"
    );
}

/// The unfocused arm: the selected window's box cursor is hollow.
#[test]
fn cursor_on_an_unfocused_frame_matches_gnu_on_x11() {
    let (gnu, neo) = matches_gnu("unfocused");
    assert_eq!(
        gnu.records,
        ["unfocused nil"],
        "precondition: GNU is unfocused"
    );
    assert_eq!(neo.records, gnu.records);
    assert_eq!(
        neo.red, gnu.red,
        "red cursor pixels (count, bbox) on an unfocused frame differ from GNU 31.1"
    );
}

/// The window arm on a focused frame: the selected window draws its box, the
/// other window of the split draws a hollow box.
#[test]
fn cursors_of_a_focused_split_frame_match_gnu_on_x11() {
    let (gnu, neo) = matches_gnu("split");
    assert_eq!(gnu.records, ["split t"], "precondition: GNU is focused");
    assert_eq!(neo.records, gnu.records);
    assert_eq!(
        neo.red, gnu.red,
        "red cursor pixels (count, bbox) on a focused split frame differ from GNU 31.1"
    );
}
