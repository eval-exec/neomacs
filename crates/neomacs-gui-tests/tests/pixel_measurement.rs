//! Pixel measurement must agree with each editor's real-font glyph placement.
#![cfg(target_os = "linux")]
use neomacs_gui_tests::{
    CommandSpec, DisplayHarness, GuiArtifactSet, GuiBackend, GuiCommandRunner, GuiRunOptions,
    GuiRunStatus, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{fs, path::PathBuf, time::Duration};

fn assert_measurements(state: &serde_json::Value) {
    for name in ["variable-pitch", "height", "pixel-space", "emoji"] {
        let values = state[name].as_array().expect("case measured");
        assert_eq!(
            values[0], values[1],
            "{name}: window measurement disagrees with redisplay: {state}"
        );
        assert_eq!(
            values[0], values[2],
            "{name}: string measurement disagrees with redisplay: {state}"
        );
    }
    assert_eq!(state["pixel-space"][0], 50);
    assert_eq!(
        state["long-string"][0].as_i64().unwrap(),
        10 * state["variable-pitch"][2].as_i64().unwrap(),
        "unbounded string width: {state}"
    );
    assert_eq!(state["window-preserved"][0], true);
    assert_eq!(
        state["partial-row"][0].as_i64().unwrap(),
        state["variable-pitch"][0].as_i64().unwrap() * 4 / 10
    );
    assert_eq!(state["height-pixels"][0], state["height-pixels"][1]);
    assert_eq!(
        state["composition-origin"][0], 0,
        "composition FROM: {state}"
    );
    assert_eq!(
        state["composition-origin"][1], state["composition-origin"][2],
        "composition FROM with tail: {state}"
    );
    assert_eq!(state["newline-only"][0], 0, "newline-only width: {state}");
    assert_eq!(state["newline-only"][1], state["newline-only"][2]);
    assert_eq!(
        state["partial-composition"][0], state["partial-composition"][1],
        "partial composition: {state}"
    );
    assert_eq!(
        state["newline-origin"][0], state["newline-origin"][1],
        "newline FROM width: {state}"
    );
    assert_eq!(
        state["newline-origin"][2], state["newline-origin"][3],
        "newline FROM height: {state}"
    );
    assert_eq!(state["source-remapping"][0], state["variable-pitch"][0]);
    assert_eq!(
        state["end-overlay"][0], state["end-overlay"][1],
        "EOB overlay: {state}"
    );
    assert_eq!(
        state["wrapped-origin"][0], state["wrapped-origin"][1],
        "wrapped FROM budget: {state}"
    );
    assert_eq!(state["wrapped-origin"][2], 1);
    assert_eq!(
        state["wrapped-boundary"][0], state["wrapped-boundary"][1],
        "wrapped boundary FROM budget: {state}"
    );
    assert_eq!(state["wrapped-boundary"][2], 1);
    assert_eq!(
        state["y-limit"][0],
        serde_json::json!([state["variable-pitch"][0], 1])
    );
}

#[test]
fn pixel_measurements_agree_with_real_glyph_placement_and_gnu() {
    let root = neomacs_infra::workspace_root();
    let artifacts = root.join(format!("tmp/pixel-measurement-{}", std::process::id()));
    let fixture = root.join("crates/neomacs-gui-tests/fixtures/pixel-measurement/init.el");
    let backend = match std::env::var("NEOMACS_GUI_TEST_BACKEND").as_deref() {
        Ok("wayland") => GuiBackend::LinuxWayland,
        _ => GuiBackend::LinuxX11,
    };
    let gnu_session = DisplayHarness::for_backend(GuiBackend::LinuxX11)
        .start_session(&artifacts.join("gnu"))
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
        ],
    };
    command.env.extend(gnu_session.env().iter().cloned());
    let output = ProcessGuiCommandRunner
        .run(
            &command,
            &GuiArtifactSet::new(&artifacts, GuiBackend::LinuxX11, "gnu"),
            &GuiRunOptions::with_timeout(Duration::from_secs(30)),
        )
        .unwrap();
    assert!(!output.timed_out, "GNU: {output:#?}");
    assert_eq!(output.exit_code, Some(0));
    let gnu: serde_json::Value = serde_json::from_slice(&fs::read(state).unwrap()).unwrap();
    assert_eq!(
        gnu["native-engine"][0], false,
        "GNU probe must run GNU Emacs"
    );
    assert_measurements(&gnu);
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifacts.join("native"))
        .unwrap();
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &artifacts,
        GuiScenario::new("pixel-measurement", &fixture),
    )
    .with_program(binary)
    .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "64");
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
    assert!(!result.timed_out);
    assert_eq!(result.exit_code, Some(0));
    let measurements: serde_json::Value =
        serde_json::from_slice(&fs::read(&result.artifacts.gui_state).unwrap()).unwrap();
    assert_eq!(measurements["native-engine"][0], true);
    assert_measurements(&measurements);
    assert!(result.png_bytes.unwrap_or_default() > 0);
    let frame: serde_json::Value = serde_json::from_slice(
        &fs::read(format!(
            "{}.frame.json",
            result.artifacts.gui_state.display()
        ))
        .unwrap(),
    )
    .unwrap();
    let positions = frame["frames"][0]["presented_hit_index"]["text_positions"]
        .as_array()
        .expect("rendered text positions");
    for (name, start, end) in [
        ("variable-pitch", 1, 11),
        ("height", 13, 16),
        ("pixel-space", 18, 19),
        ("emoji", 21, 24),
    ] {
        let x = |pos| {
            positions
                .iter()
                .find(|point| point["buffer_position"] == pos)
                .expect("rendered fixture position")["bounds"]["x"]
                .as_f64()
                .unwrap()
        };
        assert_eq!(
            x(end) - x(start),
            measurements[name][1].as_f64().unwrap(),
            "{name}: rendered frame disagrees with measurement"
        );
    }
}
