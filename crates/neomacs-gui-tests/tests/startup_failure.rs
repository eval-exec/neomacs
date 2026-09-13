use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{fs, path::PathBuf, time::Duration};

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires release binary/pdump, Fontconfig, and Weston"]
fn display_loss_exits_while_initial_font_loading_is_pending() {
    let result = run_with_pending_font(PendingFontAction::LoseDisplay);
    assert!(
        !result.timed_out,
        "display loss left startup blocked: {result:#?}"
    );
    assert!(
        result.exit_code.is_some_and(|code| code != 0),
        "{result:#?}"
    );
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires release binary/pdump, Fontconfig, and Weston"]
fn completing_pending_font_loading_starts_the_gui() {
    let result = run_with_pending_font(PendingFontAction::Complete);
    assert!(!result.timed_out, "{result:#?}");
    assert_eq!(result.exit_code, Some(0), "{result:#?}");
    let state: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&result.artifacts.gui_state).unwrap()).unwrap();
    assert_eq!(state["buffer_name"], "*neomacs-gui-smoke*");
    assert!(
        result.artifacts.png.exists(),
        "no rendered GUI: {result:#?}"
    );
}

#[cfg(target_os = "linux")]
enum PendingFontAction {
    LoseDisplay,
    Complete,
}

#[cfg(target_os = "linux")]
fn run_with_pending_font(action: PendingFontAction) -> neomacs_gui_tests::GuiRunResult {
    use std::{io::Write, os::unix::fs::OpenOptionsExt, process::Command, time::Instant};

    let root = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
    let artifacts = root.join("target/neomacs-gui-tests");
    fs::create_dir_all(&artifacts).unwrap();
    let scenario = match action {
        PendingFontAction::LoseDisplay => "startup-display-loss-pending-font",
        PendingFontAction::Complete => "startup-complete-pending-font",
    };
    let fifo = artifacts.join(format!("{scenario}-{}.fifo", std::process::id()));
    assert!(
        Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
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
            scenario,
            root.join("crates/neomacs-gui-tests/fixtures/startup-smoke.el"),
        ),
    )
    .with_program(binary)
    .with_env("FONTCONFIG_FILE", fifo.display().to_string());
    for (key, value) in session.env() {
        plan = plan.with_env(key.clone(), value.clone());
    }

    // These assertions must observe this run, never a previous smoke artifact.
    let outputs = neomacs_gui_tests::GuiArtifactSet::new(&artifacts, backend, scenario);
    for path in [&outputs.png, &outputs.gui_state] {
        if let Err(error) = fs::remove_file(path) {
            assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
        }
    }
    let result = std::thread::scope(|scope| {
        let run = scope.spawn(|| {
            plan.run_with(
                &mut ProcessGuiCommandRunner,
                GuiRunOptions::with_timeout(Duration::from_secs(20)),
            )
            .unwrap()
        });
        // A FIFO writer can open only after Fontconfig has opened its reader.
        // Holding it open without bytes keeps the real native file read pending;
        // no production delay, test flag, or startup-log timing is involved.
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut writer = loop {
            match fs::OpenOptions::new()
                .write(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(&fifo)
            {
                Ok(writer) => break writer,
                Err(error) if error.raw_os_error() == Some(libc::ENXIO) => {
                    assert!(
                        Instant::now() < deadline,
                        "Fontconfig never opened the FIFO"
                    );
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("opening font configuration FIFO: {error}"),
            }
        };
        match action {
            PendingFontAction::LoseDisplay => {
                // Only this test's private compositor is terminated. A responsive
                // native loop must observe loss before a window even exists.
                drop(session);
                let result = run.join().unwrap();
                drop(writer);
                result
            }
            PendingFontAction::Complete => {
                let config = b"<?xml version=\"1.0\"?><!DOCTYPE fontconfig SYSTEM \"fonts.dtd\"><fontconfig><include>/etc/fonts/fonts.conf</include></fontconfig>";
                // Later font users may reopen the configuration. Replace the
                // path first; the evaluator retains its already-open FIFO read.
                let replacement = fifo.with_extension("conf");
                fs::write(&replacement, config).unwrap();
                fs::rename(&replacement, &fifo).unwrap();
                writer.write_all(config).unwrap();
                drop(writer);
                run.join().unwrap()
            }
        }
    });
    fs::remove_file(&fifo).unwrap();
    result
}

#[test]
#[ignore = "requires a release binary and Weston"]
fn exit_during_lisp_startup_preserves_the_requested_status() {
    check_lisp_startup_exit_status(23);
}

#[test]
#[ignore = "requires a release binary and Weston"]
fn successful_exit_during_lisp_startup_remains_successful() {
    check_lisp_startup_exit_status(0);
}

fn check_lisp_startup_exit_status(status: i32) {
    let root = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
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
            format!("startup-explicit-exit-{status}"),
            root.join("crates/neomacs-gui-tests/fixtures/startup-smoke.el"),
        ),
    )
    .with_program(binary)
    .with_args([
        "-Q".into(),
        "--eval".into(),
        format!("(kill-emacs {status})"),
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
    assert_eq!(result.exit_code, Some(status), "{result:#?}");
}

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
