//! Issue #519: frame-local requests survive real GUI redisplay.
use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiRunStatus, GuiScenario, GuiTestPlan,
    ProcessGuiCommandRunner,
};
use std::{path::PathBuf, time::Duration};

#[test]
fn frame_bar_parameters_survive_startup_and_gui_redisplay() {
    let backend = match std::env::var("NEOMACS_GUI_TEST_BACKEND").as_deref() {
        Ok("x11") => GuiBackend::LinuxX11,
        Ok("wayland") => GuiBackend::LinuxWayland,
        Ok("macos") => GuiBackend::Macos,
        Ok("windows") => GuiBackend::Windows,
        _ => panic!("set NEOMACS_GUI_TEST_BACKEND"),
    };
    let root = neomacs_infra::crate_root!().join("../..");
    let fixture = root.join("crates/neomacs-gui-tests/fixtures/frame-bar-parameters");
    let artifacts = root.join("target/neomacs-gui-tests");
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
        GuiScenario::new("frame-bar-parameters", fixture.join("init.el")),
    )
    .with_program(binary)
    .with_args([format!("--init-directory={}", fixture.display())]);
    for (key, value) in session.env() {
        plan = plan.with_env(key.clone(), value.clone());
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(25)),
        )
        .unwrap();
    assert_eq!(result.status, GuiRunStatus::Passed, "{result:#?}");
    let states: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&result.artifacts.gui_state).unwrap()).unwrap();
    for (index, (tag, menu, mode)) in [
        ("setup", 0, true),
        ("idle", 0, true),
        ("shown", 1, true),
        ("hidden", 0, true),
        ("local-enable", 1, false),
        ("global-enable", 1, true),
        ("global-disable", 0, false),
    ]
    .into_iter()
    .enumerate()
    {
        let state = &states[index];
        assert_eq!(state["tag"], tag);
        assert_eq!(state["menu"], menu, "{state}");
        assert_eq!(state["tool"], 0, "{state}");
        assert_eq!(state["mode"], mode, "{state}");
        if menu == 0 {
            assert_eq!(state["top"], 0, "hidden frame bar reserves no row: {state}");
        } else {
            assert!(state["top"].as_u64().unwrap() > 0, "{state}");
        }
        let snapshot: serde_json::Value = serde_json::from_slice(
            &std::fs::read(format!(
                "{}.{tag}",
                result.artifacts.frame_snapshot_json.display()
            ))
            .unwrap(),
        )
        .unwrap();
        assert!(
            snapshot["frames"]
                .as_array()
                .is_some_and(|frames| !frames.is_empty()),
            "{snapshot}"
        );
        let frame = &snapshot["frames"][0];
        let bands = frame["frame_chrome"]["bands"].as_array().unwrap();
        assert_eq!(
            bands.iter().any(|band| band["kind"] == "MenuBar"),
            menu > 0,
            "rendered menu visibility at {tag}"
        );
        assert!(!bands.iter().any(|band| band["kind"] == "ToolBar"));
        let root_top = frame["window_infos"][0]["bounds"]["y"].as_f64().unwrap();
        if menu == 0 {
            assert_eq!(root_top, 0.0, "rendered root geometry at {tag}");
        } else {
            assert!(root_top > 0.0, "rendered root geometry at {tag}");
        }
    }
    assert!(
        result.png_bytes.unwrap_or_default() > 0,
        "GUI painted pixels"
    );
}
