use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{fs, path::PathBuf, time::Duration};

#[test]
#[ignore = "requires a release binary and Weston"]
fn evaluator_image_failure_exits_instead_of_waiting_for_initial_window() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let artifacts = root.join("target/neomacs-gui-tests");
    let missing_image = artifacts.join("startup-failure-intentionally-missing.pdump");
    assert!(!missing_image.exists(), "test requires an absent image");
    let backend = GuiBackend::LinuxWayland;
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifacts)
        .unwrap();
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &artifacts,
        GuiScenario::new(
            "startup-image-failure",
            root.join("crates/neomacs-gui-tests/fixtures/desktop-font-startup.el"),
        ),
    )
    .with_program(binary)
    // Override the scenario argv: startup must fail before any Lisp fixture
    // or native window can be entered, and must retain the original error.
    .with_args([
        "-Q".to_owned(),
        "--dump-file".to_owned(),
        missing_image.display().to_string(),
    ]);
    for (key, value) in session.env() {
        plan = plan.with_env(key.clone(), value.clone());
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(15)),
        )
        .unwrap();
    assert!(!result.timed_out, "{result:#?}");
    assert!(
        result.exit_code.is_some_and(|code| code != 0),
        "{result:#?}"
    );
    let stdout = fs::read_to_string(&result.artifacts.stdout).unwrap();
    let stderr = fs::read_to_string(&result.artifacts.stderr).unwrap();
    assert!(
        stdout.contains("final image should load") || stderr.contains("final image should load"),
        "original evaluator failure was lost: {stdout}\n{stderr}",
    );
}

#[test]
#[ignore = "requires release binary/pdump, Fontconfig, and Weston"]
fn empty_native_font_catalog_reports_startup_failure_without_a_window() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let artifacts = root.join("target/neomacs-gui-tests");
    let backend = GuiBackend::LinuxWayland;
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifacts)
        .unwrap();
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &artifacts,
        GuiScenario::new(
            "startup-no-fonts",
            root.join("crates/neomacs-gui-tests/fixtures/desktop-font-startup.el"),
        ),
    )
    .with_program(binary)
    .with_args(["-Q"])
    .with_env(
        "FONTCONFIG_FILE",
        root.join("crates/neomacs-gui-tests/fixtures/no-system-fonts.conf")
            .display()
            .to_string(),
    );
    for (key, value) in session.env() {
        plan = plan.with_env(key.clone(), value.clone());
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(15)),
        )
        .unwrap();
    assert!(!result.timed_out, "{result:#?}");
    assert_eq!(result.exit_code, Some(1), "{result:#?}");
    let stdout = fs::read_to_string(&result.artifacts.stdout).unwrap();
    let stderr = fs::read_to_string(&result.artifacts.stderr).unwrap();
    assert!(
        stdout.contains("no usable initial GUI font was found")
            || stderr.contains("no usable initial GUI font was found"),
        "missing font-catalog diagnostic: {stdout}\n{stderr}",
    );
}
