//! Issue #471: a silent peer must not freeze the evaluator or GUI.
#![cfg(target_os = "linux")]
use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiRunStatus, GuiScenario, GuiTestPlan,
    ProcessGuiCommandRunner,
};
use std::{path::PathBuf, time::Duration};

#[test]
fn stalled_tls_keeps_timers_and_gui_redisplay_alive() {
    let backend = match std::env::var("NEOMACS_GUI_TEST_BACKEND").as_deref() {
        Ok("wayland") => GuiBackend::LinuxWayland,
        Ok("x11") | Err(_) => GuiBackend::LinuxX11,
        Ok(other) => panic!("unsupported backend {other}"),
    };
    let root = neomacs_infra::crate_root!().join("../..");
    let artifacts = root
        .join("target/neomacs-gui-tests")
        .join(format!("stalled-tls-{}", std::process::id()));
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
        GuiScenario::new(
            "stalled-tls",
            root.join("crates/neomacs-gui-tests/fixtures/stalled-tls.el"),
        ),
    )
    .with_program(binary)
    .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "64");
    for (key, value) in session.env() {
        plan = plan.with_env(key.clone(), value.clone());
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(15)),
        )
        .unwrap();
    assert_eq!(result.status, GuiRunStatus::Passed, "{result:#?}");
    assert!(
        !result.timed_out,
        "fixture must finish without the harness watchdog: {result:#?}"
    );
    assert_eq!(result.exit_code, Some(0), "{result:#?}");
    let state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&result.artifacts.gui_state).unwrap()).unwrap();
    assert_eq!(
        state,
        serde_json::json!({"passed": true, "timer": true, "outcome": "timeout", "closed": true})
    );
    let during: serde_json::Value = serde_json::from_slice(
        &std::fs::read(format!(
            "{}.during",
            result.artifacts.frame_snapshot_json.display()
        ))
        .unwrap(),
    )
    .unwrap();
    let text: String = during["frames"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|frame| frame["window_matrices"].as_array().unwrap())
        .flat_map(|window| window["matrix"]["rows"].as_array().unwrap())
        .flat_map(|row| row["glyphs"].as_array().unwrap())
        .flat_map(|area| area.as_array().unwrap())
        .filter_map(|glyph| glyph["glyph_type"]["Char"]["ch"].as_str())
        .collect();
    assert!(
        text.contains("TIMER-DURING-TLS"),
        "timer must be visible before negotiation exits: {text}"
    );
    assert!(
        !text.contains("TLS-TIMEOUT-CLOSED"),
        "snapshot must precede timeout"
    );
    assert!(
        result.png_bytes.unwrap_or_default() > 0,
        "native pixels captured"
    );
}
