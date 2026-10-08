//! Successful accepted-process property checks must reach the GPU presentation.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    DisplayHarness, GuiArtifactSet, GuiBackend, GuiRunOptions, GuiRunStatus, GuiScenario,
    GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{
    fs, thread,
    time::{Duration, Instant},
};

#[test]
fn accepted_tcp_process_plists_isolate_properties_and_paint_success() {
    let root = neomacs_infra::workspace_root();
    let run_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let control = root.join(format!(
        "tmp/process-plist-gui-{}-{run_id}",
        std::process::id()
    ));
    fs::create_dir_all(&control).unwrap();
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let backend = match std::env::var("NEOMACS_GUI_TEST_BACKEND").as_deref() {
        Ok("wayland" | "linux-wayland") => GuiBackend::LinuxWayland,
        Ok("x11" | "linux-x11") | Err(_) => GuiBackend::LinuxX11,
        Ok(other) => panic!("unsupported GUI backend {other}"),
    };
    let session = DisplayHarness::for_backend(backend)
        .start_session(&control)
        .unwrap();
    let paths = GuiArtifactSet::new(&control, backend, "process-plist-isolation");
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &control,
        GuiScenario::new(
            "process-plist-isolation",
            root.join("crates/neomacs-gui-tests/fixtures/process-plist-isolation.el"),
        ),
    )
    .with_program(binary)
    .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "10000")
    .with_env(
        "NEOMACS_PROCESS_PLIST_CONTROL",
        control.display().to_string(),
    );
    for (key, value) in session.env() {
        plan = plan.with_env(key.clone(), value.clone());
    }
    let (run, presentation) = thread::scope(|scope| {
        let run = scope.spawn(|| {
            plan.run_with(
                &mut ProcessGuiCommandRunner,
                GuiRunOptions::with_timeout(Duration::from_secs(30)),
            )
        });
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut presentation =
            Err("process property checks did not produce a green presentation".to_owned());
        while !run.is_finished() && Instant::now() < deadline {
            if let Ok(bytes) = fs::read(control.join("result.json"))
                && let Ok(state) = serde_json::from_slice::<serde_json::Value>(&bytes)
                && state["passed"] == false
            {
                fs::write(control.join("painted"), "failed").unwrap();
                presentation = Err(format!("accepted process properties differ: {state}"));
                break;
            }
            if control.join("result.json").exists()
                && let Ok(png) = image::open(&paths.png)
            {
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
    assert_eq!(state["passed"], true, "TCP property checks: {state}");
    assert_eq!(
        state["checks"],
        serde_json::json!([
            true, true, true, true, true, true, true, true, true, true, 3
        ]),
        "accepted processes must isolate plist spines and share nested values"
    );
    let snapshot = fs::read_to_string(control.join("final.json")).unwrap();
    assert!(
        snapshot.contains("*process-plist-isolation*"),
        "property checks must paint their buffer"
    );
}
