//! EOB overlays through the native editor, compared with an initialized GNU GUI.
use neomacs_gui_tests::{
    CommandSpec, DisplayHarness, GuiArtifactSet, GuiBackend, GuiCommandRunner, GuiRunOptions,
    GuiRunStatus, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{path::PathBuf, time::Duration};

#[test]
fn motion_from_accessible_eob_with_virtual_overlay_rows_matches_gnu() {
    compare("eob");
}

#[test]
fn motion_into_eob_overlay_from_source_line_preserves_gnu_distance() {
    compare("bol");
}

fn compare(origin: &str) {
    let backend = match std::env::var("NEOMACS_GUI_TEST_BACKEND").as_deref() {
        Ok("x11") => GuiBackend::LinuxX11,
        Ok("wayland") => GuiBackend::LinuxWayland,
        Ok("macos") => GuiBackend::Macos,
        Ok("windows") => GuiBackend::Windows,
        _ => cfg_select! {
            target_os = "macos" => { GuiBackend::Macos }
            windows => { GuiBackend::Windows }
            _ => { GuiBackend::LinuxX11 }
        },
    };
    let root = neomacs_infra::workspace_root();
    let artifacts = root.join("tmp/issue602/gui-motion").join(origin);
    let fixture = root.join("crates/neomacs-gui-tests/fixtures/eob-overlay-motion.el");
    let oracle_backend = cfg_select! {
        target_os = "macos" => { GuiBackend::Macos }
        windows => { GuiBackend::Windows }
        _ => { GuiBackend::LinuxX11 }
    };
    let oracle_session = DisplayHarness::for_backend(oracle_backend)
        .start_session(artifacts.join("gnu"))
        .unwrap();
    let state = artifacts.join("gnu-state.json");
    let mut command = CommandSpec {
        program: std::env::var_os("NEOMACS_GNU_EMACS_BINARY")
            .map(PathBuf::from)
            .unwrap_or_else(|| "emacs".into()),
        args: vec!["-Q".into(), "--load".into(), fixture.display().to_string()],
        env: vec![
            ("GDK_BACKEND".into(), "x11".into()),
            ("GSETTINGS_BACKEND".into(), "memory".into()),
            ("NEOMACS_GUI_STATE_JSON".into(), state.display().to_string()),
            ("NEOMACS_MOTION_PROBE_ORIGIN".into(), origin.into()),
        ],
    };
    command.env.extend(oracle_session.env().iter().cloned());
    let output = ProcessGuiCommandRunner
        .run(
            &command,
            &GuiArtifactSet::new(artifacts.join("gnu"), oracle_backend, "motion"),
            &GuiRunOptions::with_timeout(Duration::from_secs(30)),
        )
        .unwrap();
    assert!(!output.timed_out, "GNU: {output:#?}");
    assert_eq!(output.exit_code, Some(0), "GNU: {output:#?}");
    let expected: serde_json::Value =
        serde_json::from_slice(&std::fs::read(state).unwrap()).unwrap();
    assert_eq!(expected["observations"].as_array().unwrap().len(), 16);
    assert!(
        expected["observations"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| !v.as_str().unwrap().contains("(error ")),
        "GNU: {expected}"
    );
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
        GuiScenario::new("eob-overlay-motion", fixture),
    )
    .with_program(binary)
    .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "10000")
    .with_env("NEOMACS_MOTION_PROBE_ORIGIN", origin);
    for (key, value) in session.env() {
        plan = plan.with_env(key.clone(), value.clone());
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(30)),
        )
        .unwrap();
    assert_eq!(result.status, GuiRunStatus::Passed, "{result:#?}");
    assert!(!result.timed_out, "{result:#?}");
    assert_eq!(result.exit_code, Some(0), "{result:#?}");
    assert_eq!(result.gui_state, Some(expected));
    assert!(result.png_bytes.unwrap_or_default() > 0);
    let snapshot: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&result.artifacts.frame_snapshot_json).unwrap())
            .unwrap();
    let has_overlay_x = snapshot["frames"].as_array().unwrap().iter().any(|frame| {
        frame["window_matrices"]
            .as_array()
            .unwrap()
            .iter()
            .any(|window| {
                window["matrix"]["rows"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|row| {
                        row["glyphs"].as_array().unwrap().iter().any(|area| {
                            area.as_array().unwrap().iter().any(|glyph| {
                                glyph["glyph_type"]["Char"]["ch"] == "X"
                                    && glyph["provenance"].get("Str").is_some()
                            })
                        })
                    })
            })
    });
    assert!(
        has_overlay_x,
        "motion must preserve visible overlay-string glyphs"
    );
}
