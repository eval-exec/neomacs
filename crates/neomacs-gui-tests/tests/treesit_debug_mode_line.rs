#![cfg(target_os = "linux")]
use neomacs_gui_tests::{
    GuiBackend, GuiRunOptions, GuiRunStatus, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use neomacs_melpa_test_support::{EmacsRuntime, GNU31_TREE_SITTER_GRAMMARS};
use std::{path::PathBuf, time::Duration};

#[test]
fn go_grammar_and_multibyte_mode_line_render_with_debug_logging() {
    let root = neomacs_infra::workspace_root();
    let grammar = GNU31_TREE_SITTER_GRAMMARS
        .iter()
        .find(|case| case.language == "go")
        .unwrap()
        .prepare(&EmacsRuntime::gnu_emacs())
        .unwrap();
    let artifacts = root.join(format!(
        "tmp/neomacs-gui-tests/treesit-debug-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&artifacts).unwrap();
    let ready = artifacts.join("ready");
    let session = neomacs_infra::display::start_sway(
        &artifacts,
        "output * resolution 1800x900\nxwayland disable\nseat seat0 fallback true\n",
    )
    .unwrap();
    let mut plan = GuiTestPlan::new(
        GuiBackend::LinuxWayland,
        &root,
        &artifacts,
        GuiScenario::new(
            "treesit-debug-mode-line",
            root.join("crates/neomacs-gui-tests/fixtures/treesit-debug-mode-line.el"),
        ),
    )
    .with_program(
        std::env::var_os("NEOMACS_GUI_TEST_BINARY")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("target/release/neomacs")),
    )
    .with_env("RUST_LOG", "debug")
    .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "10000")
    .with_env(
        "NEOMACS_TEST_GRAMMAR_USER_DIR",
        grammar.parent().unwrap().to_string_lossy(),
    )
    .with_env("NEOMACS_TEST_TREESIT_READY", ready.to_string_lossy());
    for (key, value) in session.env() {
        plan = plan.with_env(key.clone(), value.clone());
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(30)),
        )
        .unwrap();
    assert!(!result.timed_out, "{result:#?}");
    assert_eq!(result.exit_code, Some(0), "{result:#?}");
    assert_eq!(result.status, GuiRunStatus::Passed, "{result:#?}");
    assert!(ready.exists(), "fixture did not complete");
    let snapshot: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&result.artifacts.frame_snapshot_json).unwrap(),
    )
    .unwrap();
    let mut text = String::new();
    for window in snapshot["frames"][0]["window_matrices"].as_array().unwrap() {
        for row in window["matrix"]["rows"].as_array().unwrap() {
            for glyph in row["glyphs"][1].as_array().unwrap() {
                if let Some(ch) = glyph["glyph_type"]["Char"]["ch"].as_str() {
                    text.push_str(ch);
                }
            }
            text.push('\n');
        }
    }
    assert!(text.contains("Parsed go: source_file"), "{text}");
    assert!(text.contains(&format!("{}…END", "a".repeat(118))), "{text}");
    let image = image::open(format!(
        "{}.png",
        result.artifacts.frame_snapshot_json.display()
    ))
    .unwrap()
    .to_rgba8();
    let mut painted_ellipsis = false;
    for window in snapshot["frames"][0]["window_matrices"].as_array().unwrap() {
        for row in window["matrix"]["rows"].as_array().unwrap() {
            let mut x = window["text_pixel_bounds"]["x"].as_f64().unwrap();
            let y = window["text_pixel_bounds"]["y"].as_f64().unwrap()
                + row["pixel_y"].as_f64().unwrap();
            for glyph in row["glyphs"][1].as_array().unwrap() {
                let width = glyph["pixel_width"].as_f64().unwrap();
                if glyph["glyph_type"]["Char"]["ch"].as_str() == Some("…") {
                    let height = glyph["pixel_height"].as_f64().unwrap();
                    assert!(
                        x >= 0.0
                            && y >= 0.0
                            && x + width <= f64::from(image.width())
                            && y + height <= f64::from(image.height()),
                        "ellipsis outside image"
                    );
                    painted_ellipsis = (y as u32..(y + height) as u32).any(|py| {
                        (x as u32..(x + width) as u32).any(|px| {
                            let pixel = image.get_pixel(px, py).0;
                            pixel[0] < 80 && pixel[1] < 80 && pixel[2] < 80
                        })
                    });
                }
                x += width;
            }
        }
    }
    assert!(painted_ellipsis, "mode-line ellipsis was not painted");
}
