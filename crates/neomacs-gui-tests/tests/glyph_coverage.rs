#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    GuiBackend, GuiRunOptions, GuiRunStatus, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{path::PathBuf, time::Duration};

#[test]
// Prerequisites: fresh-built binary/runtime, Sway, wtype, and DejaVu Sans Mono.
fn grayscale_coverage_is_complementary_on_light_and_dark_backgrounds() {
    check_coverage("none");
}

#[test]
fn subpixel_coverage_is_complementary_on_light_and_dark_backgrounds() {
    check_coverage("rgb");
}

fn check_coverage(order: &str) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let artifacts = root.join(format!(
        "tmp/neomacs-gui-tests/glyph-coverage-{order}-{}",
        std::process::id()
    ));
    let backend = GuiBackend::LinuxWayland;
    std::fs::create_dir_all(&artifacts).unwrap();
    let fontconfig = artifacts.join("fonts.conf");
    let inherited =
        std::env::var("FONTCONFIG_FILE").unwrap_or_else(|_| "/etc/fonts/fonts.conf".to_owned());
    let inherited = inherited.replace('&', "&amp;").replace('<', "&lt;");
    std::fs::write(&fontconfig, format!(
        "<fontconfig><include>{inherited}</include><match target=\"pattern\"><edit name=\"rgba\" mode=\"assign\"><const>{order}</const></edit></match></fontconfig>"
    )).unwrap();
    let ready = artifacts.join("glyph-coverage.ready");
    let _ = std::fs::remove_file(&ready);
    let session = neomacs_infra::display::start_sway(
        &artifacts,
        "output * resolution 3840x2160\nxwayland disable\nseat seat0 fallback true\n",
    )
    .unwrap();
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &artifacts,
        GuiScenario::new(
            "glyph-coverage",
            root.join("crates/neomacs-gui-tests/fixtures/glyph-coverage.el"),
        ),
    )
    .with_program(binary)
    .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "10000")
    .with_env("RUST_LOG", "info")
    .with_env("FONTCONFIG_FILE", fontconfig.to_string_lossy())
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
            "-d", "100", "-k", "Shift_L", "-s", "300", "-M", "ctrl", "-k", "l", "-k", "c", "-m",
            "ctrl", "-k", "t",
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
    assert!(
        png.width() >= 3800 && png.height() >= 2100,
        "capture must use the 4K output"
    );
    let x = origin["x"].as_f64().unwrap() as u32;
    let y = origin["y"].as_f64().unwrap() as u32;
    // Identical masks on opposite backgrounds must have complementary
    // encoded pixels. Check both monochrome and colored foregrounds, with
    // the same face colors carried through the render pipeline. The first 200px stay inside each text run.
    let mut edge_pixels = 0;
    let mut maximum_error = 0;
    let mut lcd_pixels = 0;
    for first in [0, 2] {
        let y0 = y + rows[first]["pixel_y"].as_f64().unwrap() as u32;
        let y1 = y + rows[first + 1]["pixel_y"].as_f64().unwrap() as u32;
        let height = rows[first]["height_px"].as_f64().unwrap() as u32;
        assert_eq!(rows[first]["height_px"], rows[first + 1]["height_px"]);
        for dy in 0..height {
            for dx in 0..200 {
                let light = png.get_pixel(x + dx, y0 + dy);
                let dark = png.get_pixel(x + dx, y1 + dy);
                if first == 0 && light[0].abs_diff(light[2]) > 2 {
                    lcd_pixels += 1;
                }
                for channel in 0..3 {
                    let value = light[channel];
                    let low = if first == 0 { 0 } else { [32, 64, 96][channel] };
                    if value > low + 8 && value < 255 - low - 8 {
                        edge_pixels += 1;
                        maximum_error =
                            maximum_error.max((value as i32 + dark[channel] as i32 - 255).abs());
                    }
                }
            }
        }
    }
    if order == "rgb" {
        assert!(lcd_pixels > 100, "must exercise per-channel LCD coverage");
    } else {
        assert_eq!(lcd_pixels, 0, "grayscale masks must remain achromatic");
    }
    assert!(edge_pixels > 100, "must exercise antialiased glyph edges");
    assert!(
        maximum_error <= 2,
        "light/dark coverage differs by {maximum_error}/255 over {edge_pixels} edge samples"
    );
}
