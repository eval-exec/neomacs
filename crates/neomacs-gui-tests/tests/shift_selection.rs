#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    GuiBackend, GuiRunOptions, GuiRunStatus, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{path::PathBuf, time::Duration};

#[test]
// Prerequisites: fresh-built binary/runtime, Sway, wtype, and DejaVu Sans Mono.
fn holding_shift_across_right_keys_extends_the_visible_region() {
    let root = neomacs_infra::crate_root!().join("../..");
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let artifacts = root.join(format!(
        "tmp/neomacs-gui-tests/shift-selection-{}",
        std::process::id()
    ));
    let backend = GuiBackend::LinuxWayland;
    std::fs::create_dir_all(&artifacts).unwrap();
    let ready = artifacts.join("shift-selection.ready");
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
            "shift-selection",
            root.join("crates/neomacs-gui-tests/fixtures/shift-selection.el"),
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
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
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
            "-k", "Shift_L", "-s", "100", "-M", "shift", "-k", "Right", "-s", "100", "-k", "Right",
            "-s", "100", "-k", "Right", "-s", "100", "-k", "Right", "-m", "shift",
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
            GuiRunOptions::with_timeout(Duration::from_secs(40)),
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
    let y = origin["y"].as_f64().unwrap() as u32 + rows[0]["pixel_y"].as_f64().unwrap() as u32 + 1;
    let glyphs = rows[0]["glyphs"][1].as_array().unwrap();
    assert!(glyphs.len() >= 8, "missing source text glyphs");
    let mut pen_x = origin["x"].as_f64().unwrap();
    for (index, glyph) in glyphs.iter().take(8).enumerate() {
        let x = pen_x as u32 + 1;
        pen_x += glyph["pixel_width"].as_f64().unwrap();
        let selected = png.get_pixel(x, y).0 == [255, 0, 0, 255];
        assert_eq!(
            selected,
            index < 4,
            "region background at character {index}"
        );
    }
}
