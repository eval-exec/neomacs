use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiRunStatus, GuiScenario, GuiTestPlan,
    ProcessGuiCommandRunner,
};
use std::{path::PathBuf, time::Duration};

#[test]
#[ignore = "requires a built binary/pdump and a native GUI backend"]
fn child_frame_signed_absolute_parameters_reach_rendered_placement() {
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
            "child-frame-position",
            root.join("crates/neomacs-gui-tests/fixtures/child-frame-position.el"),
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
    assert!(
        !result.timed_out,
        "fixture must finish, not merely paint once"
    );
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
                .any(|window| window["buffer_name"] == "*child-position-popup*")
        })
        .expect("visible child frame in redisplay output");
    assert!(!child["frame_placement"]["parent"].is_null());
    let bounds = &child["frame_placement"]["outer_in_parent"];
    assert_eq!(
        (bounds["x"].as_f64(), bounds["y"].as_f64()),
        (Some(248.0), Some(545.0))
    );
    let frames = snapshot["frames"].as_array().unwrap();
    let parent = frames
        .iter()
        .find(|f| f["frame_placement"]["parent"].is_null())
        .unwrap();
    let parent_width = parent["frame_pixel_width"].as_f64().unwrap();
    let parent_height = parent["frame_pixel_height"].as_f64().unwrap();
    for name in [
        "created-absolute",
        "created-far",
        "created-fraction",
        "resized",
        "negative",
    ] {
        let buffer = format!("*child-position-{name}*");
        let child = frames
            .iter()
            .find(|frame| {
                frame["window_infos"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|w| w["buffer_name"] == buffer)
            })
            .expect("scenario child in redisplay output");
        let bounds = &child["frame_placement"]["outer_in_parent"];
        let remaining_x = parent_width - bounds["width"].as_f64().unwrap();
        let remaining_y = parent_height - bounds["height"].as_f64().unwrap();
        let expected = match name {
            "created-absolute" => (-12.0, 37.0),
            "created-far" => (remaining_x - 10.0, remaining_y - 20.0),
            "created-fraction" => (
                (remaining_x.max(0.0) * 0.5).trunc(),
                (remaining_y.max(0.0) * 0.25).trunc(),
            ),
            "resized" => (remaining_x - 10.0, remaining_y),
            "negative" => (-12.0, -17.0),
            _ => unreachable!(),
        };
        assert_eq!(
            (bounds["x"].as_f64().unwrap(), bounds["y"].as_f64().unwrap()),
            expected,
            "{name}"
        );
    }
}
