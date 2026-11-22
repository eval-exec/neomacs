#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiRunStatus, GuiScenario, GuiTestPlan,
    ProcessGuiCommandRunner,
};
use std::{path::PathBuf, process::Command, time::Duration};

#[test]
// Prerequisites: fresh binary/runtime, Weston, and DejaVu Sans Mono.
fn svg_tag_box_follows_remapped_window_font_without_scaling_its_text_font() {
    let root = neomacs_infra::crate_root!().join("../..");
    // A fresh worktree has no package cache. Provision the pinned fixtures
    // before launching the editor, while honoring explicit directory overrides.
    let packages: Vec<_> = [
        ("NEOMACS_GUI_SVG_LIB_DIR", "svg-lib"),
        ("NEOMACS_GUI_SVG_TAG_MODE_DIR", "svg-tag-mode"),
    ]
    .into_iter()
    .filter_map(|(variable, package)| std::env::var_os(variable).is_none().then_some(package))
    .collect();
    if !packages.is_empty() {
        let status = Command::new("bash")
            .arg(root.join("scripts/ci/setup-gui-test-packages.sh"))
            .args(packages)
            .current_dir(&root)
            .status()
            .expect("run pinned SVG package setup");
        assert!(
            status.success(),
            "pinned SVG package setup failed: {status}"
        );
    }
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let artifacts = root.join("target/neomacs-gui-tests");
    let backend = GuiBackend::LinuxWayland;
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifacts)
        .unwrap();
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &artifacts,
        GuiScenario::new(
            "svg-tag-font-remapping",
            root.join("crates/neomacs-gui-tests/fixtures/svg-tag-font-remapping.el"),
        ),
    )
    .with_program(binary);
    for (key, value) in session.env() {
        plan = plan.with_env(key.clone(), value.clone());
    }
    for key in ["NEOMACS_GUI_SVG_LIB_DIR", "NEOMACS_GUI_SVG_TAG_MODE_DIR"] {
        if let Ok(value) = std::env::var(key) {
            plan = plan.with_env(key, value);
        }
    }
    plan = plan.with_svg_packages();
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(20)),
        )
        .unwrap();
    assert!(!result.timed_out, "{result:#?}");
    assert_eq!(result.exit_code, Some(0), "{result:#?}");
    assert_eq!(result.status, GuiRunStatus::Passed, "{result:#?}");
}
