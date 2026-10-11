//! Finite SVG image.el playback, compression and XML namespace detection.

#![cfg(target_os = "linux")]

use std::{
    fs,
    path::Path,
    thread,
    time::{Duration, Instant},
};

use neomacs_gui_tests::{
    DisplayHarness, GuiArtifactSet, GuiBackend, GuiRunOptions, GuiRunStatus, GuiScenario,
    GuiTestPlan, ProcessGuiCommandRunner,
};
use serde_json::Value;

#[test]
fn finite_svg_and_namespaced_svgz_stop_at_the_static_final_presentation() {
    run_svg_scenario("svg-animation", "svg-animation.el", false);
}

#[test]
fn delayed_svg_shows_its_introduction_once_then_repeats_the_steady_cycle() {
    run_svg_scenario("svg-introduction", "svg-animation-introduction.el", true);
}

fn run_svg_scenario(scenario: &str, fixture: &str, introduction: bool) {
    let backend = match std::env::var("NEOMACS_GUI_TEST_BACKEND").as_deref() {
        Ok("wayland" | "linux-wayland") => GuiBackend::LinuxWayland,
        Ok("x11" | "linux-x11") | Err(_) => GuiBackend::LinuxX11,
        Ok(other) => panic!("unsupported NEOMACS_GUI_TEST_BACKEND={other:?}"),
    };
    let root = neomacs_infra::workspace_root();
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    assert!(binary.exists(), "fresh GUI binary required: {binary:?}");
    let run_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let artifacts = root.join(format!(
        "tmp/svg-animation-gui-{}-{run_id}",
        std::process::id()
    ));
    fs::create_dir_all(root.join("tmp")).expect("GUI artifact parent");
    fs::create_dir(&artifacts).expect("fresh SVG GUI artifacts");
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifacts)
        .expect("start GUI display");
    let paths = GuiArtifactSet::new(&artifacts, backend, scenario);
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &artifacts,
        GuiScenario::new(
            scenario,
            root.join("crates/neomacs-gui-tests/fixtures").join(fixture),
        ),
    )
    .with_program(binary)
    .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "10000")
    .with_env(
        "NEOMACS_GUI_SVG_ANIMATION_CONTROL",
        artifacts.display().to_string(),
    );
    for (key, value) in session.env() {
        plan = plan.with_env(key.clone(), value.clone());
    }
    let (result, presentation) = thread::scope(|scope| {
        let run = scope.spawn(|| {
            plan.run_with(
                &mut ProcessGuiCommandRunner,
                GuiRunOptions::with_timeout(Duration::from_secs(40)),
            )
        });
        let presentation =
            observe_presentations(&artifacts, &paths, introduction, || run.is_finished());
        if presentation.is_err() {
            fs::write(artifacts.join("stop"), "stop").expect("stop failed fixture");
        }
        (run.join().expect("GUI runner thread"), presentation)
    });
    assert!(
        presentation.is_ok(),
        "{presentation:?}; artifacts: {artifacts:?}"
    );
    let result = result.expect("GUI artifacts");
    assert!(!result.timed_out, "{result:#?}");
    assert_eq!(result.exit_code, Some(0), "{result:#?}");
    assert_eq!(result.status, GuiRunStatus::Passed, "{result:#?}");
}

fn observe_presentations(
    control: &Path,
    paths: &GuiArtifactSet,
    introduction: bool,
    finished: impl Fn() -> bool,
) -> Result<(), String> {
    for (stage, expected_index) in [("initial", 0), ("final", 2)] {
        let deadline = Instant::now() + Duration::from_secs(18);
        let mut last_observation = "no stage snapshot".to_owned();
        loop {
            if control.join(format!("{stage}.ready")).exists() {
                let state: Value = serde_json::from_str(
                    &fs::read_to_string(control.join(format!("{stage}-state.json")))
                        .map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())?;
                let images = state.as_array().ok_or("missing animation metadata array")?;
                if images.len() != if introduction { 1 } else { 2 }
                    || images.iter().any(|image| {
                        image["count"] != if introduction { 4 } else { 3 }
                            || image["delay"] != 0.5
                            || image["index"] != expected_index
                            || (introduction && image["loop-start"] != 2)
                    })
                {
                    return Err(format!("{stage}: unexpected count/delay/index: {state}"));
                }
                if introduction && stage == "final" {
                    let trace: Value = serde_json::from_str(
                        &fs::read_to_string(control.join("playback.json"))
                            .map_err(|error| error.to_string())?,
                    )
                    .map_err(|error| error.to_string())?;
                    if trace != serde_json::json!([0, 1, 2, 3, 2]) {
                        return Err(format!("introduction must play once: {trace}"));
                    }
                }
                let snapshot: Value = serde_json::from_str(
                    &fs::read_to_string(control.join(format!("{stage}.json")))
                        .map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())?;
                let image_glyphs = snapshot["frames"][0]["window_matrices"][0]["matrix"]["rows"]
                    .as_array()
                    .ok_or("missing glyph rows")?
                    .iter()
                    .filter(|row| row["enabled"] == true)
                    .flat_map(|row| row["glyphs"].as_array().into_iter().flatten())
                    .flat_map(|area| area.as_array().into_iter().flatten())
                    .filter(|glyph| !glyph["glyph_type"]["Image"].is_null())
                    .count();
                let expected_glyphs = if introduction { 2 } else { 3 };
                if image_glyphs != expected_glyphs {
                    return Err(format!(
                        "{stage}: expected {expected_glyphs} image glyphs, got {image_glyphs}"
                    ));
                }
                // Readback files can be mid-write. Retry complete PNGs until
                // the GPU paints this stage, then permit fixture advancement.
                if let Ok(png) = image::open(&paths.png) {
                    let pixels = png.to_rgba8();
                    let mut colors = [0usize; 3];
                    for pixel in pixels.pixels() {
                        let [r, g, b, _] = pixel.0;
                        colors[0] += usize::from(r > 240 && g < 15 && b < 15);
                        colors[1] += usize::from(g > 240 && r < 15 && b < 15);
                        colors[2] += usize::from(b > 240 && r < 15 && g < 15);
                    }
                    last_observation = format!("red/green/blue pixels: {colors:?}");
                    let matches = if stage == "initial" {
                        if introduction {
                            colors[0] > 6_000 && colors[1] == 0 && colors[2] > 6_000
                        } else {
                            colors.iter().all(|count| *count > 6_000)
                        }
                    } else {
                        colors[0] == 0
                            && colors[1] == 0
                            && colors[2] > if introduction { 12_000 } else { 18_000 }
                    };
                    if matches {
                        pixels
                            .save(control.join(format!("{stage}.png")))
                            .map_err(|error| error.to_string())?;
                        fs::write(control.join(format!("{stage}.ack")), "painted")
                            .map_err(|error| error.to_string())?;
                        break;
                    }
                }
            }
            if finished() || Instant::now() >= deadline {
                return Err(format!("{stage}: presentation absent ({last_observation})"));
            }
            thread::sleep(Duration::from_millis(25));
        }
    }
    Ok(())
}
