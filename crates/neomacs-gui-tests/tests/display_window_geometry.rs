//! Real display queries and pixelwise window resizing, including macOS.
#![cfg(any(target_os = "linux", target_os = "macos"))]

use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{fs, path::PathBuf, time::Duration};

fn run_geometry_probe(probe: &str, gnu: bool) {
    let root = neomacs_infra::crate_root!().join("../..");
    let backend = if cfg!(target_os = "macos") {
        GuiBackend::Macos
    } else {
        GuiBackend::LinuxX11
    };
    let artifacts = root.join("target/neomacs-gui-tests").join(format!(
        "geometry-{probe}-{}",
        if gnu { "gnu" } else { "neomacs" }
    ));
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifacts)
        .unwrap();
    let program = if gnu {
        std::env::var_os("NEOMACS_GNU_GUI_BINARY")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("emacs"))
    } else {
        std::env::var_os("NEOMACS_GUI_TEST_BINARY")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("target/release/neomacs"))
    };
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &artifacts,
        GuiScenario::new(
            probe,
            root.join("crates/neomacs-gui-tests/fixtures/display-window-geometry.el"),
        ),
    )
    .with_program(program)
    .with_env("NEOMACS_GEOMETRY_PROBE", probe)
    .with_env("TMPDIR", root.join("tmp").to_string_lossy())
    .with_env("RUST_LOG", "warn");
    for (key, value) in session.env() {
        plan = plan.with_env(key, value);
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(20)),
        )
        .unwrap();
    let stderr = fs::read_to_string(&result.artifacts.stderr).unwrap_or_default();
    assert!(!result.timed_out, "{result:#?}\n{stderr}");
    assert_eq!(result.exit_code, Some(0), "{result:#?}\n{stderr}");
    assert!(stderr.contains("GEOMETRY-PASS"), "{stderr}");
}

#[test]
fn graphical_desktop_reports_monitor_dimensions() {
    run_geometry_probe("desktop", false);
}

#[test]
fn pixelwise_window_resize_survives_character_grid_assignment() {
    run_geometry_probe("pixelwise", false);
}

// Prerequisites: GNU GUI Emacs and Xvfb, as for frame_resize_oracle.
#[cfg(target_os = "linux")]
#[test]
fn gnu_pixelwise_window_resize_oracle() {
    run_geometry_probe("pixelwise", true);
}
