//! Issue #559: a local let binding completes text in a live GUI minibuffer.
#![cfg(target_os = "linux")]
use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiRunStatus, GuiScenario, GuiTestPlan,
    ProcessGuiCommandRunner,
};
use std::{fs, path::PathBuf, time::Duration};

#[test]
fn local_case_completion_selects_and_renders_the_candidate() {
    let root = neomacs_infra::workspace_root();
    let backend = match std::env::var("NEOMACS_GUI_TEST_BACKEND").as_deref() {
        Ok("wayland") => GuiBackend::LinuxWayland,
        _ => GuiBackend::LinuxX11,
    };
    let artifacts = root.join(format!(
        "target/neomacs-gui-tests/completion-{}",
        std::process::id()
    ));
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
            "buffer-local-completion",
            root.join("crates/neomacs-gui-tests/fixtures/buffer-local-completion.el"),
        ),
    )
    .with_program(binary);
    for (key, value) in session.env() {
        plan = plan.with_env(key, value);
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            // Allow editor startup; the Lisp fixture has its own eight-second watchdog.
            GuiRunOptions::with_timeout(Duration::from_secs(30)),
        )
        .unwrap();
    assert_eq!(result.status, GuiRunStatus::Passed, "{result:#?}");
    assert_eq!(result.exit_code, Some(0));
    let state: serde_json::Value =
        serde_json::from_slice(&fs::read(&result.artifacts.gui_state).unwrap()).unwrap();
    assert_eq!(
        state,
        serde_json::json!({"passed": true, "selected": "*Messages*"})
    );
    let snapshot = fs::read_to_string(&result.artifacts.frame_snapshot_txt).unwrap();
    assert!(snapshot.contains("Selected: *Messages*"), "{snapshot}");
}
