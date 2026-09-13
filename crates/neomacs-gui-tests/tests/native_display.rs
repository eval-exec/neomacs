use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{fs, path::PathBuf, time::Duration};

#[test]
#[ignore = "requires a fresh release binary/pdump and a native graphical session"]
fn native_startup_font_and_resize_contract() {
    let root = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
    let backend = if cfg!(target_os = "macos") {
        GuiBackend::Macos
    } else if cfg!(windows) {
        GuiBackend::Windows
    } else {
        GuiBackend::LinuxWayland
    };
    // A unique directory prevents any prior PNG or state file from passing.
    let artifacts = root
        .join("target/neomacs-gui-tests")
        .join(format!("native-display-{}", std::process::id()));
    assert!(!artifacts.exists());
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifacts)
        .unwrap();
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            root.join(if cfg!(windows) {
                "target/release/neomacs.exe"
            } else {
                "target/release/neomacs"
            })
        });
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &artifacts,
        GuiScenario::new(
            "native-display",
            root.join("crates/neomacs-gui-tests/fixtures/native-display-contract.el"),
        ),
    )
    .with_program(binary);
    for (key, value) in session.env() {
        plan = plan.with_env(key, value);
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(25)),
        )
        .unwrap();
    assert!(!result.timed_out, "{result:#?}");
    assert_eq!(result.exit_code, Some(0), "{result:#?}");
    let state: serde_json::Value =
        serde_json::from_slice(&fs::read(&result.artifacts.gui_state).unwrap()).unwrap();
    assert_eq!(state["columns"], 91);
    assert_eq!(state["contract"], "native-display");
    let snapshot: serde_json::Value =
        serde_json::from_slice(&fs::read(&result.artifacts.frame_snapshot_json).unwrap()).unwrap();
    assert!(snapshot.is_object());
    let pixels = image::open(&result.artifacts.png).unwrap().to_rgba8();
    assert!(pixels.width() > 0 && pixels.height() > 0);
    let first = pixels.get_pixel(0, 0);
    assert!(
        pixels.pixels().any(|pixel| pixel != first),
        "GUI readback is blank"
    );
}
