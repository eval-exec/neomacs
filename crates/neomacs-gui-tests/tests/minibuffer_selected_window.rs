#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    GuiBackend, GuiRunOptions, GuiRunStatus, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{path::PathBuf, time::Duration};

#[test]
// Prerequisites: fresh-built binary/runtime, Sway, wtype, and DejaVu Sans Mono.
fn completion_survives_the_deleted_invoking_window() {
    let root = neomacs_infra::crate_root!().join("../..");
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let artifacts = root.join(format!(
        "tmp/neomacs-gui-tests/minibuffer-selected-window-{}",
        std::process::id()
    ));
    let backend = GuiBackend::LinuxWayland;
    std::fs::create_dir_all(&artifacts).unwrap();
    let ready = artifacts.join("minibuffer-selected-window.ready");
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
            "minibuffer-selected-window",
            root.join("crates/neomacs-gui-tests/fixtures/minibuffer-selected-window.el"),
        ),
    )
    .with_program(binary)
    .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "10000")
    .with_env("RUST_LOG", "info")
    .with_env("NEOMACS_TEST_COMPLETION_READY", ready.to_string_lossy());
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
            "-k", "Shift_L", "-s", "100", "-d", "100", "Beta", "-k", "Return",
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
    let text = snapshot_text(&snapshot);
    assert!(text.contains("Selected Beta"), "{text}");
    let candidates: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(format!(
            "{}.candidates",
            result.artifacts.frame_snapshot_json.display()
        ))
        .unwrap(),
    )
    .unwrap();
    let text = snapshot_text(&candidates);
    for candidate in ["Alpha", "Beta"] {
        assert!(
            text.lines()
                .any(|line| line.contains(candidate) && line.contains("checked")),
            "{text}"
        );
    }
    let png = image::open(format!(
        "{}.png",
        result.artifacts.frame_snapshot_json.display()
    ))
    .unwrap()
    .to_rgba8();
    // Require visible ink in each accepted candidate character, not just a
    // successfully written PNG or a layout-only snapshot.
    let window = snapshot["frames"][0]["window_matrices"]
        .as_array()
        .unwrap()
        .iter()
        .find(|window| {
            window["matrix"]["rows"][0]["glyphs"][1]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|glyph| glyph["glyph_type"]["Char"]["ch"].as_str())
                .collect::<String>()
                .starts_with("Selected Beta")
        })
        .unwrap();
    let row = &window["matrix"]["rows"][0];
    let mut pen_x = window["text_pixel_bounds"]["x"].as_f64().unwrap();
    let y = window["text_pixel_bounds"]["y"].as_f64().unwrap() + row["pixel_y"].as_f64().unwrap();
    for (index, glyph) in row["glyphs"][1]
        .as_array()
        .unwrap()
        .iter()
        .take(13)
        .enumerate()
    {
        let width = glyph["pixel_width"].as_f64().unwrap();
        if index >= 9 {
            let visible =
                (y as u32..(y + glyph["pixel_height"].as_f64().unwrap()) as u32).any(|py| {
                    (pen_x as u32..(pen_x + width) as u32).any(|px| {
                        let rgba = png.get_pixel(px, py).0;
                        rgba[0] < 80 && rgba[1] < 80 && rgba[2] < 80
                    })
                });
            assert!(visible, "accepted candidate glyph {index} was not painted");
        }
        pen_x += width;
    }
}

fn snapshot_text(snapshot: &serde_json::Value) -> String {
    let mut text = String::new();
    for window in snapshot["frames"][0]["window_matrices"].as_array().unwrap() {
        for row in window["matrix"]["rows"].as_array().unwrap() {
            for glyph in row["glyphs"][1].as_array().unwrap() {
                if let Some(character) = glyph["glyph_type"]["Char"]["ch"].as_str() {
                    text.push_str(character);
                }
            }
            text.push('\n');
        }
    }
    text
}
