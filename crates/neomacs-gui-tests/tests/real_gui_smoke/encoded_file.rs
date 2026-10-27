//! Load raw compiled bytes, then prove native input reaches a painted buffer.
#![cfg(target_os = "linux")]
use neomacs_gui_tests::{
    DisplayHarness, GuiArtifactSet, GuiBackend, GuiRunOptions, GuiRunStatus, GuiScenario,
    GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{
    fs,
    process::Command,
    thread,
    time::{Duration, Instant},
};

#[test]
fn compiled_file_decoding_survives_native_input_and_presents_success() {
    let Some(backend) = super::requested_backend() else {
        eprintln!("skipping encoded-file GUI test; set NEOMACS_GUI_TEST_BACKEND=x11");
        return;
    };
    if backend != GuiBackend::LinuxX11 {
        eprintln!("encoded-file native input test requires its isolated X11 driver");
        return;
    }
    let root = super::workspace_root();
    let run_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let control = root.join(format!(
        "tmp/neomacs-gui-tests/encoded-file-{}-{run_id}",
        std::process::id()
    ));
    fs::create_dir_all(&control).unwrap();
    let session = DisplayHarness::for_backend(backend)
        .start_session(&control)
        .unwrap();
    let paths = GuiArtifactSet::new(&control, backend, "encoded-file");
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &control,
        GuiScenario::new(
            "encoded-file",
            root.join("crates/neomacs-gui-tests/fixtures/encoded-file.el"),
        ),
    )
    .with_program(super::neomacs_binary(&root))
    .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "10000")
    .with_env(
        "NEOMACS_ENCODED_FILE_CONTROL",
        control.display().to_string(),
    )
    .with_env(
        "NEOMACS_ENCODED_FILE_FIXTURES",
        root.join("crates/neovm-oracle-tests/fixtures/encoded-file")
            .display()
            .to_string(),
    );
    for (key, value) in session.env() {
        plan = plan.with_env(key.clone(), value.clone());
    }
    let (run, presentation) = thread::scope(|scope| {
        let run = scope.spawn(|| {
            plan.run_with(
                &mut ProcessGuiCommandRunner,
                GuiRunOptions::with_timeout(Duration::from_secs(40)),
            )
        });
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut sent = false;
        let mut presentation = Err("compiled-file checks did not reach presentation".to_owned());
        while !run.is_finished() && Instant::now() < deadline {
            if control.join("ready").exists() && !sent {
                // Lisp frame-title-format need not be the native window title.
                // The ready marker identifies the process that finished loading.
                let pid = fs::read_to_string(control.join("ready")).unwrap();
                let windows = Command::new("xdotool")
                    .args(["search", "--onlyvisible", "--pid", pid.trim()])
                    .envs(session.env().iter().cloned())
                    .output()
                    .expect("xdotool must be installed for the X11 input test");
                if windows.status.success()
                    && let Some(window) = String::from_utf8_lossy(&windows.stdout).lines().next()
                {
                    let focused = Command::new("xdotool")
                        .args(["windowfocus", "--sync", window])
                        .envs(session.env().iter().cloned())
                        .output()
                        .unwrap();
                    assert!(focused.status.success(), "{focused:?}");
                    let typed = Command::new("xdotool")
                        .args(["type", "--clearmodifiers", "x"])
                        .envs(session.env().iter().cloned())
                        .output()
                        .unwrap();
                    assert!(typed.status.success(), "{typed:?}");
                    sent = true;
                }
            }
            if let Ok(bytes) = fs::read(control.join("result.json"))
                && let Ok(state) = serde_json::from_slice::<serde_json::Value>(&bytes)
            {
                if state["passed"] == false {
                    fs::write(control.join("painted"), "failed").unwrap();
                    presentation = Err(format!("compiled-file check failed: {state}"));
                    break;
                }
                if sent && let Ok(png) = image::open(&paths.png) {
                    let green = png
                        .to_rgba8()
                        .pixels()
                        .filter(|pixel| {
                            let [r, g, b, _] = pixel.0;
                            g > 240 && r < 15 && b < 15
                        })
                        .count();
                    if green > 3000 {
                        fs::write(control.join("painted"), "painted").unwrap();
                        presentation = Ok(());
                        break;
                    }
                }
            }
            thread::sleep(Duration::from_millis(25));
        }
        (run.join().unwrap(), presentation)
    });
    assert!(
        presentation.is_ok(),
        "{presentation:?}; artifacts {control:?}"
    );
    let result = run.unwrap();
    assert!(!result.timed_out, "{result:#?}");
    assert_eq!(result.exit_code, Some(0), "{result:#?}");
    assert_eq!(result.status, GuiRunStatus::Passed, "{result:#?}");
    let state: serde_json::Value =
        serde_json::from_slice(&fs::read(control.join("result.json")).unwrap()).unwrap();
    assert_eq!(state["passed"], true, "{state}");
    assert_eq!(state["typed"], "x", "{state}");
    let snapshot = fs::read_to_string(&result.artifacts.frame_snapshot_txt).unwrap();
    assert!(snapshot.contains("*encoded file*"), "{snapshot}");
    assert!(snapshot.contains("ENCODED-FILE-PASSED"), "{snapshot}");
    assert!(result.png_bytes.unwrap_or_default() > 0);
}
