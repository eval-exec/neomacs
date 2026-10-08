use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiRunStatus, GuiScenario, GuiTestPlan,
    ProcessGuiCommandRunner,
};
use std::{path::PathBuf, time::Duration};

#[test]
fn integer_width_overflow_returns_to_interactive_evaluation() {
    let backend = match std::env::var("NEOMACS_GUI_TEST_BACKEND").as_deref() {
        Ok("wayland") => GuiBackend::LinuxWayland,
        Ok("x11") => GuiBackend::LinuxX11,
        Ok("macos") => GuiBackend::Macos,
        Ok("windows") => GuiBackend::Windows,
        _ => panic!("set NEOMACS_GUI_TEST_BACKEND"),
    };
    let root = neomacs_infra::crate_root!().join("../..");
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
            "integer-width",
            root.join("crates/neomacs-gui-tests/fixtures/integer-width.el"),
        ),
    )
    .with_program(binary)
    // Keep readbacks enabled through the post-overflow redisplay, so the PNG
    // records recovery as well as initial native startup.
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
        "the recovery probe must finish: {result:#?}"
    );
    let snapshot = std::fs::read_to_string(&result.artifacts.frame_snapshot_json).unwrap();
    assert!(snapshot.contains("*integer-width*"), "{snapshot}");
    let text = std::fs::read_to_string(&result.artifacts.frame_snapshot_txt).unwrap();
    assert!(text.contains("(overflow-error)"), "{text}");
    assert!(text.contains("(65536 42)"), "{text}");
}
