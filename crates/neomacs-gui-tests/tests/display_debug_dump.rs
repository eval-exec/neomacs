//! Issue #855: live Wayland `neomacs-dump-display-state` bundle contract.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use serde_json::Value;
use std::{fs, path::PathBuf, time::Duration};

#[test]
fn wayland_display_debug_dump_captures_last_presented_and_frame_snapshot() {
    let root = neomacs_infra::workspace_root();
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let artifacts = root.join(format!("tmp/display-debug-dump-{}", std::process::id()));
    fs::create_dir_all(&artifacts).unwrap();
    let dump_path = artifacts.join("live-wayland-dump.json");

    let session = DisplayHarness::for_backend(GuiBackend::LinuxWayland)
        .start_session(&artifacts)
        .unwrap();

    let mut plan = GuiTestPlan::new(
        GuiBackend::LinuxWayland,
        &root,
        &artifacts,
        GuiScenario::new(
            "display-debug-dump",
            root.join("crates/neomacs-gui-tests/fixtures/display-debug-dump.el"),
        ),
    )
    .without_paint_proof()
    .with_program(binary)
    .with_env("NEOMACS_DEBUG_DISPLAY_DUMP", dump_path.to_string_lossy())
    .with_env("TMPDIR", root.join("tmp").to_string_lossy())
    .with_env("RUST_LOG", "warn,neomacs::cursor_geometry=debug");
    for (key, value) in session.env() {
        plan = plan.with_env(key, value);
    }

    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(25)),
        )
        .unwrap();
    let stderr = fs::read_to_string(&result.artifacts.stderr).unwrap_or_default();
    assert!(!result.timed_out, "{result:#?}\n{stderr}");
    assert_eq!(result.exit_code, Some(0), "{result:#?}\n{stderr}");
    assert!(stderr.contains("DISPLAY-DEBUG-PASS"), "{stderr}");

    let raw = fs::read_to_string(&dump_path)
        .unwrap_or_else(|e| panic!("failed reading {}: {e}\n{stderr}", dump_path.display()));
    let doc: Value = serde_json::from_str(&raw).expect("dump must be valid JSON");

    assert_eq!(doc["schema_version"].as_u64(), Some(1));
    assert!(
        doc["render_capture_error"].is_null(),
        "live GUI dump should capture render thread cleanly: {raw}"
    );
    assert!(
        doc["frame_snapshot_error"].is_null(),
        "live GUI dump should capture frame_snapshot cleanly: {raw}"
    );
    assert_eq!(doc["version"]["window_system"].as_str(), Some("neo"));
    assert_eq!(doc["cursor_geometry_capture_enabled"].as_bool(), Some(true));
    assert!(doc["cursor_geometry_log"].is_array());

    let frames = doc["last_presented"]
        .as_array()
        .expect("last_presented array");
    assert!(!frames.is_empty(), "expected at least one presented frame");
    let primary = frames
        .iter()
        .find(|f| f["is_primary"].as_bool() == Some(true))
        .expect("primary presented frame");
    assert!(
        primary["frame_seq"].as_u64().unwrap_or(0) >= 1,
        "frame must have been presented to swapchain: {primary}"
    );
    assert_ne!(
        primary["render_path"].as_str(),
        Some("not_yet_presented"),
        "primary frame must report a real presented render_path: {primary}"
    );
    assert!(primary["scale_factor"].as_f64().unwrap_or(0.0) > 0.0);
    assert!(primary["surface_px"][0].as_u64().unwrap_or(0) > 0);
    assert_eq!(primary["has_active_cursor"].as_bool(), Some(true));
    assert_eq!(primary["blink_enabled"].as_bool(), Some(true));
    let cursor_visible = primary["cursor_visible"]
        .as_bool()
        .expect("cursor_visible boolean");
    assert_eq!(
        primary["inverse_video_active"].as_bool(),
        Some(cursor_visible),
        "filled-box cursor inverse_video_active must match cursor_visible: {primary}"
    );
    assert_eq!(
        doc["diagnostics_flags"]["cursor_hidden_or_blinked_off"].as_bool(),
        Some(!cursor_visible),
        "cursor_hidden_or_blinked_off must reflect cursor_visible: {doc}"
    );

    let snapshot_text = doc["frame_snapshot"]["text"]
        .as_array()
        .expect("frame_snapshot.text array");
    assert!(
        snapshot_text.iter().any(|t| t
            .as_str()
            .is_some_and(|s| s.contains("line 2 gamma cursor-here"))),
        "frame_snapshot.text must contain the visible buffer text: {snapshot_text:?}"
    );
    assert!(
        doc["frame_snapshot"]["geometry"]["frames"]
            .as_array()
            .is_some_and(|f| !f.is_empty())
    );
    assert_eq!(
        doc["diagnostics_flags"]["presentation_lagging"].as_bool(),
        Some(false)
    );
}
