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

#[derive(strum::AsRefStr)]
#[strum(serialize_all = "kebab-case")]
enum LiveFontCase {
    OptIn,
    OptOut,
    ExplicitAndFuture,
    Geometry,
    Repeated,
    Inhibited,
    Child,
    ChildChrome,
    Minimum,
    Fullscreen,
}

#[test]
#[ignore = "requires release binary/pdump, Weston presentation feedback, GSettings and fonts"]
fn fullscreen_font_update_keeps_pixels_and_refreshes_columns() {
    check_live_font(
        GuiBackend::LinuxWayland,
        "live-fullscreen",
        LiveFontCase::Fullscreen,
    );
}

#[test]
#[ignore = "requires GNU GUI Emacs, Xvfb, GSettings and Ubuntu Mono/DejaVu fonts"]
fn gnu_live_font_preserves_child_frame_grid() {
    check_live_font(GuiBackend::LinuxX11, "gnu-live-child", LiveFontCase::Child);
}

#[test]
#[ignore = "requires release binary/pdump, Weston, GSettings and fonts"]
fn live_font_preserves_child_frame_grid() {
    check_live_font(GuiBackend::LinuxWayland, "live-child", LiveFontCase::Child);
}

#[test]
#[ignore = "requires GNU GUI Emacs, Xvfb, GSettings and fonts"]
fn gnu_child_chrome_preserves_initial_and_live_font_text_grid() {
    check_live_font(
        GuiBackend::LinuxX11,
        "gnu-live-child-chrome",
        LiveFontCase::ChildChrome,
    );
}

#[test]
#[ignore = "requires release binary/pdump, Weston, GSettings and fonts"]
fn child_chrome_preserves_initial_and_live_font_text_grid() {
    check_live_font(
        GuiBackend::LinuxWayland,
        "live-child-chrome",
        LiveFontCase::ChildChrome,
    );
}

#[test]
#[ignore = "requires GNU GUI Emacs, Xvfb, GSettings and fonts"]
fn gnu_split_window_minimum_overrides_font_resize_inhibition() {
    check_live_font(
        GuiBackend::LinuxX11,
        "gnu-live-minimum",
        LiveFontCase::Minimum,
    );
}

#[test]
#[ignore = "requires release binary/pdump, Weston, GSettings and fonts"]
fn split_window_minimum_overrides_font_resize_inhibition() {
    check_live_font(
        GuiBackend::LinuxWayland,
        "live-minimum",
        LiveFontCase::Minimum,
    );
}

#[test]
#[ignore = "requires GNU GUI Emacs, Xvfb, GSettings and Ubuntu Mono/DejaVu fonts"]
fn gnu_inhibited_font_resize_keeps_pixels_and_refreshes_columns() {
    check_live_font(
        GuiBackend::LinuxX11,
        "gnu-live-inhibited",
        LiveFontCase::Inhibited,
    );
}

#[test]
#[ignore = "requires release binary/pdump, Weston presentation feedback, GSettings and fonts"]
fn inhibited_font_resize_keeps_pixels_and_refreshes_columns() {
    check_live_font(
        GuiBackend::LinuxWayland,
        "live-inhibited",
        LiveFontCase::Inhibited,
    );
}

#[test]
#[ignore = "requires GNU GUI Emacs, Xvfb, GSettings and Ubuntu Mono/DejaVu fonts"]
fn gnu_duplicate_font_updates_are_noops_and_distinct_updates_repeat() {
    check_live_font(
        GuiBackend::LinuxX11,
        "gnu-live-repeated",
        LiveFontCase::Repeated,
    );
}

#[test]
#[ignore = "requires release binary/pdump, Weston presentation feedback, GSettings and fonts"]
fn duplicate_font_updates_are_noops_and_distinct_updates_repeat() {
    check_live_font(
        GuiBackend::LinuxWayland,
        "live-repeated",
        LiveFontCase::Repeated,
    );
}

#[test]
#[ignore = "requires GNU GUI Emacs, Xvfb, GSettings and Ubuntu Mono/DejaVu fonts"]
fn gnu_live_font_preserves_the_character_grid() {
    check_live_font(
        GuiBackend::LinuxX11,
        "gnu-live-geometry",
        LiveFontCase::Geometry,
    );
}

#[test]
#[ignore = "requires release binary/pdump, Weston presentation feedback, GSettings and fonts"]
fn live_font_preserves_the_presented_character_grid() {
    check_live_font(
        GuiBackend::LinuxWayland,
        "live-geometry",
        LiveFontCase::Geometry,
    );
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
    .with_env("NEOMACS_GUI_LIVE_FONT_CASE", case.as_ref())
    .with_env("GSETTINGS_SCHEMA_DIR", schemas.to_string_lossy())
    .with_env("XDG_CONFIG_HOME", config.to_string_lossy());
    if backend == GuiBackend::LinuxWayland {
        let receipt = artifacts.join("presentation.sexp");
        if receipt.exists() {
            fs::remove_file(&receipt).unwrap();
        }
        plan = plan.with_env(
            "NEOMACS_GUI_PRESENTATION_RECEIPT",
            receipt.to_string_lossy(),
        );
    }
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
