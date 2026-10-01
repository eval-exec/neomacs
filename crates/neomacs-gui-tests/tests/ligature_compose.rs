//! Issue #447 GUI verification: a `composition-function-table` rule with
//! `font-shape-gstring` must compose "->" through the font — the rendered
//! surface with the ligature rule enabled differs from the run with the
//! rule stripped (the composition changes at least one cell's pixels), and
//! the row's glyph matrix records the composed glyph.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    GuiBackend, GuiRunOptions, GuiRunStatus, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{path::PathBuf, time::Duration};

fn run_case(tag: &str, ligature_enabled: bool) -> (PathBuf, PathBuf) {
    let root = neomacs_infra::crate_root!().join("../..");
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let artifacts = root.join(format!(
        "tmp/neomacs-gui-tests/ligature-{tag}-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&artifacts).unwrap();
    let ready = artifacts.join("ligature.ready");
    let _ = std::fs::remove_file(&ready);
    let session = neomacs_infra::display::start_sway(
        &artifacts,
        "output * resolution 1280x720\nxwayland disable\nseat seat0 fallback true\n",
    )
    .unwrap();
    let mut plan = GuiTestPlan::new(
        GuiBackend::LinuxWayland,
        &root,
        &artifacts,
        GuiScenario::new(
            &format!("ligature-{tag}"),
            root.join("crates/neomacs-gui-tests/fixtures/ligature-compose.el"),
        ),
    )
    .with_program(binary)
    .with_env("RUST_LOG", "info")
    .with_env(
        "NEOMACS_LIGATURE_ENABLED",
        if ligature_enabled { "1" } else { "0" },
    )
    .with_env("NEOMACS_SELECTION_READY", ready.to_string_lossy())
    .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "10000");
    for (key, value) in session.env() {
        plan = plan.with_env(key.clone(), value.clone());
    }
    // Materialize the env BEFORE moving into the input thread: capturing
    // `session` in the closure would move (and drop, killing sway) the
    // session when the thread finishes — the surface loss that aborted
    // every ligature run at the C-l keypress.
    let env = session.env().to_vec();
    let input = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        while !ready.exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "fixture did not become ready"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        let output = std::process::Command::new(
            std::env::var_os("NEOMACS_GUI_WTYPE").unwrap_or_else(|| "wtype".into()),
        )
        .envs(env)
        .args([
            "-d", "300", "-M", "ctrl", "-k", "l", "-k", "c", "-m", "ctrl", "-k", "t",
        ])
        .output()
        .expect("install wtype");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    });
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(25)),
        )
        .unwrap();
    input.join().unwrap();
    assert!(!result.timed_out, "{result:#?}");
    assert_eq!(result.exit_code, Some(0), "{result:#?}");
    assert_eq!(result.status, GuiRunStatus::Passed, "{result:#?}");
    (
        result.artifacts.frame_snapshot_json.clone(),
        result.artifacts.frame_snapshot_json,
    )
}

#[test]
fn ligature_rule_composes_on_the_rendered_surface() {
    // Control: the run with the rule stripped must boot and render cleanly
    // (it did before #447's driver). If the ENABLED run crashes where this
    // passes, the crash is in the composition render path.
    let (_disabled_snapshot, disabled_png) = run_case("off", false);
    let (enabled_snapshot, enabled_png) = run_case("on", true);
    // The composed glyph must be recorded in the row's matrix.
    let snapshot: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&enabled_snapshot).unwrap()).unwrap();
    let rows = snapshot["frames"][0]["window_matrices"][0]["matrix"]["rows"]
        .as_array()
        .expect("matrix rows");
    let composed = rows
        .iter()
        .filter_map(|row| row["glyphs"][1].as_array())
        .flatten()
        .any(|glyph| {
            glyph["glyph_type"]["Composite"]["text"]
                .as_str()
                .is_some_and(|text| text.contains("->"))
        });
    assert!(
        composed,
        "the matrix must record the composed '->' glyph: {snapshot}"
    );

    // The rendered surface with the ligature rule must differ from the run
    // without it (the composed glyph changes at least one cell).
    let enabled = image::open(format!("{}.png", enabled_png.display()))
        .unwrap()
        .to_rgba8();
    let disabled = image::open(format!("{}.png", disabled_png.display()))
        .unwrap()
        .to_rgba8();
    assert_eq!(enabled.dimensions(), disabled.dimensions());
    let differing = enabled
        .pixels()
        .zip(disabled.pixels())
        .filter(|(a, b)| a != b)
        .count();
    assert!(
        differing > 0,
        "the ligature rule must change the rendered surface"
    );
}
