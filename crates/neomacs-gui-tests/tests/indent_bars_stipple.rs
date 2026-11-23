use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiRunStatus, GuiScenario, GuiTestPlan,
    ProcessGuiCommandRunner,
};
use std::{path::PathBuf, time::Duration};

#[test]
fn indent_bars_stipple_follows_the_frame_origin() {
    let backend = match std::env::var("NEOMACS_GUI_TEST_BACKEND").as_deref() {
        Ok("x11") => GuiBackend::LinuxX11,
        Ok("wayland") => GuiBackend::LinuxWayland,
        Ok("macos") => GuiBackend::Macos,
        Ok("windows") => GuiBackend::Windows,
        _ => cfg_select! {
            target_os = "macos" => { GuiBackend::Macos }
            windows => { GuiBackend::Windows }
            _ => { GuiBackend::LinuxX11 }
        },
    };
    let root = neomacs_infra::workspace_root();
    let artifacts = root.join("tmp/issue831/gui-stipple");
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
            "indent-bars-stipple",
            root.join("crates/neomacs-gui-tests/fixtures/indent-bars-stipple.el"),
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
    let font_width = state["font_width"].as_u64().unwrap();
    let text_left = state["text_left"].as_u64().unwrap();
    let phase = state["phase"].as_u64().unwrap();
    assert_ne!(
        phase, 0,
        "fringe must put the text area off the bitmap period, phase={phase}"
    );
    let pixels = image::open(&result.artifacts.png).unwrap().to_rgba8();
    let mut painted_columns = Vec::new();
    for x in 0..pixels.width() {
        let count = (0..pixels.height())
            .filter(|&y| {
                let p = pixels.get_pixel(x, y).0;
                p[0].abs_diff(0xed) <= 2 && p[1].abs_diff(0x17) <= 2 && p[2].abs_diff(0x31) <= 2
            })
            .count();
        if count > 8 {
            painted_columns.push(x);
        }
    }
    assert!(
        !painted_columns.is_empty(),
        "stipple column missing: {}",
        result.artifacts.png.display()
    );
    let surface_scale = f64::from(pixels.width()) / state["frame_width"].as_f64().unwrap();
    // Column 0 of the bitmap is lit at frame x where x % font_width == 0.
    let remainder = text_left % font_width;
    let expected = text_left + (font_width - remainder) % font_width;
    let painted = f64::from(painted_columns[0]) / surface_scale;
    assert!(
        (painted - expected as f64).abs() < 0.75,
        "frame-origin column {painted} should be {expected} (text_left={text_left}, width={font_width}); painted {painted_columns:?}"
    );
}
