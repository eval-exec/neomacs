use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiRunStatus, GuiScenario, GuiTestPlan,
    ProcessGuiCommandRunner,
};
use std::{path::PathBuf, time::Duration};

#[test]
fn space_fill_column_indicator_paints_one_stipple_column() {
    let backend = match std::env::var("NEOMACS_GUI_TEST_BACKEND").as_deref() {
        Ok("wayland") => GuiBackend::LinuxWayland,
        Ok("macos") => GuiBackend::Macos,
        Ok("windows") => GuiBackend::Windows,
        _ => GuiBackend::LinuxX11,
    };
    let root = neomacs_infra::workspace_root();
    let artifacts = root.join("tmp/issue600/gui-stipple");
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
            "fill-column-stipple",
            root.join("crates/neomacs-gui-tests/fixtures/fill-column-stipple.el"),
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
            GuiRunOptions::with_timeout(Duration::from_secs(30)),
        )
        .unwrap();
    assert_eq!(result.status, GuiRunStatus::Passed, "{result:#?}");
    assert!(!result.timed_out, "GUI fixture must finish: {result:#?}");
    assert_eq!(result.exit_code, Some(0), "{result:#?}");
    let state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&result.artifacts.gui_state).unwrap()).unwrap();
    assert_eq!(state["graphic"], true);
    let pixels = image::open(&result.artifacts.png).unwrap().to_rgba8();
    let mut painted_columns = Vec::new();
    for x in 0..pixels.width() {
        let count = (0..pixels.height())
            .filter(|&y| {
                let p = pixels.get_pixel(x, y).0;
                p[0].abs_diff(0xed) <= 2 && p[1].abs_diff(0x17) <= 2 && p[2].abs_diff(0x31) <= 2
            })
            .count();
        if count > 20 {
            painted_columns.push(x);
        }
    }
    let surface_scale = f64::from(pixels.width()) / state["frame_width"].as_f64().unwrap();
    let one_column_pixels = surface_scale.round().max(1.0) as usize;
    assert_eq!(
        painted_columns.len(),
        one_column_pixels,
        "bitmap should paint one vertical column at fill-column, found {painted_columns:?}; {}",
        result.artifacts.png.display()
    );
    // Public window geometry locates the indicator cell independently of the
    // emitted glyphs: a correctly counted line at the wrong column still fails.
    let left = state["text_left"].as_u64().unwrap();
    let cell_width = state["font_width"].as_u64().unwrap();
    let column = state["fill_column"].as_u64().unwrap();
    let start = left + column * cell_width;
    let painted = f64::from(painted_columns[0]) / surface_scale;
    assert!(
        (start as f64..(start + cell_width) as f64).contains(&painted),
        "stipple column {painted} must lie in fill-column cell {start}..{}",
        start + cell_width
    );
}
