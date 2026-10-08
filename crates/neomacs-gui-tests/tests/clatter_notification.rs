//! Real Clatter notifications must complete and reach the GPU presentation.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    DisplayHarness, GuiArtifactSet, GuiBackend, GuiRunOptions, GuiRunStatus, GuiScenario,
    GuiTestPlan, ProcessGuiCommandRunner,
};
use neomacs_melpa_test_support::clatter_notification::PrivateClatterService;
use neomacs_melpa_test_support::{CLATTER_PIN, EmacsRuntime, PreparedPackageSet};
use std::{
    fs, thread,
    time::{Duration, Instant},
};

#[test]
fn clatter_process_filter_notification_paints_success() {
    let root = neomacs_infra::workspace_root();
    let run_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let control = root.join(format!(
        "tmp/clatter-notification-gui-{}-{run_id}",
        std::process::id()
    ));
    fs::create_dir_all(&control).unwrap();
    let gnu = EmacsRuntime::gnu_emacs();
    let packages =
        PreparedPackageSet::from_locked_melpa(&gnu, CLATTER_PIN, "clatter-notify.el").unwrap();
    let service = PrivateClatterService::start(&control).unwrap();
    let script = service.write_client_script(&packages).unwrap();
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
    let paths = GuiArtifactSet::new(&control, backend, "clatter-notification");
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &control,
        GuiScenario::new("clatter-notification", script),
    )
    .with_program(binary)
    .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "10000")
    .with_env("NEOMACS_CLATTER_CONTROL", control.display().to_string());
    for (key, value) in service
        .environment()
        .into_iter()
        .chain(packages.process_environment())
    {
        plan = plan.with_env(
            key.to_string_lossy().into_owned(),
            value.to_string_lossy().into_owned(),
        );
    }
    for (key, value) in session.env() {
        plan = plan.with_env(key.clone(), value.clone());
    }
    let (run, presentation) = thread::scope(|scope| {
        let run = scope.spawn(|| {
            plan.run_with(
                &mut ProcessGuiCommandRunner,
                GuiRunOptions::with_timeout(Duration::from_secs(120)),
            )
        });
        let deadline = Instant::now() + Duration::from_secs(110);
        let mut presentation =
            Err("Clatter notification did not produce a green presentation".to_owned());
        while !run.is_finished() && Instant::now() < deadline {
            if let Ok(bytes) = fs::read(control.join("client.json"))
                && let Ok(state) = serde_json::from_slice::<serde_json::Value>(&bytes)
                && state["passed"] == false
            {
                fs::write(control.join("painted"), "failed").unwrap();
                presentation = Err(format!("Clatter notification failed: {state}"));
                break;
            }
            if control.join("client.json").exists()
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
        serde_json::from_slice(&fs::read(control.join("client.json")).unwrap()).unwrap();
    assert_eq!(state["passed"], true, "Clatter notification: {state}");
    assert_eq!(
        service.notification().unwrap(),
        PrivateClatterService::expected_notification()
    );
    let snapshot = fs::read_to_string(control.join("final.json")).unwrap();
    assert!(
        snapshot.contains("*clatter-notification*"),
        "successful notification must paint its buffer"
    );
}
