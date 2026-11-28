//! #861: Partially visible point row taller than the window's text area,
//! GNU Emacs 31.1 vs Neomacs on X11 and Wayland.
//!
//! GNU contracts (src/xdisp.c at pinned commit a360712c9d):
//! - `cursor_row_fully_visible_p` (:19329-19336): returns true (no scroll) for
//!   a row taller than `window_box_height` only when `!force_p`, minibuffer,
//!   non-zero `vscroll`, or `cursor.vpos == 0`.
//! - `try_cursor_movement` (:20098-20118): for a partially visible tall row on
//!   cursor-only motion, sets `*scroll_step = true` (`temp_scroll_step`) and
//!   returns `CURSOR_MOVEMENT_MUST_SCROLL`.
//! - `try_scrolling` (:19380-19704) + `redisplay_window` (:21087-21244): runs
//!   with `scroll_max` derived from `scroll-conservatively`, `scroll-step`, or
//!   `temp_scroll_step`, stops after `extra > 1` (`force_p = extra <= 1`), and
//!   falls through to `recenter:` (`window_box_height / 2`) when `dy > scroll_max`.
//! - `window-cursor-info` still reports the physical cursor on a tall visible
//!   row rather than dropping it as clipped.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

const CASES: usize = 14;

#[derive(Clone, Debug, PartialEq, Eq)]
struct TallRowRecord {
    case: String,
    start_line: i64,
    point_line: i64,
    posn_row: Option<i64>,
    cursor_info_row: Option<i64>,
}

fn parse_records(output: &str) -> Vec<TallRowRecord> {
    output
        .lines()
        .filter_map(|line| line.strip_prefix("TALL-POINT-ROW "))
        .map(|record| {
            let fields: Vec<&str> = record.split_whitespace().collect();
            let [
                case,
                start_line,
                point_line,
                _body_px,
                line_height,
                posn_y,
                ci_y,
            ] = fields[..]
            else {
                panic!("malformed record: {record}");
            };
            let number = |text: &str| -> i64 {
                text.parse()
                    .unwrap_or_else(|_| panic!("{text:?} is not an integer in {record}"))
            };
            let opt_row = |text: &str, lh: i64| -> Option<i64> {
                if text == "nil" {
                    None
                } else {
                    Some(number(text).div_euclid(lh))
                }
            };
            let lh = number(line_height);
            assert!(lh > 0, "positive line height in {record}");
            TallRowRecord {
                case: case.to_owned(),
                start_line: number(start_line),
                point_line: number(point_line),
                posn_row: opt_row(posn_y, lh),
                cursor_info_row: opt_row(ci_y, lh),
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
) -> Vec<TallRowRecord> {
    let fixture = root.join("crates/neomacs-gui-tests/fixtures/tall-point-row-scroll.el");
    let _ = fs::remove_dir_all(artifacts);
    fs::create_dir_all(artifacts).unwrap();
    let result_path = artifacts.join("probe-result.txt");
    let mut plan = GuiTestPlan::new(
        backend,
        root,
        artifacts,
        GuiScenario::new("tall-point-row-scroll", &fixture),
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
    assert_eq!(records.len(), CASES, "expected {CASES} cases\n{context}");
    records
}

fn tall_point_row_scroll_matches_gnu(neo_backend: GuiBackend) {
    let root = neomacs_infra::workspace_root();
    let artifacts = root
        .join("target/neomacs-gui-tests")
        .join("tall-point-row-scroll")
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

    let diverging: Vec<String> = gnu
        .iter()
        .zip(&actual)
        .filter(|(g, n)| g != n)
        .map(|(g, n)| format!("  gnu {g:?}\n  neo {n:?}"))
        .collect();
    assert!(
        diverging.is_empty(),
        "{} of {} #861 cases differ from GNU 31.1 on {}:\n{}",
        diverging.len(),
        gnu.len(),
        neo_backend.slug(),
        diverging.join("\n")
    );
}

#[test]
fn tall_point_row_scroll_matches_gnu_on_x11() {
    tall_point_row_scroll_matches_gnu(GuiBackend::LinuxX11);
}

#[test]
fn tall_point_row_scroll_matches_gnu_on_wayland() {
    tall_point_row_scroll_matches_gnu(GuiBackend::LinuxWayland);
}
