//! Issue #574: real GPU gutters must follow buffer lines through replacements.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{path::PathBuf, time::Duration};

fn painted_rows(pixels: &image::RgbaImage, colour: [u8; 3]) -> Vec<u32> {
    (0..pixels.height())
        .filter(|&y| {
            (0..pixels.width())
                .filter(|&x| pixels.get_pixel(x, y).0[..3] == colour)
                .count()
                >= 4
        })
        .collect()
}

fn check_current_line(case: &str) {
    let root = neomacs_infra::workspace_root();
    let artifacts = root.join(format!("tmp/issue-574/gui-{case}-{}", std::process::id()));
    let backend = match std::env::var("NEOMACS_GUI_TEST_BACKEND").as_deref() {
        Ok("wayland") => GuiBackend::LinuxWayland,
        _ => GuiBackend::LinuxX11,
    };
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
            case,
            root.join("crates/neomacs-gui-tests/fixtures/display-line-numbers.el"),
        ),
    )
    .with_program(binary)
    .with_env("NEOMACS_LINE_NUMBER_CASE", case)
    .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "10000");
    for (key, value) in session.env() {
        plan = plan.with_env(key, value);
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(30)),
        )
        .unwrap();
    assert!(!result.timed_out, "{result:#?}");
    assert_eq!(result.exit_code, Some(0), "{result:#?}");
    let pixels = image::open(&result.artifacts.png).unwrap().to_rgba8();
    let point_line = painted_rows(&pixels, [0, 0, 255]);
    let current_gutter = painted_rows(&pixels, [255, 255, 0]);
    assert!(
        !point_line.is_empty(),
        "the point line must be painted blue"
    );
    assert_eq!(
        current_gutter,
        point_line,
        "the yellow current-line gutter must align with the blue point line; artifacts: {}",
        artifacts.display()
    );
}

#[test]
fn display_string_newlines_keep_the_current_gutter_on_the_point_line() {
    check_current_line("expanded");
}

#[test]
fn hidden_buffer_newlines_keep_the_current_gutter_on_the_point_line() {
    check_current_line("hidden");
}
