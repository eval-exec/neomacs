//! Runtime charset preferences govern GUI file opening and saving.
#![cfg(target_os = "linux")]
use neomacs_gui_tests::{
    CommandSpec, DisplayHarness, GuiArtifactSet, GuiBackend, GuiCommandRunner, GuiRunOptions,
    GuiRunStatus, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{fs, path::PathBuf, time::Duration};

fn assert_detected_files(state: &serde_json::Value, directory: &std::path::Path) {
    let cases = state["cases"].as_array().expect("reported file cases");
    let expected: [(&str, &str, &[u32], &[u8]); 3] = [
        (
            "gbk.txt",
            "chinese-gbk-unix",
            &[21872, 21990, 10],
            &[0x86, 0xaa, 0xe0, 0xc2, 0x0a],
        ),
        (
            "gbk-control.txt",
            "chinese-gbk-unix",
            &[20013, 25991, 38229, 10],
            &[0xd6, 0xd0, 0xce, 0xc4, 0xe9, 0x46, 0x0a],
        ),
        (
            "windows-1252.txt",
            "windows-1252-unix",
            &[8364, 32, 49, 48, 10],
            &[0x80, 0x20, 0x31, 0x30, 0x0a],
        ),
    ];
    assert_eq!(cases.len(), expected.len());
    for (case, (file, coding, characters, initial)) in cases.iter().zip(expected) {
        assert_eq!(case["file"], file);
        assert_eq!(case["read-coding"], coding);
        assert_eq!(case["saved-coding"], coding);
        assert_eq!(case["characters"], serde_json::json!(characters));
        let mut bytes = initial.to_vec();
        bytes.extend_from_slice(b"abc\n");
        assert_eq!(case["bytes"], serde_json::json!(bytes));
        assert_eq!(fs::read(directory.join(file)).unwrap(), bytes);
    }
}

#[test]
fn preferred_gbk_and_windows_1252_files_match_gnu_in_gui() {
    let root = neomacs_infra::workspace_root();
    let artifacts = root.join(format!("tmp/coding-detection-{}", std::process::id()));
    let fixture = root.join("crates/neomacs-gui-tests/fixtures/coding-detection/init.el");
    let backend = match std::env::var("NEOMACS_GUI_TEST_BACKEND").as_deref() {
        Ok("wayland") => GuiBackend::LinuxWayland,
        _ => GuiBackend::LinuxX11,
    };
    let gnu_session = DisplayHarness::for_backend(GuiBackend::LinuxX11)
        .start_session(&artifacts.join("gnu"))
        .unwrap();
    let state = artifacts.join("gnu-state.json");
    let mut command = CommandSpec {
        program: std::env::var_os("NEOMACS_GUI_TEST_GNU_EMACS")
            .or_else(|| std::env::var_os("NEOMACS_GNU_EMACS_BINARY"))
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
    assert_detected_files(&gnu, &artifacts);
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
        GuiScenario::new("coding-detection", &fixture),
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
    assert_detected_files(&measurements, &result.artifacts.gui_state.parent().unwrap());
    assert!(result.png_bytes.unwrap_or_default() > 0);
}
