//! Exercise the command loop, redisplay and GPU presentation together.

use std::{
    fs,
    path::Path,
    thread,
    time::{Duration, Instant},
};

use neomacs_gui_tests::{
    DisplayHarness, GuiArtifactSet, GuiRunOptions, GuiRunStatus, GuiScenario, GuiTestPlan,
    ProcessGuiCommandRunner,
};
use serde_json::Value;

#[test]
fn org_banner_survives_repeated_page_scrolling() {
    assert_banner_round_trip("org-banner-scroll", 160);
}

#[test]
fn taller_than_window_org_image_survives_repeated_page_scrolling() {
    assert_banner_round_trip("tall-org-image-scroll", 1000);
}

fn assert_banner_round_trip(scenario: &str, image_height: u32) {
    let Some(backend) = super::requested_backend() else {
        eprintln!("set NEOMACS_GUI_TEST_BACKEND to run the scrolling GUI regression");
        return;
    };
    let root = super::workspace_root();
    // Never accept a screenshot or acknowledgement left by an earlier run.
    let artifacts = neomacs_gui_tests::fresh_artifact_dir(root.join(format!(
        "target/neomacs-gui-tests/{scenario}-{}",
        std::process::id()
    )));
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
            root.join("crates/neomacs-gui-tests/fixtures/page-scrolling.el"),
        ),
    )
    .with_program(super::neomacs_binary(&root))
    .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "10000")
    .with_env("NEOMACS_GUI_SCROLL_IMAGE_HEIGHT", image_height.to_string())
    .with_env(
        "NEOMACS_GUI_SCROLL_CONTROL",
        artifacts.display().to_string(),
    );
    for (key, value) in session.env() {
        plan = plan.with_env(key.clone(), value.clone());
    }

    let (result, presentation) = thread::scope(|scope| {
        let run = scope.spawn(|| {
            plan.run_with(
                &mut ProcessGuiCommandRunner,
                GuiRunOptions::with_timeout(Duration::from_secs(60)),
            )
        });
        let presentation = observe_presentations(&artifacts, &paths, || run.is_finished());
        if presentation.is_err() {
            fs::write(artifacts.join("stop"), "stop").expect("stop failed fixture");
        }
        (run.join().expect("GUI runner thread"), presentation)
    });
    let result = result.expect("GUI artifacts");
    assert!(
        presentation.is_ok(),
        "{presentation:?}; artifacts: {artifacts:?}"
    );
    assert!(!result.timed_out, "{result:#?}");
    assert_eq!(result.exit_code, Some(0), "{result:#?}");
    assert_eq!(result.status, GuiRunStatus::Passed, "{result:#?}");

    let trace: Value = serde_json::from_str(
        &fs::read_to_string(&result.artifacts.gui_state).expect("scroll trace"),
    )
    .expect("JSON scroll trace");
    let trace = trace.as_array().expect("scroll states");
    assert_eq!(
        trace.len(),
        17,
        "initial state and sixteen keyboard commands"
    );
    assert_eq!(trace[0]["start"], 1, "fixture must start at the banner");
    assert!(
        trace[1..9]
            .iter()
            .any(|state| state["start"].as_u64().unwrap() > 1),
        "C-v must actually leave the banner: {trace:#?}"
    );
    if image_height == 1000 {
        assert!(
            trace[1..9]
                .iter()
                .any(|state| state["vscroll"].as_u64().unwrap() > 0),
            "the oversized image must exercise pixel scrolling: {trace:#?}"
        );
    }
    let mut top_visits = 0;
    for state in &trace[9..] {
        assert_eq!(state["key"], "M-v");
        let at_top = state["start"] == 1 && state["vscroll"] == 0;
        if top_visits > 0 {
            assert!(
                at_top,
                "repeated M-v must not bounce away from the top: {trace:#?}"
            );
        }
        top_visits += usize::from(at_top);
    }
    assert!(
        top_visits >= 2,
        "M-v must reach the banner and remain there: {trace:#?}"
    );
}

fn observe_presentations(
    control: &Path,
    paths: &GuiArtifactSet,
    finished: impl Fn() -> bool,
) -> Result<(), String> {
    for (stage, visible) in [("initial", true), ("away", false), ("returned", true)] {
        let deadline = Instant::now() + Duration::from_secs(18);
        let ready = control.join(format!("{stage}.ready"));
        let snapshot = control.join(format!("{stage}.json"));
        let mut last_observation = String::from("no stage snapshot yet");
        loop {
            if ready.exists() {
                let doc: Value = serde_json::from_str(
                    &fs::read_to_string(&snapshot).map_err(|error| format!("{stage}: {error}"))?,
                )
                .map_err(|error| format!("{stage}: {error}"))?;
                let matrix = &doc["frames"][0]["window_matrices"][0]["matrix"];
                let has_image = matrix["rows"]
                    .as_array()
                    .ok_or("missing glyph rows")?
                    .iter()
                    .filter(|row| row["enabled"] == true)
                    .flat_map(|row| row["glyphs"].as_array().into_iter().flatten())
                    .flat_map(|area| area.as_array().into_iter().flatten())
                    .any(|glyph| !glyph["glyph_type"]["Image"].is_null());
                if has_image != visible {
                    return Err(format!(
                        "{stage}: expected image visibility {visible}, snapshot {snapshot:?}"
                    ));
                }
                // The writer can be partway through a PNG. Retry until a full
                // decoded frame has the expected pixels, not for a fixed delay.
                if let Ok(png) = image::open(&paths.png) {
                    let pixels = png.to_rgba8();
                    let orange = pixels
                        .pixels()
                        .filter(|pixel| {
                            let [r, g, b, _] = pixel.0;
                            r > 220 && (60..180).contains(&g) && b < 60
                        })
                        .count();
                    last_observation = format!("{orange} banner-colored pixels");
                    let presentation_matches = if visible {
                        orange > 10_000
                    } else {
                        orange == 0
                    };
                    if presentation_matches {
                        pixels
                            .save(control.join(format!("{stage}.png")))
                            .map_err(|error| error.to_string())?;
                        // The editor cannot advance until the GPU has painted
                        // this stage. In particular, an old initial screenshot
                        // cannot satisfy the final banner assertion after away.
                        fs::write(control.join(format!("{stage}.ack")), "painted")
                            .map_err(|error| error.to_string())?;
                        break;
                    }
                }
            }
            if finished() || Instant::now() >= deadline {
                return Err(format!(
                    "{stage}: presentation did not arrive ({last_observation})"
                ));
            }
            thread::sleep(Duration::from_millis(25));
        }
    }
    Ok(())
}
