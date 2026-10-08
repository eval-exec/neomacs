//! Unselected compact frames retain exclusive bar geometry across mutations.
use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiRunStatus, GuiScenario, GuiTestPlan,
    ProcessGuiCommandRunner,
};
use std::{path::PathBuf, time::Duration};

#[test]
fn unselected_compact_frame_preserves_geometry_after_parameter_and_font_changes() {
    let backend = match std::env::var("NEOMACS_GUI_TEST_BACKEND").as_deref() {
        Ok("x11") => GuiBackend::LinuxX11,
        Ok("wayland") => GuiBackend::LinuxWayland,
        Ok("macos") => GuiBackend::Macos,
        Ok("windows") => GuiBackend::Windows,
        _ => panic!("set NEOMACS_GUI_TEST_BACKEND"),
    };
    let root = neomacs_infra::crate_root!().join("../..");
    let fixture = root.join("crates/neomacs-gui-tests/fixtures/compact-frame-geometry");
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
        GuiScenario::new("compact-frame-geometry", fixture.join("init.el")),
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
    let mut initial_height = None;
    for tag in ["before", "renamed", "font"] {
        let snapshot: serde_json::Value = serde_json::from_slice(
            &std::fs::read(format!(
                "{}.{tag}",
                result.artifacts.frame_snapshot_json.display()
            ))
            .unwrap(),
        )
        .unwrap();
        let frame = &snapshot["frames"][0];
        let bands = frame["frame_chrome"]["bands"].as_array().unwrap();
        assert_eq!(bands.len(), 1, "only the compact band at {tag}: {bands:?}");
        assert_eq!(bands[0]["kind"], "CompactBar");
        let height = bands[0]["bounds"]["height"].as_f64().unwrap();
        if tag == "before" {
            initial_height = Some(height);
        } else if tag == "font" {
            assert_ne!(
                Some(height),
                initial_height,
                "font mutation changes bar metrics"
            );
        }

        assert_eq!(
            frame["window_infos"][0]["bounds"]["y"], bands[0]["bounds"]["height"],
            "compact frame reserves only its rendered band at {tag}"
        );
    }
}
