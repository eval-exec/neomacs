//! GNOME text scaling is font DPI, independent of compositor backing scale.
//!
//! GNU's X11 oracle observes the effective Xft DPI produced by GNOME's
//! xsettings daemon. Native Wayland observes the originating GSettings key.
//! Both must open the same point-sized font; integer pixel sizes bypass DPI
//! (`src/font.c:339-362`, `src/xsettings.c:780-815` at the pinned GNU commit).
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    DisplayHarness, GuiArtifactSet, GuiBackend, GuiCommandRunner, GuiRunOptions, GuiRunStatus,
    GuiScenario, GuiTestPlan, ProcessGuiCommandRunner, WaylandOutput, WestonDesktop,
    start_weston_with_desktop,
};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

#[test]
fn desktop_text_scale_one_matches_gnu_font_dpi() {
    check_text_scale("one", "1.0", 96, 16);
}

#[test]
fn desktop_text_scale_fractional_matches_gnu_font_dpi() {
    check_text_scale("fractional", "1.5", 144, 24);
}

#[test]
fn desktop_text_scale_two_matches_gnu_font_dpi() {
    check_text_scale("two", "2.0", 192, 32);
}

fn check_text_scale(name: &str, factor: &str, dpi: u32, point_pixels: u32) {
    let root = neomacs_infra::workspace_root();
    let artifacts = root.join(format!(
        "tmp/neomacs-gui-tests/desktop-text-scaling-{name}-{}-{}",
        std::process::id(),
        neomacs_gui_tests::unique_nonce()
    ));
    fs::create_dir_all(&artifacts).unwrap();
    let schemas = artifacts.join("schemas");
    let config = artifacts.join("settings-config");
    fs::create_dir_all(&schemas).unwrap();
    fs::create_dir_all(&config).unwrap();
    // Each process gets an isolated backend. No host GNOME settings change.
    fs::write(
        schemas.join("desktop-text-scaling.gschema.xml"),
        r#"<schemalist>
  <schema id="org.gnome.desktop.interface" path="/org/gnome/desktop/interface/">
    <key name="monospace-font-name" type="s"><default>'DejaVu Sans Mono 12'</default></key>
    <key name="font-name" type="s"><default>'DejaVu Sans 12'</default></key>
    <key name="toolbar-style" type="s"><default>'icons'</default></key>
    <key name="text-scaling-factor" type="d"><default>1.0</default></key>
  </schema>
</schemalist>"#,
    )
    .unwrap();
    let compiled = Command::new("glib-compile-schemas")
        .arg(&schemas)
        .output()
        .unwrap();
    assert!(compiled.status.success(), "{compiled:?}");
    let configured = Command::new("gsettings")
        .args([
            "set",
            "org.gnome.desktop.interface",
            "text-scaling-factor",
            factor,
        ])
        .env("GSETTINGS_SCHEMA_DIR", &schemas)
        .env("GSETTINGS_BACKEND", "keyfile")
        .env("XDG_CONFIG_HOME", &config)
        .output()
        .unwrap();
    assert!(configured.status.success(), "{configured:?}");
    let fontconfig = crate::desktop_font_pins::desktop_font_fontconfig(&artifacts);
    let fixture = root.join("crates/neomacs-gui-tests/fixtures/desktop-text-scaling.el");
    let options = GuiRunOptions::with_timeout(Duration::from_secs(30));

    let oracle = neomacs_test_oracle::gnu::gnu_oracle();
    assert!(oracle.is_attested(), "requires the pinned GNU oracle");
    let gnu = oracle.executable_path();
    let prefix = gnu.parent().unwrap().parent().unwrap();
    let check = Command::new("bash")
        .arg(root.join("scripts/ci/check-gnu-oracle.sh"))
        .arg(&gnu)
        .arg("31.1")
        .arg(prefix.join("share/gnu-emacs-oracle/build-manifest"))
        .arg("a360712c9d272d950d8d8255ef74570f7e90b7d9")
        .output()
        .unwrap();
    fs::write(artifacts.join("oracle-check.stdout"), &check.stdout).unwrap();
    fs::write(artifacts.join("oracle-check.stderr"), &check.stderr).unwrap();
    assert!(check.status.success(), "oracle check failed: {check:?}");
    eprintln!("{}", String::from_utf8_lossy(&check.stdout));

    let gnu_session = DisplayHarness::Xvfb
        .start_session(&artifacts.join("gnu-display"))
        .unwrap();
    let gnu_plan = GuiTestPlan::new(
        GuiBackend::LinuxX11,
        &root,
        &artifacts,
        GuiScenario::new("gnu-text-scaling", &fixture),
    )
    .with_program(gnu)
    .with_args([
        "-Q".to_owned(),
        "--xrm".to_owned(),
        format!("Xft.dpi: {dpi}"),
        "-l".to_owned(),
        fixture.display().to_string(),
    ])
    .with_env("GDK_BACKEND", "x11")
    .with_env("GDK_SCALE", "1")
    .with_env("GDK_DPI_SCALE", "1")
    .with_env("FONTCONFIG_FILE", fontconfig.display().to_string())
    .with_env("GSETTINGS_SCHEMA_DIR", schemas.display().to_string())
    .with_env("GSETTINGS_BACKEND", "keyfile")
    .with_env("XDG_CONFIG_HOME", config.display().to_string());
    let mut gnu_command = gnu_plan.command_spec();
    gnu_command.env.extend(gnu_session.env().iter().cloned());
    let gnu_artifacts = GuiArtifactSet::new(&artifacts, GuiBackend::LinuxX11, "gnu-text-scaling");
    fs::create_dir_all(gnu_artifacts.gui_state.parent().unwrap()).unwrap();
    let output = ProcessGuiCommandRunner
        .run(&gnu_command, &gnu_artifacts, &options)
        .unwrap();
    fs::write(&gnu_artifacts.stdout, &output.stdout).unwrap();
    fs::write(&gnu_artifacts.stderr, &output.stderr).unwrap();
    assert!(!output.timed_out, "GNU text-scaling timed out");
    assert_eq!(output.exit_code, Some(0), "{output:?}");
    let expected = read_state(&gnu_artifacts.gui_state);
    assert_eq!(expected["points"]["size"], point_pixels, "{expected}");
    assert_eq!(expected["pixels"]["size"], 16, "{expected}");

    // Scale-one compositor isolates font scaling from surface backing scale.
    let native_session = start_weston_with_desktop(
        &artifacts.join("native-display"),
        WaylandOutput::Standard,
        WestonDesktop::Solid,
    )
    .unwrap();
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let mut native_plan = GuiTestPlan::new(
        GuiBackend::LinuxWayland,
        &root,
        &artifacts,
        GuiScenario::new("native-text-scaling", fixture),
    )
    .with_program(binary)
    .with_env("GDK_SCALE", "1")
    .with_env("GDK_DPI_SCALE", "1")
    .with_env("FONTCONFIG_FILE", fontconfig.display().to_string())
    .with_env("GSETTINGS_SCHEMA_DIR", schemas.display().to_string())
    .with_env("GSETTINGS_BACKEND", "keyfile")
    .with_env("XDG_CONFIG_HOME", config.display().to_string());
    for (key, value) in native_session.env() {
        native_plan = native_plan.with_env(key.clone(), value.clone());
    }
    let result = native_plan
        .run_with(&mut ProcessGuiCommandRunner, options)
        .unwrap();
    assert!(!result.timed_out, "{result:#?}");
    assert_eq!(result.exit_code, Some(0), "{result:#?}");
    assert_eq!(result.status, GuiRunStatus::Passed, "{result:#?}");
    let actual = read_state(&result.artifacts.gui_state);
    eprintln!("text-scale={factor}: GNU={expected}; Neomacs={actual}");
    assert_eq!(
        actual, expected,
        "text-scaling-factor={factor}: point fonts must match effective GNU DPI; integer pixel fonts must stay unchanged"
    );
}

fn read_state(path: &Path) -> Value {
    serde_json::from_slice(
        &fs::read(path).unwrap_or_else(|error| panic!("missing {}: {error}", path.display())),
    )
    .unwrap()
}
