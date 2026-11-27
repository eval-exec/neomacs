//! Frame text height after no sizing, a pixelwise and a character
//! `set-frame-size`, GNU vs Neomacs.
//!
//! GNU contract (frame.c at the pinned commit):
//! - `frame-text-height` is `FRAME_TEXT_HEIGHT`, the frame's text area
//!   (`Fframe_text_height`). The root and minibuffer windows share that area,
//!   so their pixel heights add up to it.
//! - With no size requested, a GUI frame's text area is 36 lines
//!   (`gui_figure_window_size`: `text_height = 36 * FRAME_LINE_HEIGHT (f)`).
//! - `set-frame-size` passes HEIGHT, in pixels when PIXELWISE and otherwise
//!   lines times `FRAME_LINE_HEIGHT`, to `adjust_frame_size` as the new text
//!   height (`Fset_frame_size`, `check_frame_pixels`).
//!
//! Each row records `frame-char-height`, `frame-text-height`, `frame-height`,
//! the root and minibuffer window heights and the first window's
//! `window-body-height` after each sizing has held still for 1 s, and
//! requires Neomacs's to equal GNU's. Two shapes, two rows:
//! - settled: no size requested, then a pixelwise and a character
//!   `set-frame-size` on the settled frame;
//! - startup: a pixelwise `set-frame-size` while the `-l` file loads, before
//!   the first frame has settled (#864). GNU gives the requested text height
//!   there too.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

/// `(SHAPE CHAR-HEIGHT TEXT-HEIGHT FRAME-HEIGHT ROOT MINI BODY)` per sizing.
fn parse(output: &str) -> Vec<(String, [i64; 6])> {
    output
        .lines()
        .filter_map(|line| line.strip_prefix("FRAME-TEXT-HEIGHT "))
        .map(|record| {
            let mut fields = record.split_whitespace();
            let shape = fields.next().expect("shape").to_owned();
            let numbers: Vec<i64> = fields
                .map(|field| {
                    field
                        .parse()
                        .unwrap_or_else(|_| panic!("{field:?} in {record}"))
                })
                .collect();
            let numbers: [i64; 6] = numbers
                .try_into()
                .unwrap_or_else(|_| panic!("six numbers in {record}"));
            (shape, numbers)
        })
        .collect()
}

fn observe(
    root: &Path,
    artifacts: &Path,
    backend: GuiBackend,
    binary: &Path,
    session_env: &[(String, String)],
    scenario: &str,
) -> Vec<(String, [i64; 6])> {
    let fixture = root.join("crates/neomacs-gui-tests/fixtures/frame-text-height.el");
    let _ = fs::remove_dir_all(artifacts);
    fs::create_dir_all(artifacts).unwrap();
    let result_path = artifacts.join("probe-result.txt");
    let mut plan = GuiTestPlan::new(
        backend,
        root,
        artifacts,
        GuiScenario::new("frame-text-height", &fixture),
    )
    .without_paint_proof()
    .with_program(binary.to_path_buf())
    .with_args([
        "-Q".to_string(),
        "-l".to_string(),
        fixture.display().to_string(),
    ])
    .with_env(
        "NEOMACS_COMMIT_RESULT_FILE",
        result_path.to_string_lossy().into_owned(),
    )
    .with_env("NEOMACS_FTH_SCENARIO", scenario)
    .with_env("GSETTINGS_BACKEND", "memory");
    if backend == GuiBackend::LinuxX11 {
        plan = plan.with_env("GDK_BACKEND", "x11");
    }
    for (key, value) in session_env {
        plan = plan.with_env(key, value);
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            // Explicit hang-avoidance cap only; the fixture exits once the
            // third sizing has been recorded.
            GuiRunOptions::with_timeout(Duration::from_secs(90)),
        )
        .unwrap();
    let stderr = fs::read_to_string(&result.artifacts.stderr).unwrap_or_default();
    let output = fs::read_to_string(&result_path).unwrap_or_default();
    let context = format!("{result:#?}\noutput:\n{output}\nstderr:\n{stderr}");
    assert!(!result.timed_out, "{context}");
    assert_eq!(result.exit_code, Some(0), "{context}");
    assert!(output.contains("COMMIT-EDITING-PASS"), "{context}");
    parse(&output)
}

