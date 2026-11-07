use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiRunStatus, GuiScenario, GuiTestPlan,
    ProcessGuiCommandRunner,
};
use std::{path::PathBuf, time::Duration};

#[test]
fn printer_quit_is_caught_before_returning_to_interactive_evaluation() {
    let backend = match std::env::var("NEOMACS_GUI_TEST_BACKEND").as_deref() {
        Ok("wayland") => GuiBackend::LinuxWayland,
        Ok("x11") => GuiBackend::LinuxX11,
        Ok("macos") => GuiBackend::Macos,
        Ok("windows") => GuiBackend::Windows,
        _ => panic!("set NEOMACS_GUI_TEST_BACKEND"),
    };
    let root = neomacs_infra::workspace_root();
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let artifacts = root.join("target/neomacs-gui-tests");
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifacts)
        .unwrap();
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &artifacts,
        GuiScenario::new(
            "printer-quit",
            root.join("crates/neomacs-gui-tests/fixtures/printer-quit.el"),
        ),
    )
    .with_program(binary)
    .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "32");
    for (key, value) in session.env() {
        plan = plan.with_env(key.clone(), value.clone());
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(30)),
        )
        .unwrap();
    assert_eq!(result.status, GuiRunStatus::Passed, "{result:#?}");
    assert!(
        !result.timed_out,
        "printer quit must return control to the GUI: {result:#?}"
    );
    let snapshot = std::fs::read_to_string(&result.artifacts.frame_snapshot_json).unwrap();
    assert!(snapshot.contains("*printer-quit*"), "{snapshot}");
    let text = std::fs::read_to_string(&result.artifacts.frame_snapshot_txt).unwrap();
    assert!(text.contains("(caught inside printer)"), "{text}");
    assert!(text.contains("(t 42)"), "{text}");
}
