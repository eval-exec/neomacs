#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    DisplayHarness, GuiArtifactSet, GuiBackend, GuiRunOptions, GuiRunStatus, GuiScenario,
    GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{path::PathBuf, time::Duration};

#[test]
// Prerequisites: requires a built binary/pdump, Weston, and DejaVu Sans Mono.
fn org_selection_and_empty_line_cursor_preserve_spacing_and_backgrounds() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let artifacts = root.join(format!(
        "tmp/neomacs-gui-tests/org-line-spacing-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let backend = GuiBackend::LinuxWayland;
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifacts)
        .unwrap();
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &artifacts,
        GuiScenario::new(
            "org-line-spacing",
            root.join("crates/neomacs-gui-tests/fixtures/org-line-spacing.el"),
        ),
    )
    .with_program(binary)
    .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "10000");
    for (key, value) in session.env() {
        plan = plan.with_env(key.clone(), value.clone());
    }
    let captures = GuiArtifactSet::new(&artifacts, backend, "org-line-spacing");
    let capture = std::thread::spawn(move || capture_stages(captures));
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(40)),
        )
        .unwrap();
    capture.join().expect("capture driver panicked").unwrap();
    assert!(!result.timed_out, "{result:#?}");
    assert_eq!(result.exit_code, Some(0), "{result:#?}");
    assert_eq!(result.status, GuiRunStatus::Passed, "{result:#?}");
    let mut background_colors = Vec::new();
    for stage in ["before", "selected", "empty-cursor"] {
        let path = format!("{}.{stage}", result.artifacts.frame_snapshot_json.display());
        let snapshot: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let window = &snapshot["frames"][0]["window_matrices"][0];
        let rows: Vec<_> = window["matrix"]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["enabled"] == true && row["mode_line"] == false)
            .take(11)
            .collect();
        assert_eq!(rows.len(), 11);
        // GNU oracle, same font/configuration: 24px font + trunc(24 * .14).
        for (index, row) in rows.iter().enumerate() {
            assert_eq!(row["height_px"], 27.0, "{stage}, row {index}");
            assert_eq!(row["pixel_y"], (index * 27) as f64, "{stage}, row {index}");
        }
        // Sample beyond the text: selection and :extend backgrounds must
        // cover every pixel, including the spacing below the blank Org row.
        let png = image::open(format!("{path}.png")).unwrap().to_rgba8();
        let origin = &window["text_pixel_bounds"];
        let x = origin["x"].as_f64().unwrap() as u32 + 500;
        let y = origin["y"].as_f64().unwrap() as u32;
        let color = png.get_pixel(x, y + 4 * 27 + 5);
        background_colors.push(color.0);
        assert_ne!(
            color.0,
            [255, 255, 255, 255],
            "fixture background must be colored"
        );
        for offset in (4 * 27)..(7 * 27) {
            assert_eq!(
                png.get_pixel(x, y + offset),
                color,
                "{stage}: background gap at text y={offset}"
            );
        }
    }
    assert_ne!(
        background_colors[0], background_colors[1],
        "selection must visibly paint the region"
    );
    assert_eq!(
        background_colors[0], background_colors[2],
        "deactivating the region restores the Org background"
    );
}

/// Each stage has a distinct expected paint transition. Reading a complete PNG
/// with that background, rather than waiting a fixed duration, acknowledges the
/// stage. The unchanged assertions above still check every spacing pixel.
#[derive(Clone, Copy, Debug)]
enum CaptureStage {
    Before,
    Selected,
    EmptyCursor,
}

impl CaptureStage {
    fn name(self) -> &'static str {
        match self {
            Self::Before => "before",
            Self::Selected => "selected",
            Self::EmptyCursor => "empty-cursor",
        }
    }

    fn background(self) -> [u8; 4] {
        match self {
            Self::Before | Self::EmptyCursor => [237, 237, 237, 255],
            Self::Selected => [255, 0, 0, 255],
        }
    }
}

fn capture_stages(artifacts: GuiArtifactSet) -> Result<(), String> {
    for stage in [
        CaptureStage::Before,
        CaptureStage::Selected,
        CaptureStage::EmptyCursor,
    ] {
        let path = format!(
            "{}.{}",
            artifacts.frame_snapshot_json.display(),
            stage.name()
        );
        let deadline = std::time::Instant::now() + Duration::from_secs(12);
        loop {
            if std::time::Instant::now() >= deadline {
                return Err(format!(
                    "{stage:?}: no complete PNG with {:?} background; artifacts: {}",
                    stage.background(),
                    artifacts.png.display()
                ));
            }
            // Layout and PNG files can be observed while their writers are
            // still working. Retry incomplete reads and save the exact bytes
            // that we validated, rather than reopening a changing file.
            if let Ok(snapshot) = std::fs::read(&path)
                && let Ok(snapshot) = serde_json::from_slice::<serde_json::Value>(&snapshot)
                && let Ok(bytes) = std::fs::read(&artifacts.png)
                && let Ok(png) = image::load_from_memory(&bytes)
            {
                let origin = &snapshot["frames"][0]["window_matrices"][0]["text_pixel_bounds"];
                if let (Some(x), Some(y)) = (origin["x"].as_f64(), origin["y"].as_f64()) {
                    let (x, y) = (x as u32 + 500, y as u32 + 4 * 27 + 5);
                    let png = png.to_rgba8();
                    if x < png.width()
                        && y < png.height()
                        && png.get_pixel(x, y).0 == stage.background()
                    {
                        std::fs::write(format!("{path}.png"), bytes).map_err(|e| e.to_string())?;
                        std::fs::write(format!("{path}.captured"), b"captured")
                            .map_err(|e| e.to_string())?;
                        break;
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    Ok(())
}
