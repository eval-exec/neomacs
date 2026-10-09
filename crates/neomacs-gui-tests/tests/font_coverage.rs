//! Exact font coverage and its emoji/icon callers agree with GNU Emacs.
#![cfg(target_os = "linux")]
use neomacs_gui_tests::{
    CommandSpec, DisplayHarness, GuiArtifactSet, GuiBackend, GuiCommandRunner, GuiRunOptions,
    GuiRunStatus, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{fs, path::PathBuf, time::Duration};

fn assert_font_coverage(state: &serde_json::Value) {
    assert_eq!(state["emoji-labels"], serde_json::json!(["A", "B", "█"]));
    assert_eq!(state["emoji-icon"], "🔽");
    assert_eq!(state["entity-ascii"], true);
    assert_eq!(state["entity-block"], true);
    assert_eq!(state["shown-block"], true);
    assert_eq!(state["missing"], false);
    assert_eq!(state["shown-missing"], false);
    assert_eq!(state["shown-space"], true);
}

#[test]
fn font_entities_and_displayed_objects_report_glyph_coverage() {
    let root = neomacs_infra::workspace_root();
    let artifacts = root.join(format!("tmp/font-coverage-{}", std::process::id()));
    let fonts = artifacts.join("fonts");
    fs::create_dir_all(&fonts).unwrap();
    fs::copy(
        neomacs_test_fonts::spleen_2_2_0().otb(),
        fonts.join("spleen.otb"),
    )
    .unwrap();
    fs::copy(
        neomacs_test_fonts::noto_color_emoji_2_051(),
        fonts.join("emoji.ttf"),
    )
    .unwrap();
    let config = artifacts.join("fonts.conf");
    fs::write(&config, format!(
        "<fontconfig><dir>{}</dir><cachedir>{}</cachedir><alias><family>monospace</family><prefer><family>Spleen</family></prefer></alias></fontconfig>",
        fonts.display(), artifacts.join("fontcache").display())).unwrap();
    let fixture = root.join("crates/neomacs-gui-tests/fixtures/font-coverage/init.el");
    let backend = match std::env::var("NEOMACS_GUI_TEST_BACKEND").as_deref() {
        Ok("wayland") => GuiBackend::LinuxWayland,
        _ => GuiBackend::LinuxX11,
    };
    let gnu_session = DisplayHarness::for_backend(GuiBackend::LinuxX11)
        .start_session(&artifacts.join("gnu"))
        .unwrap();
    let labels = root.join("lisp/international/emoji-labels.el");
    let state = artifacts.join("gnu-state.json");
    let mut command = CommandSpec {
        program: std::env::var_os("NEOMACS_GUI_TEST_GNU_EMACS")
            .or_else(|| std::env::var_os("NEOMACS_GNU_EMACS_BINARY"))
            .map(PathBuf::from)
            .unwrap_or_else(|| "emacs".into()),
        args: vec!["-Q".into(), "--load".into(), fixture.display().to_string()],
        env: vec![
            ("GDK_BACKEND".into(), "x11".into()),
            (
                "NEOMACS_FONT_COVERAGE_LABELS".into(),
                labels.display().to_string(),
            ),
            ("FONTCONFIG_FILE".into(), config.display().to_string()),
            ("GSETTINGS_BACKEND".into(), "memory".into()),
            ("NEOMACS_GUI_STATE_JSON".into(), state.display().to_string()),
        ],
    };
    command.env.extend(gnu_session.env().iter().cloned());
    let output = ProcessGuiCommandRunner
        .run(
            &command,
            &GuiArtifactSet::new(&artifacts, GuiBackend::LinuxX11, "gnu"),
            &GuiRunOptions::with_timeout(Duration::from_secs(30)),
        )
        .unwrap();
    assert!(!output.timed_out, "GNU: {output:#?}");
    assert_eq!(output.exit_code, Some(0));
    let gnu: serde_json::Value = serde_json::from_slice(&fs::read(state).unwrap()).unwrap();
    assert_eq!(gnu["native-engine"], false, "GNU probe must run GNU Emacs");
    assert_font_coverage(&gnu);
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifacts.join("native"))
        .unwrap();
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &artifacts,
        GuiScenario::new("font-coverage", &fixture),
    )
    .with_program(binary)
    .with_env("NEOMACS_FONT_COVERAGE_LABELS", labels.display().to_string())
    .with_env("FONTCONFIG_FILE", config.display().to_string())
    .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "64");
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
    assert!(!result.timed_out);
    assert_eq!(result.exit_code, Some(0));
    let measurements: serde_json::Value =
        serde_json::from_slice(&fs::read(&result.artifacts.gui_state).unwrap()).unwrap();
    assert_eq!(measurements["native-engine"], true);
    assert_font_coverage(&measurements);
    assert_eq!(
        measurements["full-label-count"], gnu["full-label-count"],
        "complete emoji filtering must agree for the same fonts and label data"
    );
    assert_eq!(
        measurements["full-labels"], gnu["full-labels"],
        "complete emoji membership and order must agree for the same fonts and label data"
    );
    let snapshot: serde_json::Value =
        serde_json::from_slice(&fs::read(&result.artifacts.frame_snapshot_json).unwrap()).unwrap();
    let mut blocks = 0;
    for frame in snapshot["frames"].as_array().unwrap() {
        for window in frame["window_matrices"].as_array().unwrap() {
            for row in window["matrix"]["rows"].as_array().unwrap() {
                for area in row["glyphs"].as_array().unwrap() {
                    for glyph in area.as_array().unwrap() {
                        if glyph["glyph_type"]["Char"]["ch"] == "█" {
                            blocks += 1;
                            let face = glyph["face_id"].as_u64().unwrap().to_string();
                            let selected = &frame["char_fonts"][&face]["█"];
                            assert!(selected["glyph_id"].as_u64().is_some_and(|id| id > 0));
                            let font = selected["resolved_font_id"].as_u64().unwrap().to_string();
                            let file = frame["fonts"][&font]["identity"]["file_path"]
                                .as_str()
                                .unwrap();
                            assert!(
                                file.ends_with("spleen.otb"),
                                "block rendered through {file}"
                            );
                        }
                    }
                }
            }
        }
    }
    assert_eq!(
        blocks, 1,
        "displayed block must carry its actual font and glyph"
    );
    assert!(result.png_bytes.unwrap_or_default() > 0);
}
