//! Explicitly opt-in native GUI smoke; does not substitute for AppKit visual QA.
use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiRunStatus, GuiScenario, GuiTestPlan,
    ProcessGuiCommandRunner,
};
use std::{path::PathBuf, time::Duration};

#[test]
#[ignore = "requires a built binary/pdump and the requested native GUI backend"]
fn native_chrome_theme_change_keeps_both_frames_alive() {
    let backend = match std::env::var("NEOMACS_GUI_TEST_BACKEND").as_deref() {
        Ok("wayland") => GuiBackend::LinuxWayland,
        Ok("macos") => GuiBackend::Macos,
        Ok("windows") => GuiBackend::Windows,
        Ok("x11") => GuiBackend::LinuxX11,
        _ => panic!("set NEOMACS_GUI_TEST_BACKEND to the native backend"),
    };
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
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
            "window-chrome",
            root.join("crates/neomacs-gui-tests/fixtures/window-chrome.el"),
        ),
    )
    .with_program(binary);
    for (key, value) in session.env() {
        plan = plan.with_env(key.clone(), value.clone());
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(20)),
        )
        .unwrap();
    assert_eq!(result.status, GuiRunStatus::Passed, "{result:#?}");
    let state: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&result.artifacts.gui_state).unwrap())
            .unwrap();
    assert_eq!(state["primary_alive"], true);
    assert_eq!(state["secondary_alive"], true);
    assert!(
        result.png_bytes.unwrap_or_default() > 0,
        "native surface readback artifact"
    );
}