fn frame_text_height_matches_gnu(
    neo_backend: GuiBackend,
    scenario: &str,
    expected_shapes: &[&str],
) {
    let root = neomacs_infra::workspace_root();
    let artifacts = root
        .join("target/neomacs-gui-tests")
        .join("frame-text-height")
        .join(neo_backend.slug())
        .join(scenario);
    let oracle = neomacs_test_oracle::gnu::gnu_oracle();
    assert!(
        oracle.is_attested(),
        "set NEOVM_FORCE_ORACLE_PATH to the pinned GUI oracle"
    );
    let neo = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));

    let gnu_session = DisplayHarness::for_backend(GuiBackend::LinuxX11)
        .start_session(&artifacts.join("gnu-session"))
        .unwrap();
    let gnu = observe(
        &root,
        &artifacts.join("gnu"),
        GuiBackend::LinuxX11,
        &oracle.executable_path(),
        &gnu_session.env(),
        scenario,
    );
    drop(gnu_session);
    let neo_session = DisplayHarness::for_backend(neo_backend)
        .start_session(&artifacts.join("neo-session"))
        .unwrap();
    let actual = observe(
        &root,
        &artifacts.join("neo"),
        neo_backend,
        &neo,
        &neo_session.env(),
        scenario,
    );
    let shapes = |records: &[(String, [i64; 6])]| -> Vec<String> {
        records.iter().map(|(shape, _)| shape.clone()).collect()
    };
    assert_eq!(
        shapes(&gnu),
        expected_shapes,
        "the oracle recorded every sizing"
    );
    assert_eq!(
        shapes(&actual),
        shapes(&gnu),
        "Neomacs recorded every sizing"
    );

    for (shape, [_, text, _, root_window, mini, _]) in &gnu {
        assert_eq!(
            root_window + mini,
            *text,
            "GNU's root and minibuffer windows fill its text area ({shape})"
        );
    }
    assert_eq!(
        gnu[0].1[0], actual[0].1[0],
        "precondition: both editors use the same line height"
    );
    let diverging: Vec<String> = gnu
        .iter()
        .zip(&actual)
        .filter(|(gnu, neo)| gnu != neo)
        .map(|(gnu, neo)| {
            format!(
                "  {}: gnu [char text lines root mini body] {:?}\n  {}: neo {:?}",
                gnu.0, gnu.1, neo.0, neo.1
            )
        })
        .collect();
    assert!(
        diverging.is_empty(),
        "frame text geometry differs from GNU 31.1:\n{}",
        diverging.join("\n")
    );
}

#[test]
fn frame_text_height_matches_gnu_on_x11() {
    frame_text_height_matches_gnu(GuiBackend::LinuxX11, "settled", SETTLED);
}

#[test]
fn frame_text_height_matches_gnu_on_wayland() {
    frame_text_height_matches_gnu(GuiBackend::LinuxWayland, "settled", SETTLED);
}

/// No size requested, then a pixelwise and a character `set-frame-size` on
/// the settled frame.
const SETTLED: &[&str] = &["default", "pixelwise", "chars"];

#[test]
fn frame_size_requested_at_startup_matches_gnu_on_x11() {
    frame_text_height_matches_gnu(GuiBackend::LinuxX11, "startup", STARTUP);
}

#[test]
fn frame_size_requested_at_startup_matches_gnu_on_wayland() {
    frame_text_height_matches_gnu(GuiBackend::LinuxWayland, "startup", STARTUP);
}

/// A pixelwise `set-frame-size` while the `-l` file loads (#864).
const STARTUP: &[&str] = &["startup-pixelwise"];
