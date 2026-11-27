//! #856: a GUI tab bar whose `tab-bar-format` is nil takes no space, GNU vs
//! Neomacs.
//!
//! GNU contract (xdisp.c at the pinned commit): `redisplay_tab_bar` builds
//! the desired tab-bar string and measures it with `tab_bar_height`. An empty
//! format measures 0 px, and when the new height is below the tab-bar window's
//! current one (`auto-resize-tab-bars` is t, not `grow-only`) it calls
//! `change_tab_bar_height_hook` with it. So the reporter's `my/tab-bar-hide`
//! (`tab-bar-format` nil plus `tab-bar--update-tab-bar-lines`) gives the row
//! back to the windows and leaves no blank line.
//!
//! The stimulus is identical on both editors: `tab-bar-mode` on, hide, a
//! one-item format, then the original format, each ending as a command would
//! (`force-mode-line-update`) and recorded after its geometry has held still
//! for 1 s. Per step the row compares `tab-bar-lines`, whether
//! `(tab-bar-height nil t)` is 0, and whether the root window's pixel top is
//! 0. Those do not depend on the frame's default height or on the bar's
//! measured height, which is a separate divergence.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

/// `STEP TAB-BAR-LINES TAB-BAR-HEIGHT TEXT-HEIGHT ROOT-TOP ROOT-HEIGHT BODY-LINES`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Step {
    step: String,
    tab_bar_lines: i64,
    tab_bar_height: i64,
    root_top: i64,
}

fn parse(output: &str) -> Vec<Step> {
    output
        .lines()
        .filter_map(|line| line.strip_prefix("TAB-BAR-HIDE "))
        .map(|record| {
            let fields: Vec<&str> = record.split_whitespace().collect();
            let [step, lines, height, _text, top, _root_height, _body] = fields[..] else {
                panic!("malformed record {record}");
            };
            let number = |text: &str| -> i64 {
                text.parse()
                    .unwrap_or_else(|_| panic!("{text:?} is not a number in {record}"))
            };
            Step {
                step: step.to_owned(),
                tab_bar_lines: number(lines),
                tab_bar_height: number(height),
                root_top: number(top),
            }
        })
        .collect()
}

/// The compared view, per step: `tab-bar-lines`, whether the bar takes no
/// height, and whether the root window starts at the top of the text area.
/// The measured height of a non-empty bar is a separate divergence (22 px on
/// GNU, 20/18 on Neomacs) with its own issue, so it is deliberately not
/// compared here; a hidden bar must be exactly 0 px with the root at 0.
fn compared(steps: &[Step]) -> Vec<(String, i64, bool, bool)> {
    steps
        .iter()
        .map(|step| {
            (
                step.step.clone(),
                step.tab_bar_lines,
                step.tab_bar_height == 0,
                step.root_top == 0,
            )
        })
        .collect()
}

fn observe(
    root: &Path,
    artifacts: &Path,
    backend: GuiBackend,
    binary: &Path,
    session_env: &[(String, String)],
) -> (Vec<Step>, String) {
    let fixture = root.join("crates/neomacs-gui-tests/fixtures/tab-bar-hide.el");
    let _ = fs::remove_dir_all(artifacts);
    fs::create_dir_all(artifacts).unwrap();
    let result_path = artifacts.join("probe-result.txt");
    let mut plan = GuiTestPlan::new(
        backend,
        root,
        artifacts,
        GuiScenario::new("tab-bar-hide", &fixture),
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
            // Explicit hang-avoidance cap only; the fixture exits once the
            // third step has been recorded.
            GuiRunOptions::with_timeout(Duration::from_secs(90)),
        )
        .unwrap();
    let stderr = fs::read_to_string(&result.artifacts.stderr).unwrap_or_default();
    let output = fs::read_to_string(&result_path).unwrap_or_default();
    let context = format!("{result:#?}\noutput:\n{output}\nstderr:\n{stderr}");
    assert!(!result.timed_out, "{context}");
    assert_eq!(result.exit_code, Some(0), "{context}");
    assert!(output.contains("COMMIT-EDITING-PASS"), "{context}");
    let steps = parse(&output);
    let names: Vec<&str> = steps.iter().map(|step| step.step.as_str()).collect();
    assert_eq!(
        names,
        ["shown", "hidden", "one-item", "reshown"],
        "{context}"
    );
    (steps, output)
}

fn tab_bar_hide_matches_gnu(neo_backend: GuiBackend) {
    let root = neomacs_infra::workspace_root();
    let artifacts = root
        .join("target/neomacs-gui-tests")
        .join("tab-bar-hide")
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
    let (gnu, gnu_output) = observe(
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
    let (actual, neo_output) = observe(
        &root,
        &artifacts.join("neo"),
        neo_backend,
        &neo,
        &neo_session.env(),
    );

    assert_eq!(
        compared(&actual),
        compared(&gnu),
        "tab-bar geometry differs from GNU 31.1 \
         [step tab-bar-lines no-height root-at-top]\n\
         gnu:\n{gnu_output}\nneo:\n{neo_output}"
    );
}

#[test]
fn tab_bar_hide_matches_gnu_on_x11() {
    tab_bar_hide_matches_gnu(GuiBackend::LinuxX11);
}

#[test]
fn tab_bar_hide_matches_gnu_on_wayland() {
    tab_bar_hide_matches_gnu(GuiBackend::LinuxWayland);
}
