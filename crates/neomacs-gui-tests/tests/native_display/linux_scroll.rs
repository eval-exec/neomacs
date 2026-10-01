//! Compare actual text displacement on owned Linux displays, including 2x output.

use image::RgbaImage;
use neomacs_gui_tests::{
    DisplayHarness, GuiArtifactSet, GuiBackend, GuiRunOptions, GuiRunResult, GuiScenario,
    GuiTestPlan, ProcessGuiCommandRunner, WaylandOutput,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

pub(super) fn run_rich_pixel_contracts() -> Vec<GuiRunResult> {
    [
        ("x11", GuiBackend::LinuxX11, DisplayHarness::Xvfb, 1),
        (
            "wayland",
            GuiBackend::LinuxWayland,
            DisplayHarness::WestonHeadless(WaylandOutput::Standard),
            1,
        ),
        (
            "wayland-hidpi",
            GuiBackend::LinuxWayland,
            DisplayHarness::WestonHeadless(WaylandOutput::HiDpi4k),
            2,
        ),
    ]
    .into_iter()
    .map(|(name, backend, display, scale)| run_rich_pixel_contract(name, backend, display, scale))
    .collect()
}

fn run_rich_pixel_contract(
    name: &str,
    backend: GuiBackend,
    display: DisplayHarness,
    scale: u32,
) -> GuiRunResult {
    let root = neomacs_infra::workspace_root();
    let control = root.join("target/neomacs-gui-tests").join(format!(
        "native-display-scroll-{name}-{}",
        std::process::id()
    ));
    assert!(!control.exists(), "artifact directory must be fresh");
    let session = display
        .start_session(&control)
        .expect("owned Linux display");
    let paths = GuiArtifactSet::new(&control, backend, name);
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &control,
        GuiScenario::new(
            name,
            root.join("crates/neomacs-gui-tests/fixtures/native-scroll-contract.el"),
        ),
    )
    .with_program(binary)
    .with_env("RUST_LOG", "info")
    .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "10000")
    .with_env("NEOMACS_GUI_SCROLL_CONTROL", control.display().to_string());
    for (key, value) in session.env() {
        plan = plan.with_env(key, value);
    }

    let (result, presentation) = thread::scope(|scope| {
        let run = scope.spawn(|| {
            plan.run_with(
                &mut ProcessGuiCommandRunner,
                GuiRunOptions::with_timeout(Duration::from_secs(60)),
            )
        });
        let presentation =
            observe_pixel_displacement(&control, &paths, scale, || run.is_finished());
        if presentation.is_err() {
            fs::write(control.join("stop"), "stop").expect("stop failed rendering fixture");
        }
        (run.join().expect("GUI runner thread"), presentation)
    });
    let result = result.expect("rich scroll GUI artifacts");
    assert!(
        presentation.is_ok(),
        "{presentation:?}; artifacts: {control:?}"
    );
    assert!(!result.timed_out, "{result:#?}");
    assert_eq!(result.exit_code, Some(0), "{result:#?}");
    result
}

