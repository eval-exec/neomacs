//! Category-aware word wrapping and kinsoku through private GNU/Neomacs GUI frames.
#![cfg(target_os = "linux")]
use neomacs_gui_tests::{
    CommandSpec, DisplayHarness, GuiArtifactSet, GuiBackend, GuiCommandRunner, GuiRunOptions,
    GuiRunStatus, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{fs, path::PathBuf, time::Duration};

fn assert_geometry(state: &serde_json::Value) {
    assert!(state["width"].as_u64().unwrap() >= 18, "{state}");
    assert_eq!(state["cjk"][0], 15, "{state}");
    assert!(state["cjk"][1].as_i64().unwrap() > 15, "{state}");
    assert_eq!(state["cjk"][2], 15, "{state}");
    assert_eq!(state["ascii"][0], 15, "{state}");
    assert!(state["ascii"][1].as_i64().unwrap() > 15, "{state}");
    for rule in ["not-at-bol", "not-at-eol"] {
        assert_eq!(
            state[rule][0].as_i64().unwrap() - 1,
            state[rule][1].as_i64().unwrap(),
            "{rule}: {state}"
        );
    }
}

#[test]
fn category_word_wrap_and_kinsoku_match_live_gnu() {
    let root = neomacs_infra::workspace_root();
    let artifacts = root.join(format!("tmp/category-word-wrap-{}", std::process::id()));
    let fixture = root.join("crates/neomacs-gui-tests/fixtures/category-word-wrap/init.el");
    let backend = match std::env::var("NEOMACS_GUI_TEST_BACKEND").as_deref() {
        Ok("wayland") => GuiBackend::LinuxWayland,
        _ => GuiBackend::LinuxX11,
    };
    let gnu_session = DisplayHarness::for_backend(GuiBackend::LinuxX11)
        .start_session(&artifacts.join("gnu"))
        .unwrap();
    let gnu_state = artifacts.join("gnu-state.json");
    let mut command = CommandSpec {
        program: std::env::var_os("NEOMACS_GNU_EMACS_BINARY")
            .map(PathBuf::from)
            .unwrap_or_else(|| "emacs".into()),
        args: vec!["-Q".into(), "--load".into(), fixture.display().to_string()],
        env: vec![
            ("GDK_BACKEND".into(), "x11".into()),
            ("GSETTINGS_BACKEND".into(), "memory".into()),
            (
                "NEOMACS_GUI_STATE_JSON".into(),
                gnu_state.display().to_string(),
            ),
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
    assert_eq!(output.exit_code, Some(0), "GNU: {output:#?}");
    let gnu: serde_json::Value = serde_json::from_slice(&fs::read(gnu_state).unwrap()).unwrap();
    assert_eq!(gnu["native-engine"], false);
    assert_geometry(&gnu);

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
        GuiScenario::new("category-word-wrap", &fixture),
    )
    .with_program(binary)
    .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "1");
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
    let native: serde_json::Value =
        serde_json::from_slice(&fs::read(&result.artifacts.gui_state).unwrap()).unwrap();
    assert_eq!(native["native-engine"], true);
    assert_geometry(&native);
    // Equal column counts alone do not guarantee equal CJK glyph geometry.
    // Check the comparison's preconditions before comparing absolute positions.
    for metric in ["width", "character-widths", "pixel-widths"] {
        assert_eq!(
            native[metric], gnu[metric],
            "{metric}: GNU={gnu}, Neomacs={native}"
        );
    }
    for result in ["cjk", "ascii", "not-at-bol", "not-at-eol"] {
        assert_eq!(
            native[result], gnu[result],
            "{result}: GNU={gnu}, Neomacs={native}"
        );
    }
}
