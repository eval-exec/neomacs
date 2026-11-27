//! #890: the native window title follows the frame's `title` and `name`
//! parameters, GNU vs Neomacs.
//!
//! GNU contract: `x_set_name` (src/xfns.c:2309-2349) puts `title` on the
//! native window when it is set and the name otherwise, and an explicit
//! `name` stops `frame-title-format` from renaming the frame. A frame that
//! `x-create-frame` made without a `name` stays implicit (xfns.c:5115-5125).
//! The row records the native title (xdotool, on the editor's own display)
//! with `explicit-name` and `title` after each change, and requires GNU's.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

fn observe(root: &Path, artifacts: &Path, binary: &Path, editor: &str) -> Vec<String> {
    let fixture = root.join("crates/neomacs-gui-tests/fixtures/frame-native-title.el");
    let _ = fs::remove_dir_all(artifacts);
    fs::create_dir_all(artifacts).unwrap();
    let session = DisplayHarness::for_backend(GuiBackend::LinuxX11)
        .start_session(&artifacts.join("session"))
        .unwrap();
    let result_path = artifacts.join("records.txt");
    let mut plan = GuiTestPlan::new(
        GuiBackend::LinuxX11,
        root,
        artifacts,
        GuiScenario::new("frame-native-title", &fixture),
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
    .with_env("GSETTINGS_BACKEND", "memory")
    .with_env("GDK_BACKEND", "x11");
    for (key, value) in session.env() {
        plan = plan.with_env(key, value);
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            // Explicit hang-avoidance cap only; the fixture exits on its own.
            GuiRunOptions::with_timeout(Duration::from_secs(60)),
        )
        .unwrap();
    let stderr = fs::read_to_string(&result.artifacts.stderr).unwrap_or_default();
    let records = fs::read_to_string(&result_path).unwrap_or_default();
    assert!(
        !result.timed_out && result.exit_code == Some(0),
        "{editor}: {result:#?}\nrecords:\n{records}\nstderr:\n{stderr}"
    );
    records
        .lines()
        .filter_map(|line| line.strip_prefix("FRAME-TITLE "))
        .map(str::to_owned)
        .collect()
}

#[test]
fn native_title_follows_title_and_name_like_gnu_on_x11() {
    let root = neomacs_infra::workspace_root();
    let artifacts = root.join("target/neomacs-gui-tests/frame-native-title");
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
    assert_eq!(gnu.len(), 5, "the oracle recorded every step: {gnu:#?}");
    let actual = observe(&root, &artifacts.join("neo"), &neo, "neo");
    assert_eq!(
        actual, gnu,
        "[step explicit-name title native-title] differs from GNU 31.1"
    );
}
