//! Interactive saving preserves a legacy encoding and its original bytes.
#![cfg(target_os = "linux")]
use neomacs_gui_tests::{
    CommandSpec, DisplayHarness, GuiArtifactSet, GuiBackend, GuiCommandRunner, GuiRunOptions,
    GuiRunStatus, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{fs, path::PathBuf, time::Duration};

fn assert_saved_file(state: &serde_json::Value, file: &std::path::Path) {
    assert_eq!(state["read-coding"], "japanese-shift-jis-unix");
    assert_eq!(state["saved-coding"], "japanese-shift-jis-unix", "{state}");
    let expected = [
        0x8a, 0xbf, 0x8e, 0x9a, 0x87, 0x40, 0x0a, 0x61, 0x62, 0x63, 0x0a,
    ];
    assert_eq!(state["bytes"], serde_json::json!(expected));
    assert_eq!(fs::read(file).unwrap(), expected);
}

#[test]
fn shift_jis_save_keeps_original_encoding_through_interactive_command() {
    let root = neomacs_infra::workspace_root();
    let artifacts = root.join(format!("tmp/coding-save-{}", std::process::id()));
    let fixture = root.join("crates/neomacs-gui-tests/fixtures/coding-save/init.el");
    let backend = match std::env::var("NEOMACS_GUI_TEST_BACKEND").as_deref() {
        Ok("wayland") => GuiBackend::LinuxWayland,
        _ => GuiBackend::LinuxX11,
    };
    let gnu_session = DisplayHarness::for_backend(GuiBackend::LinuxX11)
        .start_session(&artifacts.join("gnu"))
        .unwrap();
    let state = artifacts.join("gnu-state.json");
    let mut command = CommandSpec {
        program: std::env::var_os("NEOMACS_GNU_EMACS_BINARY")
            .map(PathBuf::from)
            .unwrap_or_else(|| "emacs".into()),
        args: vec!["-Q".into(), "--load".into(), fixture.display().to_string()],
        env: vec![
            ("GDK_BACKEND".into(), "x11".into()),
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
    assert_saved_file(&gnu, &artifacts.join("cp932.txt"));
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
        GuiScenario::new("coding-save", &fixture),
    )
    .with_program(binary)
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
    assert_saved_file(
        &measurements,
        &result
            .artifacts
            .gui_state
            .parent()
            .unwrap()
            .join("cp932.txt"),
    );
    assert!(result.png_bytes.unwrap_or_default() > 0);
}
