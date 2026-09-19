use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{fs, path::PathBuf, time::Duration};

#[cfg(target_os = "linux")]
#[test]
// Prerequisites: requires release binary/pdump, Vulkan loader, readable procfs wait channels, and Weston.
fn display_loss_exits_while_gpu_driver_discovery_is_pending() {
    check_pending_gpu(GpuStartupAction::LoseDisplay);
}

#[cfg(target_os = "linux")]
#[test]
// Prerequisites: requires release binary/pdump, Vulkan loader, procfs wait channels, Xvfb and xdotool.
fn closing_window_exits_successfully_while_gpu_discovery_is_pending() {
    check_pending_gpu(GpuStartupAction::CloseWindow);
}

#[cfg(target_os = "linux")]
#[test]
// Prerequisites: requires release binary/pdump, Vulkan ICD, procfs wait channels, Xvfb and xdotool.
fn resize_during_gpu_discovery_reaches_lisp_after_completion() {
    check_pending_gpu(GpuStartupAction::ResizeAndComplete);
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy, strum::AsRefStr)]
#[strum(serialize_all = "kebab-case")]
enum GpuStartupAction {
    LoseDisplay,
    CloseWindow,
    ResizeAndComplete,
}

#[cfg(target_os = "linux")]
fn check_pending_gpu(action: GpuStartupAction) {
    use std::{
        os::unix::fs::OpenOptionsExt,
        process::{Command, Stdio},
        time::Instant,
    };

    let root = neomacs_infra::workspace_root();
    let artifacts = root.join("target/neomacs-gui-tests").join(format!(
        "startup-pending-gpu-{}-{}",
        action.as_ref(),
        std::process::id()
    ));
    fs::create_dir_all(&artifacts).unwrap();
    // The Vulkan loader classifies paths without .json as directories. Its
    // manifest reader blocks in fopen before checking the file's size.
    let manifest = artifacts.join("pending-driver.json");
    assert!(
        Command::new("mkfifo")
            .arg(&manifest)
            .status()
            .unwrap()
            .success()
    );
    let backend = match action {
        GpuStartupAction::LoseDisplay => GuiBackend::LinuxWayland,
        GpuStartupAction::CloseWindow | GpuStartupAction::ResizeAndComplete => GuiBackend::LinuxX11,
    };
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifacts)
        .unwrap();
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let mut command = Command::new(binary);
    command
        .current_dir(&root)
        .arg("-Q")
        .env(
            "WINIT_UNIX_BACKEND",
            if matches!(action, GpuStartupAction::LoseDisplay) {
                "wayland"
            } else {
                "x11"
            },
        )
        .env("WGPU_BACKEND", "vulkan")
        .env("VK_DRIVER_FILES", &manifest)
        .env("NEOMACS_GUI_GPU_READY_FILE", artifacts.join("lisp-ready"))
        .env("RUST_LOG", "info")
        .stdout(fs::File::create(artifacts.join("stdout.log")).unwrap())
        .stderr(fs::File::create(artifacts.join("stderr.log")).unwrap())
        .stdin(Stdio::null());
    let driver = if matches!(action, GpuStartupAction::ResizeAndComplete) {
        let driver = std::env::var_os("NEOMACS_GUI_VULKAN_ICD")
            .map(PathBuf::from)
            .or_else(|| {
                [
                    "/run/opengl-driver/share/vulkan/icd.d/lvp_icd.x86_64.json",
                    "/usr/share/vulkan/icd.d/lvp_icd.x86_64.json",
                ]
                .into_iter()
                .map(PathBuf::from)
                .find(|path| path.is_file())
            })
            .expect("set NEOMACS_GUI_VULKAN_ICD to a usable native driver manifest");
        command
            .env(
                "VK_DRIVER_FILES",
                std::env::join_paths([&manifest, &driver]).unwrap(),
            )
            .arg("-l")
            .arg(root.join("crates/neomacs-gui-tests/fixtures/startup-gpu-resize.el"));
        Some(driver)
    } else {
        if matches!(action, GpuStartupAction::LoseDisplay) {
            command.args(["--eval", r#"(while t (sleep-for 1))"#]);
        }
        None
    };
    for (key, value) in session.env() {
        command.env(key, value);
    }
    let mut child = command.spawn().unwrap();
    let result = (|| -> Result<(), String> {
        let tasks = PathBuf::from(format!("/proc/{}/task", child.id()));
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            // Unlike opening a FIFO writer, reading wchan cannot release the
            // loader's blocked open. This process has only our GPU FIFO.
            let waiting = fs::read_dir(&tasks)
                .map_err(|error| error.to_string())?
                .filter_map(Result::ok)
                .any(|task| {
                    fs::read_to_string(task.path().join("wchan"))
                        .is_ok_and(|channel| channel.trim() == "wait_for_partner")
                });
            if waiting {
                break;
            }
            if Instant::now() >= deadline {
                return Err("GPU manifest open was not observed in procfs".into());
            }
            if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
                return Err(format!(
                    "process exited before GPU discovery blocked: {status}"
                ));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        if matches!(action, GpuStartupAction::LoseDisplay) {
            drop(session);
        } else {
            if matches!(action, GpuStartupAction::ResizeAndComplete) {
                let deadline = Instant::now() + Duration::from_secs(25);
                while !artifacts.join("lisp-ready").is_file() {
                    if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
                        return Err(format!(
                            "Lisp exited before selecting its child frame: {status}"
                        ));
                    }
                    if Instant::now() >= deadline {
                        return Err("Lisp did not select its child during GPU discovery".into());
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
            let mut search = Command::new("xdotool");
            search.args(["search", "--pid", &child.id().to_string()]);
            for (key, value) in session.env() {
                search.env(key, value);
            }
            let found = search.output().map_err(|error| error.to_string())?;
            let found = String::from_utf8_lossy(&found.stdout);
            let window = found
                .lines()
                .next()
                .ok_or("no native window owned by test child")?;
            let mut change = Command::new("xdotool");
            match action {
                GpuStartupAction::CloseWindow => {
                    change.args(["windowclose", window]);
                }
                GpuStartupAction::ResizeAndComplete => {
                    change.args(["windowsize", window, "901", "603"]);
                }
                GpuStartupAction::LoseDisplay => unreachable!(),
            }
            for (key, value) in session.env() {
                change.env(key, value);
            }
            if !change
                .status()
                .map_err(|error| error.to_string())?
                .success()
            {
                return Err("xdotool could not change the owned native window".into());
            }
            if let Some(driver) = &driver {
                // Release the deliberately invalid first manifest, then let
                // the loader continue to the real ICD. Future discovery sees
                // a regular manifest at both paths.
                let writer = fs::OpenOptions::new()
                    .write(true)
                    .custom_flags(libc::O_NONBLOCK)
                    .open(&manifest)
                    .map_err(|error| error.to_string())?;
                let replacement = manifest.with_extension("replacement");
                fs::copy(driver, &replacement).map_err(|error| error.to_string())?;
                fs::rename(replacement, &manifest).map_err(|error| error.to_string())?;
                drop(writer);
            }
            // Keep this private X server alive until the process has exited.
            let _session = session;
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
                    return if status.success() {
                        Ok(())
                    } else {
                        Err(format!("native startup action exited with {status}"))
                    };
                }
                if Instant::now() >= deadline {
                    return Err("native startup action did not finish".into());
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
                return if status.success() {
                    Err("display loss during GPU startup incorrectly succeeded".into())
                } else {
                    Ok(())
                };
            }
            if Instant::now() >= deadline {
                return Err("display loss left native dispatch blocked in GPU discovery".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    })();
    // Always reap this child, including failures to observe its wait channel.
    let _ = child.kill();
    let _ = child.wait();
    fs::remove_file(manifest).unwrap();
    if result.is_ok() && matches!(action, GpuStartupAction::ResizeAndComplete) {
        assert!(
            fs::read_to_string(artifacts.join("stderr.log"))
                .unwrap()
                .contains("GPU-RESIZE-PASS")
        );
    }
    assert!(
        result.is_ok(),
        "{}; evidence: {}",
        result.unwrap_err(),
        artifacts.display()
    );
}

#[cfg(target_os = "linux")]
#[test]
// Prerequisites: requires release binary/pdump, Fontconfig, and Weston.
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
// Prerequisites: requires release binary/pdump, Fontconfig, and Weston.
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

    let root = neomacs_infra::workspace_root();
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
// Prerequisites: requires a release binary and Weston.
fn exit_during_lisp_startup_preserves_the_requested_status() {
    check_lisp_startup_exit_status(23);
}

#[test]
// Prerequisites: requires a release binary and Weston.
fn successful_exit_during_lisp_startup_remains_successful() {
    check_lisp_startup_exit_status(0);
}

fn check_lisp_startup_exit_status(status: i32) {
    let root = neomacs_infra::workspace_root();
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
// Prerequisites: requires a release binary and Weston.
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
// Prerequisites: requires release binary/pdump, Fontconfig, and Weston.
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
