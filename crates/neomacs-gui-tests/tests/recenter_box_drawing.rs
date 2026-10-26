use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiRunStatus, GuiScenario, GuiTestPlan,
    ProcessGuiCommandRunner,
};
use serde_json::Value;
use std::{path::PathBuf, time::Duration};

#[test]
fn third_recenter_keeps_point_fully_visible_at_bottom_with_taller_rows() {
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
            "recenter-box-drawing",
            root.join("crates/neomacs-gui-tests/fixtures/recenter-box-drawing.el"),
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
    let state: Value =
        serde_json::from_slice(&std::fs::read(&result.artifacts.gui_state).unwrap()).unwrap();
    assert_eq!(state["point"], state["original_point"], "{state:#}");
    assert_eq!(
        state["trace"],
        serde_json::json!(["middle", "top", "bottom"])
    );

    // Read the real rendered row and text viewport. Canonical frame line
    // counts and a glyph's ink bitmap are not the row's layout geometry.
    let snapshot: Value =
        serde_json::from_slice(&std::fs::read(&result.artifacts.frame_snapshot_json).unwrap())
            .unwrap();
    let frame = &snapshot["frames"][0];
    let cursor = &frame["phys_cursor"];
    let window = frame["window_matrices"]
        .as_array()
        .unwrap()
        .iter()
        .find(|window| window["window_id"] == cursor["window_id"])
        .expect("cursor's rendered window");
    assert_eq!(
        cursor["charpos"].as_u64().unwrap() + 1,
        state["point"].as_u64().unwrap(),
        "the rendered cursor must still represent point"
    );
    let row = &window["matrix"]["rows"][cursor["row"].as_u64().unwrap() as usize];
    assert_eq!(row["role"], "Text");
    let viewport_bottom = window["text_clip_bounds"]["y"].as_f64().unwrap()
        + window["text_clip_bounds"]["height"].as_f64().unwrap();
    let row_top = window["pixel_bounds"]["y"].as_f64().unwrap() + row["pixel_y"].as_f64().unwrap();
    let row_height = row["height_px"].as_f64().unwrap();
    // GNU does not reserve the point line's trailing extra spacing at bottom.
    let text_height = row_height - row["line_spacing_px"].as_f64().unwrap();
    let gap = viewport_bottom - row_top - text_height;
    assert!(
        gap >= -0.5 && gap < row_height + 0.5,
        "point's row must fit fully at bottom: gap={gap}, row={row:#}, viewport={:#}",
        window["text_clip_bounds"]
    );
    let text = std::fs::read_to_string(&result.artifacts.frame_snapshot_txt).unwrap();
    assert!(text.contains("│├└"), "{text}");
}
