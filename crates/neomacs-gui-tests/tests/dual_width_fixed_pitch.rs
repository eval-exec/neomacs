//! Real dual-width font layout, including a fixed-pitch backend classification.
#![cfg(target_os = "linux")]
use neomacs_gui_tests::{
    CommandSpec, DisplayHarness, GuiArtifactSet, GuiBackend, GuiCommandRunner, GuiRunOptions,
    GuiRunStatus, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{fs, path::PathBuf, time::Duration};

#[test]
fn fixed_pitch_cjk_layout_and_measurement_match_gnu() {
    let root = neomacs_infra::workspace_root();
    let control = root.join(format!("tmp/dual-width-font-{}", std::process::id()));
    fs::create_dir_all(control.join("fonts")).unwrap();
    fs::copy(
        neomacs_test_fonts::plemol_jp_console_nf_regular(),
        control.join("fonts/PlemolJPConsoleNF-Regular.ttf"),
    )
    .unwrap();
    let config = control.join("fonts.conf");
    // CoreText exposes TraitMonoSpace for this dual-width font. Fontconfig
    // usually calls it dual-width (90); force 100 to exercise the same policy.
    fs::write(&config, format!("<fontconfig><dir>{}/fonts</dir><cachedir>{}/cache</cachedir><match target=\"scan\"><test name=\"family\" compare=\"eq\"><string>PlemolJP Console NF</string></test><edit name=\"spacing\" mode=\"assign\"><int>100</int></edit></match></fontconfig>",control.display(),control.display())).unwrap();
    let fixture = root.join("crates/neomacs-gui-tests/fixtures/dual-width-fixed-pitch/init.el");
    let backend = match std::env::var("NEOMACS_GUI_TEST_BACKEND").as_deref() {
        Ok("wayland") => GuiBackend::LinuxWayland,
        _ => GuiBackend::LinuxX11,
    };
    let session = DisplayHarness::for_backend(backend)
        .start_session(&control)
        .unwrap();
    let gnu_session = DisplayHarness::for_backend(GuiBackend::LinuxX11)
        .start_session(&control.join("gnu-display"))
        .unwrap();
    let gnu_state = control.join("gnu-state.json");
    let mut gnu_command = CommandSpec {
        program: std::env::var_os("NEOMACS_GNU_EMACS_BINARY")
            .map(PathBuf::from)
            .unwrap_or_else(|| "emacs".into()),
        args: vec![
            "-Q".into(),
            "--xrm".into(),
            "Xft.dpi: 96".into(),
            "--load".into(),
            fixture.display().to_string(),
        ],
        env: vec![
            ("FONTCONFIG_FILE".into(), config.display().to_string()),
            ("GDK_BACKEND".into(), "x11".into()),
            ("GSETTINGS_BACKEND".into(), "memory".into()),
            (
                "NEOMACS_GUI_STATE_JSON".into(),
                gnu_state.display().to_string(),
            ),
        ],
    };
    gnu_command.env.extend(gnu_session.env().iter().cloned());
    let output = ProcessGuiCommandRunner
        .run(
            &gnu_command,
            &GuiArtifactSet::new(&control, GuiBackend::LinuxX11, "gnu-dual-width"),
            &GuiRunOptions::with_timeout(Duration::from_secs(30)),
        )
        .unwrap();
    assert!(!output.timed_out, "GNU: {output:#?}");
    assert_eq!(output.exit_code, Some(0), "GNU: {output:#?}");
    let expected: serde_json::Value =
        serde_json::from_slice(&fs::read(&gnu_state).unwrap()).unwrap();
    assert_eq!(
        expected["positions"],
        serde_json::json!([0, 7, 14, 28, 42, 56, 70, 77, 84])
    );
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &control,
        GuiScenario::new("dual-width-fixed-pitch", &fixture),
    )
    .with_program(binary)
    .with_args([
        "-Q".to_owned(),
        "--xrm".to_owned(),
        "Xft.dpi: 96".to_owned(),
        "--load".to_owned(),
        fixture.display().to_string(),
    ])
    .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "64")
    .with_env("GSETTINGS_BACKEND", "memory")
    .with_env("FONTCONFIG_FILE", config.display().to_string())
    .with_env("WINIT_X11_SCALE_FACTOR", "1");
    for (key, value) in session.env() {
        plan = plan.with_env(key.clone(), value.clone());
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(30)),
        )
        .unwrap();
    assert!(!result.timed_out, "{result:#?}");
    assert_eq!(result.exit_code, Some(0), "{result:#?}");
    assert_eq!(result.status, GuiRunStatus::Passed, "{result:#?}");
    let actual: serde_json::Value =
        serde_json::from_slice(&fs::read(&result.artifacts.gui_state).unwrap()).unwrap();
    assert_eq!(
        actual, expected,
        "measurement and every glyph position must match GNU"
    );
    let snapshot: serde_json::Value = serde_json::from_slice(
        &fs::read(format!(
            "{}.frame.json",
            result.artifacts.gui_state.display()
        ))
        .unwrap(),
    )
    .unwrap();
    let frame = &snapshot["frames"][0];
    let fonts = frame["fonts"].as_object().unwrap();
    assert!(
        fonts
            .values()
            .any(|font| font["family"] == "PlemolJP Console NF"
                && font["glyph_advance"]["MonospaceCells"] == 7.0),
        "publish the outline cell policy"
    );
    assert!(
        result.png_bytes.unwrap_or_default() > 0,
        "native GUI paints the glyphs"
    );
}
