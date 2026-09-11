use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiRunStatus, GuiScenario, GuiTestPlan,
    ProcessGuiCommandRunner,
};
use std::{path::PathBuf, time::Duration};

#[test]
#[ignore = "requires a built binary/pdump and a native GUI backend"]
fn fitting_chromeless_child_before_redisplay_keeps_one_text_line() {
    let backend = match std::env::var("NEOMACS_GUI_TEST_BACKEND").as_deref() {
        Ok("wayland") => GuiBackend::LinuxWayland,
        Ok("x11") => GuiBackend::LinuxX11,
        Ok("macos") => GuiBackend::Macos,
        Ok("windows") => GuiBackend::Windows,
        _ => panic!("set NEOMACS_GUI_TEST_BACKEND"),
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
            "child-frame-sizing",
            root.join("crates/neomacs-gui-tests/fixtures/child-frame-sizing.el"),
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
    assert!(!result.timed_out);
    let snapshot: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&result.artifacts.frame_snapshot_json).unwrap(),
    )
    .unwrap();
    let child = snapshot["frames"]
        .as_array()
        .unwrap()
        .iter()
        .find(|frame| {
            frame["window_infos"]
                .as_array()
                .unwrap()
                .iter()
                .any(|w| w["buffer_name"] == "*child-sizing*")
        })
        .expect("child in redisplay output");
    let text: String = child["window_matrices"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|matrix| matrix["matrix"]["rows"].as_array().unwrap())
        .flat_map(|row| row["glyphs"].as_array().unwrap())
        .flat_map(|area| area.as_array().unwrap())
        .filter_map(|glyph| glyph["glyph_type"]["Char"]["ch"].as_str())
        .collect();
    assert_eq!(text, "TestABC", "fitting must preserve the displayed text");
    assert_eq!(
        child["frame_rows"], 1,
        "one-line buffer must fit to one text line"
    );
    assert_eq!(
        child["frame_pixel_height"].as_f64().unwrap(),
        child["char_height"].as_f64().unwrap() + 6.0,
        "only the two three-pixel borders surround the text line"
    );
}
