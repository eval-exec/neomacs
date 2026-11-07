//! Real command-input and small-font newline regressions, including macOS.
#![cfg(any(target_os = "linux", target_os = "macos"))]

use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{fs, path::PathBuf, time::Duration};

/// Honor explicit compositor selection, defaulting to the host's native backend.
fn commit_probe_backend(configured: Option<&str>) -> GuiBackend {
    match configured {
        Some("wayland" | "linux-wayland") => GuiBackend::LinuxWayland,
        Some("x11" | "linux-x11") => GuiBackend::LinuxX11,
        Some("macos") => GuiBackend::Macos,
        None if cfg!(target_os = "macos") => GuiBackend::Macos,
        None => GuiBackend::LinuxX11,
        Some(other) => panic!("unsupported NEOMACS_GUI_TEST_BACKEND={other:?}"),
    }
}

/// Run a native commit-editing fixture and require its explicit success marker.
fn run_commit_probe(probe: &str) {
    let root = neomacs_infra::crate_root!().join("../..");
    let configured = std::env::var("NEOMACS_GUI_TEST_BACKEND").ok();
    let backend = commit_probe_backend(configured.as_deref());
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

/// Backend aliases and platform defaults must not require a live compositor.
#[test]
fn commit_probe_respects_requested_backend() {
    for name in ["wayland", "linux-wayland"] {
        assert_eq!(commit_probe_backend(Some(name)), GuiBackend::LinuxWayland);
    }
    for name in ["x11", "linux-x11"] {
        assert_eq!(commit_probe_backend(Some(name)), GuiBackend::LinuxX11);
    }
    assert_eq!(commit_probe_backend(Some("macos")), GuiBackend::Macos);
    assert_eq!(
        commit_probe_backend(None),
        if cfg!(target_os = "macos") {
            GuiBackend::Macos
        } else {
            GuiBackend::LinuxX11
        }
    );
}

#[test]
#[should_panic(expected = "unsupported NEOMACS_GUI_TEST_BACKEND")]
fn commit_probe_rejects_unknown_backend() {
    commit_probe_backend(Some("unknown"));
}

#[test]
fn first_commit_character_uses_the_asynchronously_selected_buffer() {
    run_commit_probe("async-map");
}

#[test]
fn delayed_timer_service_waits_for_commit_commands() {
    run_commit_probe("async-map-delayed");
}

#[test]
fn overlay_separator_uses_its_small_font_height() {
    run_commit_probe("overlay-separator");
}

#[test]
fn log_edit_separator_uses_its_small_font_height() {
    run_commit_probe("separator");
}
