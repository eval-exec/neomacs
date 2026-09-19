#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    GuiBackend, GuiRunOptions, GuiRunStatus, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{path::PathBuf, time::Duration};

#[test]
// Prerequisites: fresh-built binary/runtime, Sway, wtype, and DejaVu Sans Mono.
fn org_region_survives_silent_emphasis_visibility_updates() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let artifacts = root.join(format!(
        "tmp/neomacs-gui-tests/org-selection-{}",
        std::process::id()
    ));
    let backend = GuiBackend::LinuxWayland;
    std::fs::create_dir_all(&artifacts).unwrap();
    let ready = artifacts.join("org-silent-selection.ready");
    let _ = std::fs::remove_file(&ready);
    let session = neomacs_infra::display::start_sway(
        &artifacts,
        "output * resolution 1000x700\nxwayland disable\nseat seat0 fallback true\n",
    )
    .unwrap();
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &artifacts,
        GuiScenario::new(
            "org-silent-selection",
            root.join("crates/neomacs-gui-tests/fixtures/org-silent-selection.el"),
        ),
    )
    .with_program(binary)
    .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "10000")
    .with_env("RUST_LOG", "info")
    .with_env("NEOMACS_SELECTION_READY", ready.to_string_lossy());
    for (key, value) in session.env() {
        plan = plan.with_env(key.clone(), value.clone());
    }
    let env = session.env().to_vec();
    let input = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        while !ready.exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "fixture did not become ready"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        let output = std::process::Command::new(
            std::env::var_os("NEOMACS_GUI_WTYPE").unwrap_or_else(|| "wtype".into()),
        )
        .envs(env)
        .args([
            "-k", "Shift_L", "-s", "50", "-M", "ctrl", "-k", "space", "-k", "n", "-k", "n", "-k",
            "c", "-m", "ctrl", "-k", "t",
        ])
        .output()
        .expect("install wtype");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    });
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(25)),
        )
        .unwrap();
    input.join().unwrap();
    assert!(!result.timed_out, "{result:#?}");
    assert_eq!(result.exit_code, Some(0), "{result:#?}");
    assert_eq!(result.status, GuiRunStatus::Passed, "{result:#?}");
    let snapshot: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&result.artifacts.frame_snapshot_json).unwrap(),
    )
    .unwrap();
    let window = &snapshot["frames"][0]["window_matrices"][0];
    let origin = &window["text_pixel_bounds"];
    let rows = window["matrix"]["rows"].as_array().unwrap();
    let png = image::open(format!(
        "{}.png",
        result.artifacts.frame_snapshot_json.display()
    ))
    .unwrap()
    .to_rgba8();
    let x = origin["x"].as_f64().unwrap() as u32 + 2;
    let y = origin["y"].as_f64().unwrap() as u32 + rows[1]["pixel_y"].as_f64().unwrap() as u32 + 1;
    assert_eq!(
        png.get_pixel(x, y).0,
        [255, 0, 0, 255],
        "selected emphasis line must retain the red region background"
    );
}
