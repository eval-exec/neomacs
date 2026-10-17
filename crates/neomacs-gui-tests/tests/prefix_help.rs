//! Issue #576: prefix help must leave the GUI command loop running.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{path::PathBuf, time::Duration};

/// Check ordinary dispatch, hook state, and presentation for one prefix case.
fn check_prefix_help(shifted: bool) {
    let root = neomacs_infra::workspace_root();
    let artifacts = root.join(format!("tmp/issue-576/gui-{}", std::process::id()));
    let backend = match std::env::var("NEOMACS_GUI_TEST_BACKEND").as_deref() {
        Ok("wayland") => GuiBackend::LinuxWayland,
        _ => GuiBackend::LinuxX11,
    };
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifacts)
        .unwrap();
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &artifacts,
        GuiScenario::new(
            "prefix-help",
            root.join("crates/neomacs-gui-tests/fixtures/prefix-help.el"),
        ),
    )
    .with_program(binary)
    .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "10000");
    if shifted {
        plan = plan.with_env("NEOMACS_PREFIX_HELP_SHIFTED", "1");
    }
    for (key, value) in session.env() {
        plan = plan.with_env(key, value);
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(20)),
        )
        .unwrap();
    assert!(!result.timed_out, "{result:#?}");
    assert_eq!(result.exit_code, Some(0), "{result:#?}");
    let snapshot = std::fs::read_to_string(&result.artifacts.frame_snapshot_txt)
        .expect("the command following prefix help must produce a rendered frame");
    let (keys, flag) = if shifted {
        ("f5 8", "t")
    } else {
        ("3 8", "nil")
    };
    for expected in [
        "prefix-help-session-alive".to_owned(),
        format!("pre describe-prefix-bindings ({keys}) {flag}"),
        format!("post describe-prefix-bindings ({keys}) {flag}"),
    ] {
        assert!(
            snapshot.contains(&expected),
            "missing {expected:?}: {snapshot}"
        );
    }
    let pixels = image::open(&result.artifacts.png).unwrap().to_rgba8();
    let green = pixels
        .pixels()
        .filter(|pixel| {
            let [r, g, b, _] = pixel.0;
            g > 240 && r < 15 && b < 15
        })
        .count();
    assert!(
        green > 3000,
        "the success buffer must reach GPU presentation: {}",
        artifacts.display()
    );
}

/// Prefix help opens its window and permits the next GUI command to run.
#[test]
fn prefix_help_runs_hooks_and_keeps_the_gui_session_alive() {
    check_prefix_help(false);
}

/// Shifted-prefix help publishes its translation state before command hooks.
#[test]
fn shifted_prefix_help_publishes_translation_to_gui_command_hooks() {
    check_prefix_help(true);
}
