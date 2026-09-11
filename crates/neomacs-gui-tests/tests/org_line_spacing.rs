use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiRunStatus, GuiScenario, GuiTestPlan,
    ProcessGuiCommandRunner,
};
use std::{path::PathBuf, time::Duration};

#[test]
#[ignore = "requires a built binary/pdump, Weston, and DejaVu Sans Mono"]
fn org_selection_and_empty_line_cursor_preserve_spacing_and_backgrounds() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let artifacts = root.join("target/neomacs-gui-tests");
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
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(25)),
        )
        .unwrap();
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
