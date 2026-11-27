//! #889: a GUI start with an empty clipboard logs no clipboard WARN.
//!
//! Before Neomacs's first seat event, or while unfocused, a Wayland compositor
//! has not sent this client the selection. smithay-clipboard reports that as
//! "no events received on any seat". It is a normal state, an absence, not a
//! failed read, so a plain start must not print
//! `WARN ... clipboard read failed`. A real transport failure still WARNs;
//! the clipboard policy unit rows pin that. The row runs at RUST_LOG=warn,
//! the default filter a user sees.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{fs, path::PathBuf, time::Duration};

fn startup_logs_no_clipboard_warn(backend: GuiBackend) {
    let root = neomacs_infra::workspace_root();
    let artifacts = root
        .join("target/neomacs-gui-tests/startup-clipboard-quiet")
        .join(backend.slug());
    let _ = fs::remove_dir_all(&artifacts);
    fs::create_dir_all(&artifacts).unwrap();
    let fixture = root.join("crates/neomacs-gui-tests/fixtures/startup-clipboard-quiet.el");
    let neo = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifacts.join("session"))
        .unwrap();
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &artifacts,
        GuiScenario::new("startup-clipboard-quiet", &fixture),
    )
    .without_paint_proof()
    .with_program(neo)
    .with_args([
        "-Q".to_string(),
        "-l".to_string(),
        fixture.display().to_string(),
    ])
    .with_env("GSETTINGS_BACKEND", "memory")
    // The default filter a user sees (env_filter_spec(None) is warn-based).
    .with_env("RUST_LOG", "warn");
    for (key, value) in session.env() {
        plan = plan.with_env(key, value);
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            // Explicit hang-avoidance cap only; the fixture exits after 2 s.
            GuiRunOptions::with_timeout(Duration::from_secs(30)),
        )
        .unwrap();
    let stderr = fs::read_to_string(&result.artifacts.stderr).unwrap_or_default();
    let stdout = fs::read_to_string(&result.artifacts.stdout).unwrap_or_default();
    let log = format!("{stdout}{stderr}");
    assert!(
        !result.timed_out && result.exit_code == Some(0),
        "{result:#?}\n{log}"
    );
    assert!(
        log.contains("STARTUP-CLIPBOARD nil"),
        "the empty clipboard reads as nil:\n{log}"
    );
    let warns: Vec<&str> = log
        .lines()
        .filter(|line| line.contains("clipboard read failed"))
        .collect();
    assert!(
        warns.is_empty(),
        "an empty or unseen selection logged a clipboard WARN:\n{}",
        warns.join("\n")
    );
}

#[test]
fn startup_logs_no_clipboard_warn_on_wayland() {
    startup_logs_no_clipboard_warn(GuiBackend::LinuxWayland);
}

#[test]
fn startup_logs_no_clipboard_warn_on_x11() {
    startup_logs_no_clipboard_warn(GuiBackend::LinuxX11);
}
