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
    // The attested GNU capture (width 8, text left 5) is x=8,16,...,192.
    // Glyph-local tiling would light text_left + k*width (5,13,21,...).
    // A doubled bar still has the right first column, so require the whole set.
    let remainder = text_left % font_width;
    let expected = text_left + (font_width - remainder) % font_width;
    let bars = logical_bars(&painted_columns, surface_scale);
    assert_eq!(
        bars.len(),
        24,
        "expected one bar per space, got {bars:?} from {painted_columns:?}"
    );
    for k in 0..24usize {
        let want = (expected + (k as u64) * font_width) as f64;
        let got = bars[k];
        assert!(
            (got - want).abs() < 0.75,
            "bar {k} at {got} should be {want} (text_left={text_left}, width={font_width})"
        );
        let ghost = (text_left + (k as u64) * font_width) as f64;
        if (ghost - want).abs() > 1.0 {
            assert!(
                bars.iter().all(|bar| (bar - ghost).abs() >= 0.75),
                "glyph-local column {ghost} is painted: {bars:?}"
            );
        }
    }
}

/// One logical bar per run of device columns, at the run's left edge.
fn logical_bars(painted: &[u32], scale: f64) -> Vec<f64> {
    let mut bars = Vec::new();
    let mut index = 0;
    while index < painted.len() {
        let start = painted[index];
        let mut end = start;
        index += 1;
        while index < painted.len() && painted[index] == end + 1 {
            let width = f64::from(painted[index] - start + 1) / scale;
            if width > 1.25 {
                break;
            }
            end = painted[index];
            index += 1;
        }
        let logical_width = f64::from(end - start + 1) / scale;
        assert!(
            logical_width < 1.25,
            "stipple bar at device x={start} is {logical_width} logical px wide"
        );
        bars.push(f64::from(start) / scale);
    }
    bars
}
