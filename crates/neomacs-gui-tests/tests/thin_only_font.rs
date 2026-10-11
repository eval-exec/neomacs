//! A Thin-only family must support named metrics and real Vertico completion.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    DisplayHarness, GuiArtifactSet, GuiBackend, GuiRunOptions, GuiRunStatus, GuiScenario,
    GuiTestPlan, ProcessGuiCommandRunner,
};
use neomacs_melpa_test_support::{EmacsRuntime, PreparedPackageSet};
use std::{
    fs, thread,
    time::{Duration, Instant},
};

const VERTICO_PIN: (&str, &str) = ("vertico", "20260805.1129");

#[test]
fn thin_only_family_metrics_and_vertico_completion_match_gnu() {
    let root = neomacs_infra::workspace_root();
    let run_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let control = root.join(format!(
        "tmp/thin-only-font-gui-{}-{run_id}",
        std::process::id()
    ));
    fs::create_dir_all(&control).unwrap();
    let gnu = EmacsRuntime::gnu_emacs();
    let packages = PreparedPackageSet::from_locked_melpa(&gnu, VERTICO_PIN, "vertico.el").unwrap();
    let fonts = control.join("fonts");
    fs::create_dir_all(&fonts).unwrap();
    fs::copy(
        neomacs_test_fonts::mplus_1_code_thin(),
        fonts.join("MPLUS1Code-Thin.ttf"),
    )
    .unwrap();
    let config = control.join("fonts.conf");
    fs::write(
        &config,
        format!(
            "<fontconfig><dir>{}</dir><cachedir>{}</cachedir></fontconfig>",
            fonts.display(),
            control.join("fontcache").display()
        ),
    )
    .unwrap();
    let script = control.join("fixture.el");
    fs::write(
        &script,
        format!(
            "{}\n{}",
            packages.startup_elisp(),
            include_str!("../fixtures/thin-only-font-vertico.el")
        ),
    )
    .unwrap();
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
    let gnu_control = control.join("GNU");
    fs::create_dir_all(&gnu_control).unwrap();
    // The pinned GNU reference uses GTK/X11. Keep its display owned by this
    // test even when the Neomacs presentation is hosted by Weston.
    let gnu_session = (backend == GuiBackend::LinuxWayland)
        .then(|| DisplayHarness::for_backend(GuiBackend::LinuxX11).start_session(&gnu_control))
        .transpose()
        .unwrap();
    let mut command = gnu.command();
    command
        .args(["-Q", "-l"])
        .arg(&script)
        .env("NEOMACS_THIN_CONTROL", &gnu_control)
        .env("FONTCONFIG_FILE", &config)
        .env("TMPDIR", &control)
        .envs(packages.process_environment())
        .envs(
            gnu_session
                .as_ref()
                .unwrap_or(&session)
                .env()
                .iter()
                .cloned(),
        );
    let output =
        neomacs_melpa_test_support::output_with_timeout(&mut command, Duration::from_secs(30))
            .unwrap();
    fs::write(gnu_control.join("stdout"), &output.stdout).unwrap();
    fs::write(gnu_control.join("stderr"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "GNU reference failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected = serde_json::json!({"passed": true, "initial-canonical": true, "updated-canonical": true, "regular-info": true, "thin-info": true, "positive-default-height": true, "regular-thin-file": true, "thin-thin-file": true, "selection": "alpha", "candidates": ["alpha", "beta", "gamma"]});
    let gnu_result: serde_json::Value =
        serde_json::from_slice(&fs::read(gnu_control.join("result.json")).unwrap()).unwrap();
    assert_eq!(gnu_result, expected);
    let paths = GuiArtifactSet::new(&control, backend, "thin-only-font-vertico");
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &control,
        GuiScenario::new("thin-only-font-vertico", &script),
    )
    .with_program(binary)
    .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "10000")
    .with_env("RUST_LOG", "warn")
    .with_env("FONTCONFIG_FILE", config.display().to_string())
    .with_env("TMPDIR", control.display().to_string())
    .with_env("NEOMACS_THIN_CONTROL", control.display().to_string());
    for (key, value) in packages.process_environment() {
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
        let deadline = Instant::now() + Duration::from_secs(115);
        let mut presentation =
            Err("Thin-only font metrics did not produce a green presentation".to_owned());
        while !run.is_finished() && Instant::now() < deadline {
            if let Ok(error) = fs::read_to_string(control.join("error.el")) {
                presentation = Err(format!("public font/Vertico error: {error}"));
                break;
            }
            if let Ok(bytes) = fs::read(control.join("result.json"))
                && let Ok(state) = serde_json::from_slice::<serde_json::Value>(&bytes)
                && state["passed"] == false
            {
                fs::write(control.join("painted"), "failed").unwrap();
                presentation = Err(format!("Thin-only font metrics failed: {state}"));
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
    assert_eq!(state["passed"], true, "Thin-only font metrics: {state}");
    assert_eq!(
        state, expected,
        "public font metrics and real Vertico must match GNU"
    );
    let snapshot = fs::read_to_string(control.join("final.json")).unwrap();
    assert!(
        snapshot.contains("*thin-only-font-vertico*"),
        "successful completion must paint its buffer"
    );
}
