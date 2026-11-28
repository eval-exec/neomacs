//! #873: header-line height before redisplay, center recenter placement, and
//! 1.4 mode-line/header-line pixel height, GNU 31.1 vs Neomacs.
//!
//! GNU contracts (pinned commit a360712c9d):
//! - `WINDOW_HEADER_LINE_HEIGHT` (window.h:1030-1033) and `window_box_height`
//!   (xdisp.c:1307-1370) check live `window_wants_header_line` before reading
//!   the previous redisplay's matrix height, falling back to
//!   `estimate_mode_line_height` when no height has been measured yet.
//! - `Frecenter` (window.c:7309) calls `move_it_vertically_backward` with
//!   `window_box_height(w) / 2`, which only steps back an extra row when the
//!   shortfall exceeds `min(window_box_height(w), line_height * 2 / 3)`
//!   (xdisp.c:11386-11393).
//! - `merge_face_heights` (xfaces.c:2253-2270) multiplies a relative float
//!   `:height` against the inherited height in tenths of a point (truncating
//!   to integer) before `POINT_TO_PIXEL` converts to pixel size.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone, Debug, PartialEq, Eq)]
struct PhaseRecord {
    phase: String,
    char_height: i64,
    mode_line_height: i64,
    header_line_height: i64,
    body_lines: i64,
    body_pixels: i64,
    start_line: i64,
}

fn parse_records(output: &str) -> Vec<PhaseRecord> {
    output
        .lines()
        .filter_map(|line| line.strip_prefix("HEADER-LINE-RECENTER "))
        .map(|record| {
            let fields: Vec<&str> = record.split_whitespace().collect();
            let [
                phase,
                char_height,
                mode_line_height,
                header_line_height,
                body_lines,
                body_pixels,
                start_line,
            ] = fields[..]
            else {
                panic!("malformed record: {record}");
            };
            let number = |text: &str| -> i64 {
                text.parse()
                    .unwrap_or_else(|_| panic!("{text:?} is not an integer in {record}"))
            };
            PhaseRecord {
                phase: phase.to_owned(),
                char_height: number(char_height),
                mode_line_height: number(mode_line_height),
                header_line_height: number(header_line_height),
                body_lines: number(body_lines),
                body_pixels: number(body_pixels),
                start_line: number(start_line),
            }
        })
        .collect()
}

fn observe(
    root: &Path,
    artifacts: &Path,
    backend: GuiBackend,
    binary: &Path,
    session_env: &[(String, String)],
) -> Vec<PhaseRecord> {
    let fixture = root.join("crates/neomacs-gui-tests/fixtures/header-line-recenter.el");
    let _ = fs::remove_dir_all(artifacts);
    fs::create_dir_all(artifacts).unwrap();
    let result_path = artifacts.join("probe-result.txt");
    let mut plan = GuiTestPlan::new(
        backend,
        root,
        artifacts,
        GuiScenario::new("header-line-recenter", &fixture),
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
            // Hang-avoidance cap only; the fixture exits synchronously from
            // `window-setup-hook`.
            GuiRunOptions::with_timeout(Duration::from_secs(60)),
        )
        .unwrap();
    let stderr = fs::read_to_string(&result.artifacts.stderr).unwrap_or_default();
    let output = fs::read_to_string(&result_path).unwrap_or_default();
    let context = format!("{result:#?}\noutput:\n{output}\nstderr:\n{stderr}");
    assert!(!result.timed_out, "{context}");
    assert_eq!(result.exit_code, Some(0), "{context}");
    assert!(output.contains("COMMIT-EDITING-PASS"), "{context}");
    let records = parse_records(&output);
    assert_eq!(records.len(), 6, "expected six phases\n{context}");
    records
}

fn header_line_recenter_matches_gnu(neo_backend: GuiBackend) {
    let root = neomacs_infra::workspace_root();
    let artifacts = root
        .join("target/neomacs-gui-tests")
        .join("header-line-recenter")
        .join(neo_backend.slug());
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
    );

    assert_eq!(
        gnu[0].char_height, actual[0].char_height,
        "precondition: both editors use the same default line height"
    );
    let diverging: Vec<String> = gnu
        .iter()
        .zip(&actual)
        .filter(|(g, n)| g != n)
        .map(|(g, n)| format!("  gnu {g:?}\n  neo {n:?}"))
        .collect();
    assert!(
        diverging.is_empty(),
        "{} of {} #873 phases differ from GNU 31.1:\n{}",
        diverging.len(),
        gnu.len(),
        diverging.join("\n")
    );
}

#[test]
fn header_line_recenter_and_mode_line_height_match_gnu_on_x11() {
    header_line_recenter_matches_gnu(GuiBackend::LinuxX11);
}
