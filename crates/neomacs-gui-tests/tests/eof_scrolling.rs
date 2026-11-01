//! The EOF scroll exception needs real graphical frames in both editors.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    CommandSpec, DisplayHarness, GuiArtifactSet, GuiBackend, GuiCommandRunner, GuiRunOptions,
    GuiRunStatus, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{fs, path::PathBuf, time::Duration};

#[test]
fn final_row_scrolling_matches_pinned_gnu_gui() {
    let root = neomacs_infra::workspace_root();
    let artifacts = root.join(format!(
        "target/neomacs-gui-tests/eof-{}",
        std::process::id()
    ));
    let fixture = root.join("crates/neomacs-gui-tests/fixtures/eof-scrolling.el");
    let gnu_session = DisplayHarness::Xvfb
        .start_session(artifacts.join("gnu"))
        .expect("start GNU X11 display");
    let gnu_state = artifacts.join("gnu-state.json");
    let mut command = CommandSpec {
        program: neomacs_test_oracle::gnu::gnu_oracle().executable_path(),
        args: vec!["-Q".into(), "--load".into(), fixture.display().to_string()],
        env: vec![
            ("GDK_BACKEND".into(), "x11".into()),
            ("GSETTINGS_BACKEND".into(), "memory".into()),
            (
                "NEOMACS_GUI_STATE_JSON".into(),
                gnu_state.display().to_string(),
            ),
        ],
    };
    command.env.extend(gnu_session.env().iter().cloned());
    let output = ProcessGuiCommandRunner
        .run(
            &command,
            &GuiArtifactSet::new(&artifacts, GuiBackend::LinuxX11, "gnu"),
            &GuiRunOptions::with_timeout(Duration::from_secs(30)),
        )
        .expect("run GNU EOF fixture");
    assert!(!output.timed_out, "GNU: {output:#?}");
    assert_eq!(output.exit_code, Some(0), "GNU: {output:#?}");
    let gnu: serde_json::Value =
        serde_json::from_slice(&fs::read(gnu_state).expect("GNU state")).expect("GNU JSON");
    assert_eq!(
        gnu,
        serde_json::json!([
            ["end-of-buffer", 5, 5, 0],
            ["moved", 5, 6, true, "end-of-buffer"],
            ["moved", 5, 6, true, "end-of-buffer"],
            [[5, 6, true], 5, 6, 32]
        ])
    );

    let session = DisplayHarness::Xvfb
        .start_session(artifacts.join("native"))
        .expect("start native X11 display");
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let mut plan = GuiTestPlan::new(
        GuiBackend::LinuxX11,
        &root,
        &artifacts,
        GuiScenario::new("eof-scrolling", &fixture),
    )
    .with_program(binary)
    .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "64");
    for (key, value) in session.env() {
        plan = plan.with_env(key, value);
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(30)),
        )
        .expect("run native EOF fixture");
    assert_eq!(result.status, GuiRunStatus::Passed, "{result:#?}");
    assert_eq!(result.exit_code, Some(0), "{result:#?}");
    let native: serde_json::Value =
        serde_json::from_slice(&fs::read(&result.artifacts.gui_state).expect("native state"))
            .expect("native JSON");
    assert_eq!(native, gnu, "graphical EOF scrolling must match pinned GNU");
}
