//! Issue #447 GUI verification: a `composition-function-table` rule with
//! `font-shape-gstring` must compose "->" through the font — the row's glyph
//! matrix records the AutomaticComposite glyph (one width-carrying base cell
//! plus zero-width member cells, like GNU's terminal composer), and the
//! matrix differs from the run with the rule stripped.
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
    .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "10000")
    .with_env("NEOMACS_TRACE_COMPOSITION", "1");
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
    let (disabled_snapshot, _disabled_png) = run_case("off", false);
    let (enabled_snapshot, _enabled_png) = run_case("on", true);
    // The composed glyph must be recorded in the row's matrix as an
    // AutomaticComposite whose terminal decomposition spans the whole rule
    // match: GNU's terminal composer writes the base cell with cmp->width and
    // zero-width member cells for the rest of the run (src/term.c
    // produce_composition_glyph), so "->" is one 2-cell base plus a padding
    // member cell — not two independent chars.
    let snapshot: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&enabled_snapshot).unwrap()).unwrap();
    let rows = snapshot["frames"][0]["window_matrices"][0]["matrix"]["rows"]
        .as_array()
        .expect("matrix rows");
    let mut composed = false;
    for row in rows.iter().filter_map(|row| row["glyphs"][1].as_array()) {
        for (index, glyph) in row.iter().enumerate() {
            let Some(text) = glyph["glyph_type"]["AutomaticComposite"]["text"]
                .as_str()
                .filter(|text| text.contains("->"))
            else {
                continue;
            };
            let base_width = glyph["pixel_width"].as_f64().unwrap_or(0.0);
            let members = row[index + 1..]
                .iter()
                .take(text.chars().count().saturating_sub(1))
                .collect::<Vec<_>>();
            let members_carry_the_rest = members.iter().all(|member| {
                member["padding"].as_bool() == Some(true)
                    && member["pixel_width"].as_f64() == Some(0.0)
            });
            assert!(
                members_carry_the_rest,
                "the composed run's remaining columns must be zero-width member cells: {row:?}"
            );
            assert!(
                base_width > 0.0,
                "the composed glyph's base cell must carry the run width"
            );
            composed = true;
        }
    }
    assert!(
        composed,
        "the matrix must record the composed '->' glyph: {snapshot}"
    );

    // The MATRIX with the ligature rule must differ from the run without it.
    // (The pixels deliberately stay identical: DejaVu Sans Mono has no '->'
    // ligature, so GNU renders the same cells either way — the contract is
    // the composition GLYPH, which is what a real ligature font would paint
    // differently.)
    let disabled: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&disabled_snapshot).unwrap()).unwrap();
    assert_ne!(
        snapshot["frames"][0]["window_matrices"][0]["matrix"]["rows"],
        disabled["frames"][0]["window_matrices"][0]["matrix"]["rows"],
        "the ligature rule must change the recorded row matrix"
    );
}
