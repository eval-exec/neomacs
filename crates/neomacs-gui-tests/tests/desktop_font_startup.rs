use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiRunStatus, GuiScenario, GuiTestPlan,
    ProcessGuiCommandRunner, WaylandOutput,
};
use std::{fs, path::PathBuf, process::Command, time::Duration};

#[test]
#[ignore = "requires release binary/pdump, Weston, glib-compile-schemas, Ubuntu Mono, DejaVu and SVG packages"]
fn desktop_monospace_font_drives_initial_window_and_svg_metrics() {
    check_desktop_font_startup(WaylandOutput::Standard, "desktop-font-startup");
}

#[test]
#[ignore = "requires release binary/pdump, Weston, glib-compile-schemas, Ubuntu Mono, DejaVu and SVG packages"]
fn hidpi_desktop_font_preserves_initial_and_resized_column_grid() {
    check_desktop_font_startup(WaylandOutput::HiDpi4k, "desktop-font-startup-hidpi");
}

fn check_desktop_font_startup(output: WaylandOutput, scenario: &str) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let artifacts = root.join("target/neomacs-gui-tests").join(scenario);
    let schemas = artifacts.join("desktop-font-startup-schemas");
    fs::create_dir_all(&schemas).unwrap();
    fs::copy(
        root.join("crates/neomacs-gui-tests/fixtures/desktop-fonts.gschema.xml"),
        schemas.join("desktop-fonts.gschema.xml"),
    )
    .unwrap();
    assert!(
        Command::new("glib-compile-schemas")
            .arg(&schemas)
            .status()
            .unwrap()
            .success()
    );
    let backend = GuiBackend::LinuxWayland;
    let session = DisplayHarness::WestonHeadless(output)
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
            scenario,
            root.join("crates/neomacs-gui-tests/fixtures/desktop-font-startup.el"),
        ),
    )
    .with_program(binary)
    .with_env("GSETTINGS_SCHEMA_DIR", schemas.to_string_lossy())
    .with_env("GSETTINGS_BACKEND", "memory");
    for (key, value) in session.env() {
        plan = plan.with_env(key.clone(), value.clone());
    }
    for key in ["NEOMACS_GUI_SVG_LIB_DIR", "NEOMACS_GUI_SVG_TAG_MODE_DIR"] {
        plan = plan.with_env(key, std::env::var(key).expect("set SVG package directory"));
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(20)),
        )
        .unwrap();
    assert!(!result.timed_out, "{result:#?}");
    assert_eq!(result.exit_code, Some(0), "{result:#?}");
    assert_eq!(result.status, GuiRunStatus::Passed, "{result:#?}");
}