fn observe_pixel_displacement(
    control: &Path,
    paths: &GuiArtifactSet,
    scale: u32,
    finished: impl Fn() -> bool,
) -> Result<(), String> {
    let (origin, origin_state) = capture_stage(control, paths, "origin", None, &finished)?;
    check_scale(&origin, &origin_state, scale)?;
    fs::write(control.join("origin.ack"), "painted").map_err(|error| error.to_string())?;
    let (down, down_state) = capture_stage(control, paths, "down", Some(&origin), &finished)?;
    check_scale(&down, &down_state, scale)?;
    if origin.dimensions() != down.dimensions() {
        return Err("frame resized during the pixel scroll".into());
    }

    // Measure displacement independently of the Lisp scroll state. Including
    // zero, adjacent pixels, and twice the requested distance rejects stale
    // rendering, rounding, whole-line movement, and applying HiDPI twice.
    let scores: Vec<f64> = (0..=21 * scale)
        .map(|offset| translation_error(&origin, &down, offset, 21 * scale))
        .collect();
    let best = (0..scores.len())
        .min_by(|&a, &b| scores[a].total_cmp(&scores[b]))
        .unwrap();
    let expected = 7 * scale;
    let report = json!({
        "scale": scale,
        "expected-physical-shift": expected,
        "observed-physical-shift": best,
        "mean-rgb-errors": scores,
        "origin": origin_state,
        "down": down_state,
    });
    fs::write(
        control.join("rendered-distance.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .map_err(|error| error.to_string())?;
    if best != expected as usize || scores[best] >= scores[0] {
        return Err(format!(
            "seven logical pixels rendered at the wrong distance: {report}"
        ));
    }
    fs::write(control.join("down.ack"), "painted").map_err(|error| error.to_string())?;
    Ok(())
}

fn check_scale(image: &RgbaImage, state: &Value, expected: u32) -> Result<(), String> {
    let width = state["width"].as_u64().ok_or("missing Lisp frame width")?;
    let height = state["height"]
        .as_u64()
        .ok_or("missing Lisp frame height")?;
    if image.width() as u64 != width * expected as u64
        || image.height() as u64 != height * expected as u64
    {
        return Err(format!(
            "expected {expected}x physical readback of {width}x{height}, got {:?}",
            image.dimensions()
        ));
    }
    Ok(())
}

fn capture_stage(
    control: &Path,
    paths: &GuiArtifactSet,
    stage: &str,
    origin: Option<&RgbaImage>,
    finished: &impl Fn() -> bool,
) -> Result<(RgbaImage, Value), String> {
    let deadline = Instant::now() + Duration::from_secs(18);
    let origin_sample = origin.map(body_sample);
    let mut previous = Vec::new();
    let mut stable = 0;
    loop {
        // A complete ready file reports only public Lisp positions/dimensions.
        // It never invokes redisplay or captures a pre-command frame snapshot.
        if let Ok(bytes) = fs::read(control.join(format!("{stage}.ready")))
            && let Ok(state) = serde_json::from_slice::<Value>(&bytes)
            && let Ok(image) = image::open(&paths.png)
        {
            let image = image.to_rgba8();
            let sample = body_sample(&image);
            // Startup's scratch presentation cannot pass as the rich buffer.
            // Wait for an actual changed body after down, not cursor movement.
            let rich = sample
                .chunks_exact(3)
                .filter(|rgb| {
                    (rgb[0] > 190 && rgb[1] > 130 && rgb[2] < 90)
                        || (rgb[0] < 90 && rgb[1] > 170 && rgb[2] > 170)
                })
                .count()
                > 25;
            let changed = origin_sample
                .as_ref()
                .is_none_or(|initial| *initial != sample);
            if rich && changed {
                stable = if sample == previous { stable + 1 } else { 1 };
                previous = sample;
                if stable >= 3 {
                    image
                        .save(control.join(format!("{stage}.png")))
                        .map_err(|error| error.to_string())?;
                    return Ok((image, state));
                }
            } else {
                stable = 0;
            }
        }
        if finished() || Instant::now() >= deadline {
            return Err(format!(
                "no stable rich {stage} presentation before deadline"
            ));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn body_sample(image: &RgbaImage) -> Vec<u8> {
    // Interior text excludes the cursor, fringes, clipping edges, and mode line.
    let mut sample = Vec::new();
    for y in (image.height() * 15 / 100..image.height() * 65 / 100).step_by(3) {
        for x in (image.width() * 5 / 100..image.width() * 80 / 100).step_by(5) {
            sample.extend_from_slice(&image.get_pixel(x, y).0[..3]);
        }
    }
    sample
}

fn translation_error(origin: &RgbaImage, down: &RgbaImage, offset: u32, max: u32) -> f64 {
    let mut error = 0_u64;
    let mut count = 0_u64;
    let bottom = (origin.height() * 65 / 100).min(origin.height().saturating_sub(max));
    for y in (origin.height() * 15 / 100..bottom).step_by(3) {
        for x in (origin.width() * 5 / 100..origin.width() * 80 / 100).step_by(5) {
            let before = origin.get_pixel(x, y + offset);
            let after = down.get_pixel(x, y);
            for channel in 0..3 {
                error += before[channel].abs_diff(after[channel]) as u64;
                count += 1;
            }
        }
    }
    error as f64 / count as f64
}
