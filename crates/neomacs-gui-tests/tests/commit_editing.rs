//! Real command-input and small-font newline regressions, including macOS.
#![cfg(any(target_os = "linux", target_os = "macos"))]

use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{fs, path::PathBuf, time::Duration};

fn run_commit_probe(probe: &str) {
    let root = neomacs_infra::crate_root!().join("../..");
    let backend = if cfg!(target_os = "macos") {
        GuiBackend::Macos
    } else {
        GuiBackend::LinuxX11
    };
    let artifacts = root
        .join("target/neomacs-gui-tests")
        .join(format!("commit-{probe}"));
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifacts)
        .unwrap();
    let program = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &artifacts,
        GuiScenario::new(
            probe,
            root.join("crates/neomacs-gui-tests/fixtures/commit-editing.el"),
        ),
    )
    .with_program(program)
    .with_env("NEOMACS_COMMIT_PROBE", probe)
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
    assert!(stderr.contains("COMMIT-EDITING-PASS"), "{stderr}");
}

#[test]
fn first_commit_character_uses_the_asynchronously_selected_buffer() {
    run_commit_probe("async-map");
}

#[test]
fn log_edit_separator_uses_its_small_font_height() {
    run_commit_probe("separator");
}
