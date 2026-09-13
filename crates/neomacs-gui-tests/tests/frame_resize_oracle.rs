use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{fs, path::PathBuf, time::Duration};

#[test]
#[ignore = "requires GNU GUI Emacs and Xvfb"]
fn gnu_width_only_resize_preserves_native_height() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let artifacts = root.join("target/neomacs-gui-tests/gnu-width-resize");
    let backend = GuiBackend::LinuxX11;
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifacts)
        .unwrap();
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &artifacts,
        GuiScenario::new(
            "width-resize-oracle",
            root.join("crates/neomacs-gui-tests/fixtures/frame-resize-oracle.el"),
        ),
    )
    .with_program(
        std::env::var_os("NEOMACS_GNU_GUI_BINARY")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("emacs")),
    )
    .with_env("GDK_BACKEND", "x11");
    for (key, value) in session.env() {
        plan = plan.with_env(key, value);
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(15)),
        )
        .unwrap();
    assert!(!result.timed_out, "{result:#?}");
    assert_eq!(result.exit_code, Some(0), "{result:#?}");
    assert!(
        fs::read_to_string(result.artifacts.stderr)
            .unwrap()
            .contains("RESIZE-ORACLE columns=91")
    );
}
