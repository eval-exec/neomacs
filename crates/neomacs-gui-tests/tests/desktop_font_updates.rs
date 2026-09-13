use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{fs, path::PathBuf, process::Command, time::Duration};

#[test]
#[ignore = "requires GNU GUI Emacs, Xvfb, GSettings and Ubuntu Mono/DejaVu fonts"]
fn gnu_opted_in_frame_follows_live_desktop_font() {
    check_live_font(GuiBackend::LinuxX11, "gnu-live-font", LiveFontCase::OptIn);
}

#[test]
#[ignore = "requires release binary/pdump, Weston, GSettings and Ubuntu Mono/DejaVu fonts"]
fn opted_in_frame_follows_live_desktop_font() {
    check_live_font(GuiBackend::LinuxWayland, "live-font", LiveFontCase::OptIn);
}

#[test]
#[ignore = "requires GNU GUI Emacs, Xvfb, GSettings and Ubuntu Mono/DejaVu fonts"]
fn gnu_opted_out_frame_keeps_its_font_while_queries_refresh() {
    check_live_font(
        GuiBackend::LinuxX11,
        "gnu-live-opt-out",
        LiveFontCase::OptOut,
    );
}

#[test]
#[ignore = "requires release binary/pdump, Weston, GSettings and Ubuntu Mono/DejaVu fonts"]
fn opted_out_frame_keeps_its_font_while_queries_refresh() {
    check_live_font(
        GuiBackend::LinuxWayland,
        "live-opt-out",
        LiveFontCase::OptOut,
    );
}

enum LiveFontCase {
    OptIn,
    OptOut,
    ExplicitAndFuture,
}

#[test]
#[ignore = "requires GNU GUI Emacs, Xvfb, GSettings and Ubuntu Mono/DejaVu fonts"]
fn gnu_live_font_overrides_current_fonts_and_sets_future_defaults() {
    check_live_font(
        GuiBackend::LinuxX11,
        "gnu-live-explicit-future",
        LiveFontCase::ExplicitAndFuture,
    );
}

#[test]
#[ignore = "requires release binary/pdump, Weston, GSettings and Ubuntu Mono/DejaVu fonts"]
fn live_font_overrides_current_fonts_and_sets_future_defaults() {
    check_live_font(
        GuiBackend::LinuxWayland,
        "live-explicit-future",
        LiveFontCase::ExplicitAndFuture,
    );
}

fn check_live_font(backend: GuiBackend, scenario: &str, case: LiveFontCase) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let artifacts = root.join("target/neomacs-gui-tests").join(scenario);
    let config = artifacts.join("settings-config");
    if config.exists() {
        fs::remove_dir_all(&config).unwrap();
    }
    fs::create_dir_all(&config).unwrap();
    let schemas = artifacts.join("schemas");
    fs::create_dir_all(&schemas).unwrap();
    fs::copy(
        root.join("crates/neomacs-gui-tests/fixtures/desktop-font-live.gschema.xml"),
        schemas.join("desktop-font-live.gschema.xml"),
    )
    .unwrap();
    assert!(
        Command::new("glib-compile-schemas")
            .arg(&schemas)
            .status()
            .unwrap()
            .success()
    );
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifacts)
        .unwrap();
    let binary = match backend {
        GuiBackend::LinuxX11 => std::env::var_os("NEOMACS_GNU_GUI_BINARY")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("emacs")),
        GuiBackend::LinuxWayland => std::env::var_os("NEOMACS_GUI_TEST_BINARY")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("target/release/neomacs")),
        _ => unreachable!("live GSettings oracle runs on Linux"),
    };
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &artifacts,
        GuiScenario::new(
            scenario,
            root.join("crates/neomacs-gui-tests/fixtures/desktop-font-live.el"),
        ),
    )
    .with_program(binary)
    .with_env("GSETTINGS_BACKEND", "keyfile")
    .with_env(
        "NEOMACS_GUI_LIVE_FONT_CASE",
        match case {
            LiveFontCase::OptIn => "opt-in",
            LiveFontCase::OptOut => "opt-out",
            LiveFontCase::ExplicitAndFuture => "explicit-and-future",
        },
    )
    .with_env("GSETTINGS_SCHEMA_DIR", schemas.to_string_lossy())
    .with_env("XDG_CONFIG_HOME", config.to_string_lossy());
    if backend == GuiBackend::LinuxX11 {
        plan = plan.with_env("GDK_BACKEND", "x11").with_args([
            "-Q".to_string(),
            "--xrm".to_string(),
            "Xft.dpi: 96".to_string(),
            "-l".to_string(),
            root.join("crates/neomacs-gui-tests/fixtures/desktop-font-live.el")
                .to_string_lossy()
                .into_owned(),
        ]);
    }
    for (key, value) in session.env() {
        plan = plan.with_env(key, value);
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(20)),
        )
        .unwrap();
    assert!(!result.timed_out, "{result:#?}");
    assert_eq!(result.exit_code, Some(0), "{result:#?}");
    assert!(
        fs::read_to_string(result.artifacts.stderr)
            .unwrap()
            .contains("LIVE-FONT-PASS")
    );
}
